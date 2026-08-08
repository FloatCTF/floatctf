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
        scheduler::{schedule_auto_precheck, schedule_event_start},
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
    let crypto = AwdCrypto::from_config_secret().map_err(|e| AppError::Internal(e.to_string()))?;

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

    // ── P1-14：跨赛事 CIDR / IP 重叠校验（提交前）──
    // 当前赛事尚未入库：存量 awd_events / awd_team_networks 全部视为“其他赛事”，
    // 任何重叠/端口 IP 落入即拒绝创建。
    use crate::entity::{awd_events, awd_team_networks};
    let existing_events = awd_events::Entity::find()
        .all(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let existing_networks = awd_team_networks::Entity::find()
        .all(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    crate::modules::event::awd_team::service::deploy_service::validate_no_cross_event_overlap(
        &existing_events,
        &existing_networks,
        &b.gamebox_cidr,
        &b.wireguard_cidr,
        &b.flagserver_ip,
        &b.judgeserver_ip,
    )
    .map_err(AppError::from)?;

    // Create the awd_events record
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
    schedule_event_start(&txn, event_id, b.planned_start_at).await?;
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
    event_service::start_event(
        ctx.db.get_ref(),
        awd.network.as_ref(),
        awd.firewall.as_ref(),
        awd.publisher.as_ref(),
        event_id,
    )
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
    event_service::pause_event(
        ctx.db.get_ref(),
        awd.network.as_ref(),
        awd.firewall.as_ref(),
        event_id,
    )
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
    event_service::resume_event(
        ctx.db.get_ref(),
        awd.network.as_ref(),
        awd.firewall.as_ref(),
        event_id,
    )
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
///
/// P4-5 跨层闭环：DB ban → WG host 挂起（DB 保持 Active）→ banned set reconcile
/// → conntrack 清理 → publish。duration_secs 设置时创建自动解封任务（P4-7）。
#[post("/events/{event_id}/awd/teams/{team_id}/ban")]
pub async fn ban_team(
    admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<BanTeamRequest>,
) -> UniResult<Uuid> {
    let (event_id, team_id) = path.into_inner();
    let admin_id = admin.into_inner().id;

    let ban_id = crate::modules::event::awd_team::service::ban_service::ban_team(
        ctx.db.get_ref(),
        awd.network.as_ref(),
        awd.firewall.as_ref(),
        awd.publisher.as_ref(),
        event_id,
        team_id,
        body.reason.as_deref(),
        Some(admin_id),
    )
    .await
    .map_err(AppError::from)?;

    // P4-7：duration 到期自动解封任务
    if let Some(duration_secs) = body.duration_secs {
        if duration_secs > 0 {
            let execute_at = chrono::Utc::now() + chrono::Duration::seconds(duration_secs);
            schedule_team_unban(
                ctx.db.get_ref(),
                event_id,
                ban_id,
                execute_at,
            )
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        }
    }

    // P5-11 审计
    awd.audit
        .record(
            crate::infrastructure::audit::AuditAction::TeamBanned,
            &format!("AWD team {team_id} banned in event {event_id}"),
            serde_json::json!({ "event_id": event_id, "team_id": team_id }),
            None,
            Some(admin_id),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(ban_id.into()).into()
}

/// 创建自动解封一次性任务（P4-7）。
async fn schedule_team_unban(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    ban_id: Uuid,
    execute_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sea_orm::DbErr> {
    use crate::entity::scheduled_tasks;
    use sea_orm::ActiveValue::Set;
    let now = chrono::Utc::now();
    scheduled_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        group_id: Set(Some(event_id)),
        task_name: Set(format!("AWD auto-unban ban {ban_id}")),
        description: Set(Some("automatic unban after ban duration".into())),
        task_key: Set(crate::scheduler::TaskKey::AwdTeamUnban.to_string()),
        trigger_type: Set("once".into()),
        status: Set("pending".into()),
        execute_at: Set(Some(execute_at.into())),
        payload: Set(Some(serde_json::json!({
            "event_id": event_id,
            "round_id": ban_id,
        }))),
        enabled: Set(true),
        protected: Set(true),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(())
}

/// DELETE /api/admin/events/{event_id}/awd/teams/{team_id}/ban
#[delete("/events/{event_id}/awd/teams/{team_id}/ban")]
pub async fn unban_team(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (event_id, team_id) = path.into_inner();
    let admin_id = _admin.into_inner().id;

    // P4-5 反向闭环：DB unbanned → WG host 恢复 peers → banned set reconcile → publish
    crate::modules::event::awd_team::service::ban_service::unban_team(
        ctx.db.get_ref(),
        awd.network.as_ref(),
        awd.firewall.as_ref(),
        awd.publisher.as_ref(),
        event_id,
        team_id,
        Some(admin_id),
    )
    .await
    .map_err(AppError::from)?;

    // P5-11 审计
    awd.audit
        .record(
            crate::infrastructure::audit::AuditAction::TeamUnbanned,
            &format!("AWD team {team_id} unbanned in event {event_id}"),
            serde_json::json!({ "event_id": event_id, "team_id": team_id }),
            None,
            Some(admin_id),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

// ── Score Adjustment ──

/// POST /api/admin/events/{event_id}/awd/score/adjust
#[post("/events/{event_id}/awd/score/adjust")]
pub async fn adjust_score(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
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

    // P5-11 审计
    awd.audit
        .record(
            crate::infrastructure::audit::AuditAction::ScoreAdjusted,
            &format!("AWD score adjusted for team {} in event {}", body.team_id, event_id),
            serde_json::json!({ "event_id": event_id, "team_id": body.team_id, "delta": body.delta }),
            None,
            Some(admin_id),
            Some(&ctx.req),
        )
        .await;

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
        awd.firewall.as_ref(),
        awd.crypto.as_ref(),
        &ctx.config.awd,
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
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<Uuid> {
    let event_id = path.into_inner();
    let run_id = crate::modules::event::awd_team::service::precheck_service::run_precheck(
        ctx.db.get_ref(),
        event_id,
        "manual",
        awd.network.as_ref(),
        awd.firewall.as_ref(),
        awd.containers.as_ref(),
        awd.crypto.as_ref(),
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
    let admin_id = _admin.into_inner().id;
    crate::modules::event::awd_team::service::reset_service::execute_reset(
        ctx.db.get_ref(),
        awd.containers.as_ref(),
        crate::modules::event::awd_team::service::reset_service::ResetContext {
            event_id,
            instance_id,
            team_id: uuid::Uuid::nil(), // Admin：ownership 豁免，真实 team_id 由 service 解析
            actor: crate::modules::event::awd_team::service::reset_service::ResetActor::Admin {
                admin_id,
                charge_team: false,
            },
        },
    )
    .await
    .map_err(AppError::from)?;

    // P5-11 审计
    awd.audit
        .record(
            crate::infrastructure::audit::AuditAction::GameboxReset,
            &format!("admin reset gamebox {instance_id} in event {event_id}"),
            serde_json::json!({ "event_id": event_id, "instance_id": instance_id }),
            None,
            Some(admin_id),
            Some(&ctx.req),
        )
        .await;

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
        awd.firewall.as_ref(),
        event_id,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awd/tokens/rotate
///
/// P3-10 完整编排（计划 §5.6）：
/// 1. key_version + 1，新 token 用新版本加密（修复原实现硬编码 1 + 用外键当主键
///    静默 0 行的历史 bug）；
/// 2. DB 原子更新：token ciphertext + key_version + rotation audit（同一事务）；
/// 3. 容器 rollout：recreate flagserver/judgeserver 容器（同固定 IP/网络，新 INTERNAL_TOKEN）。
///
/// 失败模型：DB 更新是原子 desired state；rollout 失败返回错误可重跑，
/// 绝不允许"DB 已只认新 token 但运行中容器仍拿旧 token"的静默态。
#[post("/events/{event_id}/awd/tokens/rotate")]
pub async fn rotate_tokens(
    admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    use crate::entity::awd_events;
    use crate::modules::event::awd_team::crypto::AwdCrypto;

    let event_id = path.into_inner();
    let admin_id = admin.into_inner().id;

    // 1. 解析真实 awd_event（真实主键 + 当前 key_version + infra 信息）
    let awd_event = event_repo::find_by_event_id(ctx.db.get_ref(), event_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("AWD event not found".into()))?;
    let new_key_version = awd_event.key_version + 1;

    let crypto = AwdCrypto::from_config_secret().map_err(|e| AppError::Internal(e.to_string()))?;

    // 2. 生成并加密新 token（新 key_version）
    let fs_token = AwdCrypto::generate_token();
    let js_token = AwdCrypto::generate_token();
    let fs_aad = AwdCrypto::build_aad(event_id, "internal_token");
    let fs_blob = crypto
        .encrypt(&fs_token, &fs_aad, new_key_version)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let js_aad = AwdCrypto::build_aad(event_id, "internal_token");
    let js_blob = crypto
        .encrypt(&js_token, &js_aad, new_key_version)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 3. DB 原子更新（真实主键）+ audit 同一事务
    let txn = ctx.db.begin().await?;
    let mut active: awd_events::ActiveModel = awd_events::ActiveModel {
        id: Set(awd_event.id),
        key_version: Set(new_key_version),
        flagserver_token_ciphertext: Set(Some(fs_blob.ciphertext)),
        flagserver_token_nonce: Set(Some(fs_blob.nonce)),
        judgeserver_token_ciphertext: Set(Some(js_blob.ciphertext)),
        judgeserver_token_nonce: Set(Some(js_blob.nonce)),
        ..Default::default()
    };
    active
        .update(&txn)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let rotation = crate::entity::awd_internal_token_rotations::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        token_type: Set("all".to_string()),
        rotated_by: Set(Some(admin_id)),
        ..Default::default()
    };
    rotation
        .insert(&txn)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    txn.commit().await?;

    // 4. 容器 rollout：recreate flagserver/judgeserver（同 IP/网络，新 token）
    let network_name = awd_event
        .docker_network_name
        .clone()
        .ok_or_else(|| AppError::Internal("Docker network not configured".into()))?;
    let fs_token_str =
        String::from_utf8(fs_token).map_err(|_| AppError::Internal("token not utf8".into()))?;
    let js_token_str =
        String::from_utf8(js_token).map_err(|_| AppError::Internal("token not utf8".into()))?;
    rollout_infra_container(
        ctx.db.get_ref(),
        awd.containers.as_ref(),
        &awd_event,
        event_id,
        "flagserver",
        &awd_event.flagserver_ip,
        &network_name,
        ctx.config.awd.flagserver_image.clone(),
        fs_token_str,
    )
    .await?;
    rollout_infra_container(
        ctx.db.get_ref(),
        awd.containers.as_ref(),
        &awd_event,
        event_id,
        "judgeserver",
        &awd_event.judgeserver_ip,
        &network_name,
        ctx.config.awd.judgeserver_image.clone(),
        js_token_str,
    )
    .await?;

    // P5-11 审计
    awd.audit
        .record(
            crate::infrastructure::audit::AuditAction::TokenRotated,
            &format!("AWD internal tokens rotated for event {event_id}"),
            serde_json::json!({ "event_id": event_id, "key_version": new_key_version }),
            None,
            Some(admin_id),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// 重建一个 infra 容器（stop → create，同 fixed_ip/网络，env 带新 token）。
async fn rollout_infra_container(
    db: &sea_orm::DatabaseConnection,
    containers: &dyn fcmc::AwdContainerRuntime,
    awd_event: &crate::entity::awd_events::Model,
    event_id: Uuid,
    kind: &str,
    fixed_ip: &str,
    network_name: &str,
    image_ref: String,
    token: String,
) -> UniResult<()> {
    let container_name = format!("fctf-{}-{}", kind, &event_id.to_string()[..8]);

    if let Err(e) = containers.stop_container(&container_name).await {
        // 容器不存在也继续 create（幂等 rollout）
        tracing::info!("[Rotate] stop {}: {}", container_name, e);
    }

    containers
        .create_infrastructure_container(fcmc::InfrastructureContainerSpec {
            event_id,
            container_name: container_name.clone(),
            image_ref,
            network_name: network_name.to_string(),
            fixed_ip: fixed_ip.to_string(),
            env: vec![
                format!("EVENT_ID={event_id}"),
                format!("INTERNAL_TOKEN={token}"),
                format!("LISTEN_ADDR=0.0.0.0:8080"),
            ],
            cpu_millis: Some(500),
            memory_bytes: Some(256 * 1024 * 1024),
        })
        .await
        .map_err(|e| {
            AppError::Internal(format!(
                "token rotation DB committed but {kind} rollout failed: {e}（可重跑本端点）"
            ))
        })?;

    // 更新 runtime resource 记录的 container_id（rollout 后容器 id 变化）
    use crate::entity::awd_runtime_resources;
    let updated = awd_runtime_resources::Entity::update_many()
        .col_expr(
            awd_runtime_resources::Column::ResourceId,
            sea_orm::sea_query::Expr::value(container_name.clone()),
        )
        .filter(awd_runtime_resources::Column::EventId.eq(event_id))
        .filter(awd_runtime_resources::Column::ResourceType.eq(kind))
        .exec(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    if updated.rows_affected == 0 {
        tracing::warn!("[Rotate] no awd_runtime_resources row for {kind} — recorded manually");
    }

    let _ = awd_event;
    tracing::info!("[Rotate] {kind} container rolled out as {}", container_name);
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
            // 注意：必须用 awd_events 真实主键 awd_event.id（原实现误用外键 event_id，
            // 条件匹配 0 行、字段更新被静默丢弃——Phase 0 修复）。
            id: Set(awd_event.id),
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

    // 状态机唯一入口（Phase 0）：配置变更 → Configuring 并清除 verified 标记。
    if awd_event.status != crate::entity::sea_orm_active_enums::AwdEventStatus::Configuring {
        event_repo::transition_event(
            ctx.db.get_ref(),
            awd_event.id,
            awd_event.status.clone(),
            crate::entity::sea_orm_active_enums::AwdEventStatus::Configuring,
            event_repo::TransitionPatch::config_changed(),
        )
        .await
        .map_err(AppError::from)?;
    } else {
        // 已是 Configuring：仅清除 verified 标记（保持原语义）。
        let mut clear: crate::entity::awd_events::ActiveModel =
            crate::entity::awd_events::ActiveModel {
                id: Set(awd_event.id),
                verified_at: Set(None),
                verified_revision: Set(None),
                ..Default::default()
            };
        clear
            .update(ctx.db.get_ref())
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
    }

    // P2-9：配置代数 +1（使 Start Gate 拦截旧 verified_generation）
    event_repo::touch_configuration(ctx.db.get_ref(), awd_event.id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    // 最后更新非状态字段（转移成功后）。
    active
        .update(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    UniResponse::ok_none().into()
}
