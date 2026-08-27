//! AWD 赛事管理端 HTTP 处理器。

use actix_web::web;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait,
};
use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard, prelude::*},
    modules::event::awd::{
        domain::AwdEventStatusExt,
        repo::{ban_repo, event_repo, round_repo},
        scheduler::{self, schedule_auto_precheck, schedule_event_start},
        service::{config_service, event_service, score_service},
    },
};

use super::dto::*;

use actix_web::{delete, get, patch, post, put};

// ── Event Management ──

/// GET /api/admin/events/{event_id}/awd
/// 查询 AWD 配置；尚未在 Configure 页保存时返回 data=null。
#[get("{event_id}/awd")]
pub async fn get_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdEventStatusDto> {
    let event_id = path.into_inner();
    let m = event_repo::find_by_event_id(ctx.db.get_ref(), event_id)
        .await
        .map_err(AppError::from)?;
    let dto = if let Some(model) = m {
        let mut dto = AwdEventStatusDto::from(model);
        dto.planned_start_at = scheduler::find_event_start_schedule(ctx.db.get_ref(), event_id)
            .await
            .map_err(AppError::from)?;
        Some(dto)
    } else {
        None
    };
    UniResponse::ok(dto).into()
}

/// POST /api/admin/events/awd
#[post("awd")]
pub async fn create_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    body: web::Json<CreateAwdEventRequest>,
) -> UniResult<Uuid> {
    use crate::modules::event::awd::crypto::AwdCrypto;

    use sea_orm::sea_query::LockType;

    let b = body.into_inner();
    b.config.validate().map_err(AppError::from)?;
    let config_patch: config_service::AwdEventConfigPatch = b.config.clone().into();
    config_patch.validate().map_err(AppError::from)?;
    let planned_start_at = if b.config.clear_planned_start {
        None
    } else {
        b.config.planned_start_at
    };

    // 锁住父 Event，使同一赛事的首次 Configure 串行化；重复请求明确返回 409，
    // 不把 UNIQUE(event_id) 冲突误报成数据库 500。
    let txn = ctx.db.begin().await?;
    let event = crate::entity::events::Entity::find_by_id(b.event_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Event {} not found", b.event_id)))?;
    if event.family != crate::entity::sea_orm_active_enums::EventFamily::Awd {
        return Err(AppError::BadRequest(format!(
            "Event {} is not an AWD team Event",
            event.id
        )));
    }
    let event_id = event.id;
    let end = event
        .end_time
        .ok_or_else(|| AppError::Validation("competition event end_time is required".into()))?;
    if planned_start_at.is_some_and(|start_at| start_at >= end) {
        return Err(AppError::Validation(
            "planned_start_at must be before the event end_time".into(),
        ));
    }
    if event_repo::find_by_event_id(&txn, event_id)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(
            "AWD event is already configured; use PATCH to update it".into(),
        ));
    }

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

    // 初始状态仍是 Draft；插入后在同一事务经 transition_event 正式进入 Configuring。
    // 网络由 Event Network 页单独分配。
    let awd_id = Uuid::new_v4();
    let model = crate::entity::awd_events::ActiveModel {
        id: Set(awd_id),
        event_id: Set(event_id),
        round_duration_secs: Set(b
            .config
            .round_duration_secs
            .unwrap_or(config_service::DEFAULT_ROUND_DURATION_SECS)),
        free_reset_count: Set(b
            .config
            .free_reset_count
            .unwrap_or(config_service::DEFAULT_FREE_RESET_COUNT)),
        extra_reset_penalty: Set(b
            .config
            .extra_reset_penalty
            .unwrap_or(config_service::DEFAULT_EXTRA_RESET_PENALTY)),
        reset_protection_secs: Set(b
            .config
            .reset_protection_secs
            .unwrap_or(config_service::DEFAULT_RESET_PROTECTION_SECS)),
        judge_max_concurrency: Set(b
            .config
            .judge_max_concurrency
            .unwrap_or(config_service::DEFAULT_JUDGE_MAX_CONCURRENCY)),
        judge_default_timeout_secs: Set(b
            .config
            .judge_default_timeout_secs
            .unwrap_or(config_service::DEFAULT_JUDGE_TIMEOUT_SECS)),
        judge_retry_interval_secs: Set(b
            .config
            .judge_retry_interval_secs
            .unwrap_or(config_service::DEFAULT_JUDGE_RETRY_INTERVAL_SECS)),
        judge_grace_period_secs: Set(b
            .config
            .judge_grace_period_secs
            .unwrap_or(config_service::DEFAULT_JUDGE_GRACE_PERIOD_SECS)),
        archive_retention_hours: Set(b
            .config
            .archive_retention_hours
            .unwrap_or(config_service::DEFAULT_ARCHIVE_RETENTION_HOURS)),
        event_secret_ciphertext: Set(secret_blob.ciphertext),
        event_secret_nonce: Set(secret_blob.nonce),
        flagserver_token_ciphertext: Set(Some(fs_blob.ciphertext)),
        flagserver_token_nonce: Set(Some(fs_blob.nonce)),
        judgeserver_token_ciphertext: Set(Some(js_blob.ciphertext)),
        judgeserver_token_nonce: Set(Some(js_blob.nonce)),
        ..Default::default()
    };

    model
        .insert(&txn)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    event_repo::transition_event(
        &txn,
        awd_id,
        crate::entity::sea_orm_active_enums::AwdEventStatus::Draft,
        crate::entity::sea_orm_active_enums::AwdEventStatus::Configuring,
        Default::default(),
    )
    .await
    .map_err(AppError::from)?;
    schedule_auto_precheck(
        &txn,
        event_id,
        planned_start_at.unwrap_or(event.start_time),
        chrono::Utc::now(),
    )
    .await?;
    schedule_event_start(&txn, event_id, planned_start_at).await?;
    txn.commit().await?;

    UniResponse::ok(event_id.into()).into()
}

/// PATCH /api/admin/events/{event_id}/awd
/// Configure 页修改 AWD runtime / reset / judge / archive 参数。
#[patch("{event_id}/awd")]
pub async fn configure_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: web::Json<AwdEventConfigRequest>,
) -> UniResult<AwdEventStatusDto> {
    let event_id = path.into_inner();
    let request = body.into_inner();
    request.validate().map_err(AppError::from)?;
    if request.expected_updated_at.is_none() {
        return Err(AppError::Validation(
            "expected_updated_at is required when updating AWD configuration".into(),
        ));
    }
    if !request.has_changes() {
        return Err(AppError::Validation(
            "at least one AWD configuration field must be provided".into(),
        ));
    }
    let model = config_service::update_event_config(ctx.db.get_ref(), event_id, request.into())
        .await
        .map_err(AppError::from)?;
    let mut dto = AwdEventStatusDto::from(model);
    dto.planned_start_at = scheduler::find_event_start_schedule(ctx.db.get_ref(), event_id)
        .await
        .map_err(AppError::from)?;
    UniResponse::ok(Some(dto)).into()
}

/// POST /api/admin/events/{event_id}/awd/start
#[post("{event_id}/awd/start")]
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
    if let Err(error) =
        scheduler::cancel_pending_event_lifecycle_schedules(ctx.db.get_ref(), event_id).await
    {
        tracing::error!(
            event_id = %event_id,
            error = %error,
            "manual AWD start succeeded but pending start-task cleanup failed"
        );
    }
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awd/pause
#[post("{event_id}/awd/pause")]
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
#[post("{event_id}/awd/resume")]
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
        awd.publisher.as_ref(),
        event_id,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awd/finish
#[post("{event_id}/awd/finish")]
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
#[post("{event_id}/awd/teams/{team_id}/ban")]
pub async fn ban_team(
    admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<BanTeamRequest>,
) -> UniResult<Uuid> {
    let (event_id, team_id) = path.into_inner();
    let admin_id = admin.into_inner().id;

    let ban_id = crate::modules::event::awd::service::ban_service::ban_team(
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
            schedule_team_unban(ctx.db.get_ref(), event_id, ban_id, execute_at)
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
#[delete("{event_id}/awd/teams/{team_id}/ban")]
pub async fn unban_team(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (event_id, team_id) = path.into_inner();
    let admin_id = _admin.into_inner().id;

    // P4-5 反向闭环：DB unbanned → WG host 恢复 peers → banned set reconcile → publish
    crate::modules::event::awd::service::ban_service::unban_team(
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
#[post("{event_id}/awd/score/adjust")]
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
#[post("{event_id}/awd/deploy")]
pub async fn deploy_awd_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    crate::modules::event::awd::service::deploy_service::deploy_event(
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
#[get("{event_id}/awd/scores")]
pub async fn get_event_scores(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<crate::modules::event::awd::domain::TeamScore>> {
    let event_id = path.into_inner();

    use crate::entity::event_teams;
    let teams = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .all(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let team_info: Vec<(uuid::Uuid, String)> = teams.into_iter().map(|t| (t.id, t.name)).collect();

    let scores = crate::modules::event::awd::service::score_service::get_scoreboard(
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
#[post("{event_id}/awd/precheck")]
pub async fn run_precheck(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<Uuid> {
    let event_id = path.into_inner();
    let run_id = crate::modules::event::awd::service::precheck_service::run_precheck(
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
#[post("{event_id}/awd/gameboxes/{instance_id}/reset")]
pub async fn admin_reset_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (event_id, instance_id) = path.into_inner();
    let admin_id = _admin.into_inner().id;
    crate::modules::event::awd::service::reset_service::execute_reset(
        ctx.db.get_ref(),
        awd.containers.as_ref(),
        crate::modules::event::awd::service::reset_service::ResetContext {
            event_id,
            instance_id,
            team_id: uuid::Uuid::nil(), // Admin：ownership 豁免，真实 team_id 由 service 解析
            actor: crate::modules::event::awd::service::reset_service::ResetActor::Admin {
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
#[get("{event_id}/awd/prechecks")]
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
#[get("{event_id}/awd/judge")]
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
#[post("{event_id}/awd/archive")]
pub async fn archive_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    crate::modules::event::awd::service::archive_service::archive_event(
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
#[post("{event_id}/awd/tokens/rotate")]
pub async fn rotate_tokens(
    admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    use crate::entity::awd_events;
    use crate::modules::event::awd::crypto::AwdCrypto;

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
    let event_network = crate::modules::event::awd::repo::event_network_repo::require_by_event_id(
        ctx.db.get_ref(),
        event_id,
    )
    .await
    .map_err(AppError::from)?;
    let network_name = event_network.docker_network_name.clone();
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
        &event_network.flagserver_ip.ip().to_string(),
        &network_name,
        ctx.config.awd.flagserver_image.clone(),
        fs_token_str,
        &ctx.config.awd.platform_internal_url,
    )
    .await?;
    rollout_infra_container(
        ctx.db.get_ref(),
        awd.containers.as_ref(),
        &awd_event,
        event_id,
        "judgeserver",
        &event_network.judgeserver_ip.ip().to_string(),
        &network_name,
        ctx.config.awd.judgeserver_image.clone(),
        js_token_str,
        &ctx.config.awd.platform_internal_url,
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
/// 探活：固定 IP:端口 TCP connect，最多 ~12s（24 × 500ms），容器启动需要时间。
async fn wait_for_tcp_ready(host: &str, port: u16) -> bool {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::net::TcpStream;
    use tokio::time::{Duration, sleep, timeout};

    let addr = match host.parse::<IpAddr>() {
        Ok(ip) => SocketAddr::new(ip, port),
        Err(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port),
    };
    for attempt in 0..24u32 {
        if timeout(Duration::from_millis(500), TcpStream::connect(addr))
            .await
            .is_ok_and(|r| r.is_ok())
        {
            if attempt > 0 {
                tracing::info!("[Rotate] health probe ok after {attempt} attempt(s): {addr}");
            }
            return true;
        }
        sleep(Duration::from_millis(500)).await;
    }
    tracing::warn!("[Rotate] health probe failed (24 attempts): {addr}");
    false
}

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
    platform_internal_url: &str,
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
            env: {
                let mut envs = vec![
                    format!("EVENT_ID={event_id}"),
                    format!("INTERNAL_TOKEN={token}"),
                    format!("LISTEN_ADDR=0.0.0.0:8080"),
                ];
                if kind == "judgeserver" {
                    envs.push(format!("PLATFORM_INTERNAL_URL={platform_internal_url}"));
                    envs.push(format!("MAX_CONCURRENT={}", awd_event.judge_max_concurrency));
                }
                envs
            },
            cpu_millis: Some(500),
            memory_bytes: Some(256 * 1024 * 1024),
        })
        .await
        .map_err(|e| {
            AppError::Internal(format!(
                "token rotation DB committed but {kind} rollout failed: {e}（可重跑本端点）"
            ))
        })?;

    // 验收 #16：两段健康才完成——重建后探活固定 IP:8080（TCP connect 证明容器
    // 监听中；app 级 /health 校验留待 Judgeserver 自身），未就绪视为失败可重跑
    if !wait_for_tcp_ready(fixed_ip, 8080).await {
        return Err(AppError::Internal(format!(
            "token rotation DB committed but {kind} container not healthy after rollout "
        )));
    }
    tracing::info!("[Rotate] {kind} container healthy after rollout ({container_name})");

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

/// GET /api/admin/events/{event_id}/awd/network —— 查看 Event Network（§64）
#[get("{event_id}/awd/network")]
pub async fn get_event_network(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<super::dto::EventNetworkResponse> {
    let event_id = path.into_inner();
    let net = crate::modules::event::awd::repo::event_network_repo::require_by_event_id(
        ctx.db.get_ref(),
        event_id,
    )
    .await
    .map_err(AppError::from)?;
    Ok(UniResponse::ok(Some(super::dto::EventNetworkResponse {
        event_id: net.event_id,
        allocation_mode: format!("{:?}", net.allocation_mode).to_lowercase(),
        gamebox_cidr: net.gamebox_cidr.to_string(),
        wireguard_cidr: net.wireguard_cidr.to_string(),
        infrastructure_subnet: net.infrastructure_subnet.to_string(),
        flagserver_ip: net.flagserver_ip.ip().to_string(),
        judgeserver_ip: net.judgeserver_ip.ip().to_string(),
        wireguard_interface_name: net.wireguard_interface_name,
        wireguard_listen_port: net.wireguard_listen_port,
        docker_network_name: net.docker_network_name,
        locked: net.locked_at.is_some(),
    })))
}

/// PUT /api/admin/events/{event_id}/awd/network —— 分配（automatic 默认 / manual，§23/§24）
/// 幂等：已分配未锁定 → 直接返回；已锁定 → AWD_NETWORK_LOCKED。
#[put("{event_id}/awd/network")]
pub async fn update_network(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: web::Json<super::dto::NetworkUpdateRequest>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    use crate::modules::event::awd::{repo::event_network_repo, service::event_network_service};

    if let Some(existing) = event_network_repo::find_by_event_id(ctx.db.get_ref(), event_id)
        .await
        .map_err(AppError::from)?
    {
        if existing.locked_at.is_some() {
            return Err(AppError::from(
                crate::modules::event::awd::AwdError::NetworkLocked(
                    "network locked after deploy（AWD_NETWORK_LOCKED）".into(),
                ),
            ));
        }
        // 已分配未锁定：幂等 no-op
        return UniResponse::ok_none().into();
    }

    let b = body.into_inner();
    let is_manual = b.gamebox_cidr.is_some() || b.wireguard_cidr.is_some();
    if is_manual {
        event_network_service::allocate_manual(
            ctx.db.get_ref(),
            event_id,
            event_network_service::ManualNetworkRequest {
                gamebox_cidr: b.gamebox_cidr.unwrap_or_default(),
                wireguard_cidr: b.wireguard_cidr.unwrap_or_default(),
                wireguard_listen_port: b.wireguard_listen_port,
            },
        )
        .await
        .map_err(AppError::from)?;
    } else {
        event_network_service::allocate_automatic(ctx.db.get_ref(), event_id)
            .await
            .map_err(AppError::from)?;
    }

    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awd/network/reallocate —— §33/§93
#[post("{event_id}/awd/network/reallocate")]
pub async fn reallocate_network(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    crate::modules::event::awd::service::event_network_service::reallocate(
        ctx.db.get_ref(),
        event_id,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}
