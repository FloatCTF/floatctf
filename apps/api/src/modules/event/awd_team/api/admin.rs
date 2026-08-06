//! AWD admin API handlers.
//!
//! All endpoints require SuperAdmin authentication.

use actix_web::{HttpResponse, web};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};
use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard, prelude::*},
    modules::event::awd_team::{
        domain::AwdEventStatusExt,
        repo::{ban_repo, event_repo, round_repo},
        scheduler::schedule_auto_precheck,
        service::{event_service, score_service},
    },
};

use super::dto::*;

use actix_web::{delete, get, post, put};

// ── Event Management ──

/// POST /api/admin/events/awd
#[post("/events/awd")]
pub async fn create_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    body: web::Json<CreateAwdEventRequest>,
) -> UniResult<Uuid> {
    use crate::modules::event::awd_team::crypto::AwdCrypto;

    let b = body.into_inner();
    let event = crate::entity::events::Entity::find_by_id(b.event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Event {} not found", b.event_id)))?;
    if event.r#type != crate::entity::sea_orm_active_enums::EventType::AwdTeam {
        return Err(AppError::BadRequest(format!(
            "Event {} is not an AWD team Event",
            event.id
        )));
    }
    let event_id = event.id;

    // Initialize crypto for token encryption
    let crypto = AwdCrypto::from_env_secret().map_err(|e| AppError::Internal(e.to_string()))?;

    // Generate and encrypt event secret
    let event_secret = AwdCrypto::generate_event_secret();
    let secret_aad = AwdCrypto::build_aad(event_id, "event_secret");
    let secret_blob = crypto
        .encrypt(&event_secret, &secret_aad, 1)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Generate and encrypt internal tokens
    let fs_token = AwdCrypto::generate_token();
    let fs_aad = AwdCrypto::build_aad(event_id, "internal_token");
    let fs_blob = crypto
        .encrypt(&fs_token, &fs_aad, 1)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let js_token = AwdCrypto::generate_token();
    let js_aad = AwdCrypto::build_aad(event_id, "internal_token");
    let js_blob = crypto
        .encrypt(&js_token, &js_aad, 1)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Create the awd_events record
    use crate::entity::awd_events;
    let model = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        gamebox_cidr: Set(b.gamebox_cidr),
        wireguard_cidr: Set(b.wireguard_cidr),
        wireguard_interface_name: Set(b.wireguard_interface_name),
        wireguard_listen_port: Set(b.wireguard_listen_port),
        flagserver_ip: Set(b.flagserver_ip),
        judgeserver_ip: Set(b.judgeserver_ip),
        round_duration_secs: Set(b.round_duration_secs),
        event_secret_ciphertext: Set(secret_blob.ciphertext),
        event_secret_nonce: Set(secret_blob.nonce),
        flagserver_token_ciphertext: Set(Some(fs_blob.ciphertext)),
        flagserver_token_nonce: Set(Some(fs_blob.nonce)),
        judgeserver_token_ciphertext: Set(Some(js_blob.ciphertext)),
        judgeserver_token_nonce: Set(Some(js_blob.nonce)),
        ..Default::default()
    };

    let txn = ctx.db.begin().await?;
    model
        .insert(&txn)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    schedule_auto_precheck(&txn, event_id, event.start_time, chrono::Utc::now()).await?;
    txn.commit().await?;

    UniResponse::ok(event_id.into()).into()
}

/// POST /api/admin/events/{event_id}/awd/start
#[post("/events/{event_id}/awd/start")]
pub async fn start_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    event_service::start_event(ctx.db.get_ref(), awd.network.as_ref(), event_id)
        .await
        .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awd/pause
#[post("/events/{event_id}/awd/pause")]
pub async fn pause_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    event_service::pause_event(ctx.db.get_ref(), awd.network.as_ref(), event_id)
        .await
        .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awd/resume
#[post("/events/{event_id}/awd/resume")]
pub async fn resume_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    event_service::resume_event(ctx.db.get_ref(), awd.network.as_ref(), event_id)
        .await
        .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awd/finish
#[post("/events/{event_id}/awd/finish")]
pub async fn finish_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    event_service::finish_event(ctx.db.get_ref(), event_id)
        .await
        .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

// ── Ban Management ──

/// POST /api/admin/events/{event_id}/awd/teams/{team_id}/ban
#[post("/events/{event_id}/awd/teams/{team_id}/ban")]
pub async fn ban_team(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<BanTeamRequest>,
) -> UniResult<Uuid> {
    let (event_id, team_id) = path.into_inner();
    let admin_id = _admin.into_inner().id;

    // Find active round for effective_round_id
    let active_round = round_repo::find_active_round(ctx.db.get_ref(), event_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let ban = ban_repo::create_ban(
        ctx.db.get_ref(),
        event_id,
        team_id,
        body.reason.as_deref(),
        active_round.map(|r| r.id),
        Some(admin_id),
    )
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    UniResponse::ok(ban.id.into()).into()
}

/// DELETE /api/admin/events/{event_id}/awd/teams/{team_id}/ban
#[delete("/events/{event_id}/awd/teams/{team_id}/ban")]
pub async fn unban_team(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (event_id, team_id) = path.into_inner();
    let admin_id = _admin.into_inner().id;

    let ban = ban_repo::find_active_ban(ctx.db.get_ref(), event_id, team_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("No active ban found".into()))?;

    // Find next round for effective unban
    let latest = round_repo::find_latest_round(ctx.db.get_ref(), event_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    if let Some(round) = latest {
        ban_repo::request_unban(ctx.db.get_ref(), ban.id, round.id)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
    }

    // For MVP: complete unban immediately
    ban_repo::complete_unban(ctx.db.get_ref(), ban.id, Some(admin_id))
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    UniResponse::ok_none().into()
}

// ── Score Adjustment ──

/// POST /api/admin/events/{event_id}/awd/score/adjust
#[post("/events/{event_id}/awd/score/adjust")]
pub async fn adjust_score(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: web::Json<ScoreAdjustRequest>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    let admin_id = _admin.into_inner().id;

    score_service::record_adjustment(
        ctx.db.get_ref(),
        event_id,
        body.team_id,
        body.delta,
        &body.reason,
        admin_id,
    )
    .await
    .map_err(AppError::from)?;

    UniResponse::ok_none().into()
}

// ── GameBox Management ──

// ── Deployment ──

/// POST /api/admin/events/{event_id}/awd/deploy
#[post("/events/{event_id}/awd/deploy")]
pub async fn deploy_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    crate::modules::event::awd_team::service::deploy_service::deploy_event(
        ctx.db.get_ref(),
        awd.containers.as_ref(),
        awd.network.as_ref(),
        awd.crypto.as_ref(),
        event_id,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// GET /api/admin/events/{event_id}/awd/scores
#[get("/events/{event_id}/awd/scores")]
pub async fn get_event_scores(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<crate::modules::event::awd_team::domain::TeamScore>> {
    let event_id = path.into_inner();

    use crate::entity::event_teams;
    let teams = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .all(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let team_info: Vec<(uuid::Uuid, String)> = teams.into_iter().map(|t| (t.id, t.name)).collect();

    let scores = crate::modules::event::awd_team::service::score_service::get_scoreboard(
        ctx.db.get_ref(),
        event_id,
        &team_info,
    )
    .await
    .map_err(AppError::from)?;

    UniResponse::ok(scores.into()).into()
}

// ── Precheck ──

/// POST /api/admin/events/{event_id}/awd/precheck
#[post("/events/{event_id}/awd/precheck")]
pub async fn run_precheck(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Uuid> {
    let event_id = path.into_inner();
    let run_id = crate::modules::event::awd_team::service::precheck_service::run_precheck(
        ctx.db.get_ref(),
        event_id,
        "manual",
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(run_id.into()).into()
}

// ── Reset (admin) ──

/// POST /api/admin/events/{event_id}/awd/gameboxes/{instance_id}/reset
#[post("/events/{event_id}/awd/gameboxes/{instance_id}/reset")]
pub async fn admin_reset_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (event_id, instance_id) = path.into_inner();
    crate::modules::event::awd_team::service::reset_service::execute_reset(
        ctx.db.get_ref(),
        awd.containers.as_ref(),
        crate::modules::event::awd_team::service::reset_service::ResetContext {
            event_id,
            instance_id,
            team_id: uuid::Uuid::nil(),
            requested_by: uuid::Uuid::nil(),
            is_free: true,
        },
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

// ── Missing admin endpoints from plan ──

/// GET /api/admin/events/{event_id}/awd/prechecks
#[get("/events/{event_id}/awd/prechecks")]
pub async fn get_prechecks(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<PrecheckRunDto>> {
    let event_id = path.into_inner();
    let runs = crate::entity::awd_precheck_runs::Entity::find()
        .filter(crate::entity::awd_precheck_runs::Column::EventId.eq(event_id))
        .order_by_desc(crate::entity::awd_precheck_runs::Column::StartedAt)
        .all(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let dtos: Vec<PrecheckRunDto> = runs
        .into_iter()
        .map(|r| PrecheckRunDto {
            id: r.id,
            event_id: r.event_id,
            status: format!("{:?}", r.status),
            trigger: Some(r.trigger),
            revision: r.revision,
            error_msg: r.error_msg,
            started_at: Some(r.started_at.to_rfc3339()),
            completed_at: r.completed_at.map(|t| t.to_rfc3339()),
        })
        .collect();

    UniResponse::ok(dtos.into()).into()
}

/// GET /api/admin/events/{event_id}/awd/judge
#[get("/events/{event_id}/awd/judge")]
pub async fn get_judge_batches(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<JudgeBatchDto>> {
    let event_id = path.into_inner();
    let batches = crate::entity::awd_judge_batches::Entity::find()
        .filter(crate::entity::awd_judge_batches::Column::EventId.eq(event_id))
        .order_by_desc(crate::entity::awd_judge_batches::Column::CreatedAt)
        .all(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let dtos: Vec<JudgeBatchDto> = batches
        .into_iter()
        .map(|b| JudgeBatchDto {
            id: b.id,
            event_id: b.event_id,
            round_id: Some(b.round_id),
            total_tasks: b.total_tasks,
            completed_tasks: b.completed_tasks,
            failed_tasks: b.failed_tasks,
            status: format!("{:?}", b.status),
            created_at: Some(b.created_at.to_rfc3339()),
        })
        .collect();

    UniResponse::ok(dtos.into()).into()
}

/// POST /api/admin/events/{event_id}/awd/archive
#[post("/events/{event_id}/awd/archive")]
pub async fn archive_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    crate::modules::event::awd_team::service::archive_service::archive_event(
        ctx.db.get_ref(),
        awd.containers.as_ref(),
        awd.network.as_ref(),
        event_id,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awd/tokens/rotate
#[post("/events/{event_id}/awd/tokens/rotate")]
pub async fn rotate_tokens(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    use crate::entity::awd_events;
    use crate::modules::event::awd_team::crypto::AwdCrypto;

    let event_id = path.into_inner();

    // Initialize crypto
    let crypto = AwdCrypto::from_env_secret().map_err(|e| AppError::Internal(e.to_string()))?;

    // Generate new tokens
    let fs_token = AwdCrypto::generate_token();
    let js_token = AwdCrypto::generate_token();

    // Encrypt tokens
    let fs_aad = AwdCrypto::build_aad(event_id, "internal_token");
    let fs_blob = crypto
        .encrypt(&fs_token, &fs_aad, 1)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let js_aad = AwdCrypto::build_aad(event_id, "internal_token");
    let js_blob = crypto
        .encrypt(&js_token, &js_aad, 1)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Update the event with new encrypted tokens
    let mut active: awd_events::ActiveModel = awd_events::ActiveModel {
        id: Set(event_id),
        ..Default::default()
    };
    active.flagserver_token_ciphertext = Set(Some(fs_blob.ciphertext));
    active.flagserver_token_nonce = Set(Some(fs_blob.nonce));
    active.judgeserver_token_ciphertext = Set(Some(js_blob.ciphertext));
    active.judgeserver_token_nonce = Set(Some(js_blob.nonce));

    active
        .update(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    // Create rotation audit record
    let rotation = crate::entity::awd_internal_token_rotations::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        token_type: Set("all".to_string()),
        ..Default::default()
    };
    rotation
        .insert(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    UniResponse::ok_none().into()
}

/// PUT /api/admin/events/{event_id}/awd/network
#[put("/events/{event_id}/awd/network")]
pub async fn update_network(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: web::Json<super::dto::NetworkUpdateRequest>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    let awd_event = event_repo::find_by_event_id(ctx.db.get_ref(), event_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("AWD event not found".into()))?;

    if !awd_event.status.is_configurable() {
        return Err(AppError::BadRequest(
            "Cannot update network in current status".into(),
        ));
    }

    // Check if network is locked (deployed or later)
    if awd_event.docker_network_id.is_some() {
        return Err(AppError::BadRequest(
            "Network is locked after first deployment".into(),
        ));
    }

    let mut active: crate::entity::awd_events::ActiveModel =
        crate::entity::awd_events::ActiveModel {
            id: Set(event_id),
            ..Default::default()
        };

    if let Some(cidr) = &body.gamebox_cidr {
        active.gamebox_cidr = Set(cidr.clone());
    }
    if let Some(cidr) = &body.wireguard_cidr {
        active.wireguard_cidr = Set(cidr.clone());
    }
    if let Some(name) = &body.wireguard_interface_name {
        active.wireguard_interface_name = Set(name.clone());
    }
    if let Some(port) = body.wireguard_listen_port {
        active.wireguard_listen_port = Set(port);
    }
    if let Some(ip) = &body.flagserver_ip {
        active.flagserver_ip = Set(ip.clone());
    }
    if let Some(ip) = &body.judgeserver_ip {
        active.judgeserver_ip = Set(ip.clone());
    }

    // Config change clears verified
    active.verified_at = Set(None);
    active.verified_revision = Set(None);
    active.status = Set(crate::entity::sea_orm_active_enums::AwdEventStatus::Configuring);

    active
        .update(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    UniResponse::ok_none().into()
}
