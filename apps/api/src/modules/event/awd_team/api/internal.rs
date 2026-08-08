//! AWD internal API handlers — for FlagServer and JudgeServer communication.
//!
//! All endpoints require AwdInternalAuth (Bearer token validated against
//! encrypted tokens stored in awd_events).

use actix_web::{HttpResponse, web};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, prelude::*},
    entity::sea_orm_active_enums::{JudgeTaskStatus, ScoreEventType},
    modules::event::awd_team::{
        AwdError,
        api::auth::{AwdInternalAuth, AwdInternalPrincipal},
        domain::JudgeTaskStatusExt,
        repo::{event_repo, judge_repo, round_repo, score_repo},
        service::flag_service,
    },
};

use super::dto::*;

use actix_web::post;

/// POST /internal/awd/events/{event_id}/flags/issue
/// Called by FlagServer to issue a flag for a GameBox.
/// Requires FlagServer token.
#[post("/internal/awd/events/{event_id}/flags/issue")]
pub async fn issue_flag(
    auth: AwdInternalAuth,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
    body: web::Json<IssueFlagInternalRequest>,
) -> UniResult<FlagIssueResponse> {
    // Verify the caller is a FlagServer for this event
    let event_id = match auth.principal {
        AwdInternalPrincipal::FlagServer { event_id } => event_id,
        _ => return Err(AppError::Forbidden("Not enough permission".into())),
    };

    // P5-10 限流：internal（每 event 每分钟）
    awd.rate_limiter
        .check(
            ctx.db.get_ref(),
            crate::infrastructure::ratelimit::RateScope::Internal,
            &event_id.to_string(),
        )
        .await
        .map_err(AppError::from)?;

    let _path_event_id = path.into_inner();

    let awd_event = event_repo::find_by_event_id(ctx.db.get_ref(), event_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("AWD event not found".into()))?;

    // Decrypt event-bound secret (AAD = event_id:event_secret) for deterministic flags.
    use crate::modules::event::awd_team::crypto::AwdCrypto;
    let crypto = AwdCrypto::from_config_secret().map_err(|e| AppError::Internal(e.to_string()))?;
    let secret = crypto
        .decrypt_event_secret(
            event_id,
            &awd_event.event_secret_ciphertext,
            &awd_event.event_secret_nonce,
            awd_event.key_version,
        )
        .map_err(|e| AppError::Internal(format!("event_secret decrypt failed: {e}")))?;

    let flag_prefix = "flag";

    let result = flag_service::issue_flag(
        ctx.db.get_ref(),
        flag_service::FlagIssueContext {
            event_id,
            round_id: Uuid::nil(),            // will be resolved by service
            gamebox_instance_id: Uuid::nil(), // will be resolved by service
            source_ip: body.source_ip.clone(),
        },
        &secret,
        flag_prefix,
    )
    .await
    .map_err(|e: AwdError| match e {
        AwdError::Forbidden(_) => AppError::Forbidden("Not enough permission".into()),
        AwdError::NotFound(_) => AppError::NotFound(e.to_string()),
        _ => AppError::Internal(e.to_string()),
    })?;

    UniResponse::ok(FlagIssueResponse { flag: result.flag }.into()).into()
}

/// POST /internal/awd/events/{event_id}/judge/callback
/// Called by JudgeServer after each task completes.
/// Requires JudgeServer token.
#[post("/internal/awd/events/{event_id}/judge/callback")]
pub async fn judge_callback(
    auth: AwdInternalAuth,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
    body: web::Json<JudgeCallbackRequest>,
) -> UniResult<()> {
    // Verify the caller is a JudgeServer for this event
    let event_id = match auth.principal {
        AwdInternalPrincipal::JudgeServer { event_id } => event_id,
        _ => return Err(AppError::Forbidden("Not enough permission".into())),
    };

    // P5-10 限流：internal（每 event 每分钟）
    awd.rate_limiter
        .check(
            ctx.db.get_ref(),
            crate::infrastructure::ratelimit::RateScope::Internal,
            &event_id.to_string(),
        )
        .await
        .map_err(AppError::from)?;

    let _path_event_id = path.into_inner();
    let cb = body.into_inner();

    // Find the judge task
    let task = judge_repo::find_task_by_id(ctx.db.get_ref(), cb.task_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Judge task not found".into()))?;

    // Verify the task belongs to this event
    if task.event_id != event_id {
        return Err(AppError::Forbidden("Not enough permission".into()));
    }

    // Don't overwrite terminal states
    if task.status.is_terminal() {
        return UniResponse::ok_none().into();
    }

    // Parse status from callback
    let status = match cb.status.as_str() {
        "up" => JudgeTaskStatus::Up,
        "down" => JudgeTaskStatus::Down,
        "judge_error" => JudgeTaskStatus::JudgeError,
        "judge_timeout" => JudgeTaskStatus::JudgeTimeout,
        _ => JudgeTaskStatus::JudgeError,
    };

    let is_up = status.is_up();
    let is_down = status.is_down();

    // Update task
    judge_repo::update_task_status(
        ctx.db.get_ref(),
        cb.task_id,
        status,
        cb.exit_code,
        cb.stdout.as_deref(),
        cb.stderr.as_deref(),
        cb.duration_ms,
    )
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    // If up/down, record score
    if is_up || is_down {
        // Get the template for scoring values
        use crate::entity::awd_gamebox_templates;
        let template = awd_gamebox_templates::Entity::find_by_id(task.template_id)
            .one(ctx.db.get_ref())
            .await
            .map_err(|e| AppError::Database(e.to_string()))?
            .ok_or_else(|| AppError::NotFound("Template not found".into()))?;

        let score_type = if is_up {
            ScoreEventType::JudgeFix
        } else {
            ScoreEventType::JudgeDown
        };

        let delta = if is_up {
            template.fix_points
        } else {
            -template.down_points
        };

        let idempotency_key = cb.callback_id.clone();

        // 记录计分（Phase 0 P0-4：禁止静默吞错）。
        // 重复回调由 idempotency_key 唯一约束兜底 → 视为幂等成功；
        // 其他失败显式返回错误，JudgeServer 重试（同 callback_id，P3-9），
        // 完整的事务化/可重试设计见 Phase 3 P3-8。
        match score_repo::create_score_event(
            ctx.db.get_ref(),
            event_id,
            Some(task.round_id),
            task.team_id,
            score_type,
            delta,
            &idempotency_key,
            None,
            Some(task.gamebox_instance_id),
            Some(task.template_id),
            Some("judge check"),
        )
        .await
        {
            Ok(_) => {}
            Err(e) if is_duplicate_key(&e) => {
                tracing::debug!(
                    "[Judge] duplicate score event (callback_id={}) — idempotent skip",
                    idempotency_key
                );
            }
            Err(e) => {
                tracing::error!(
                    "[Judge] score write failed for task {} callback {}: {}",
                    cb.task_id,
                    idempotency_key,
                    e
                );
                return Err(AppError::Internal(format!("score write failed: {e}")));
            }
        }
    }

    UniResponse::ok_none().into()
}

/// 判断数据库错误是否为唯一约束冲突（幂等重复回调）。
/// 不依赖 sqlx 直接依赖：以错误文本中的 constraint code / 关键词判定。
fn is_duplicate_key(e: &sea_orm::DbErr) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("23505") || msg.contains("duplicate")
}

/// GET /internal/awd/events/{event_id}/health
/// Requires any valid internal token (FlagServer or JudgeServer).
#[actix_web::get("/internal/awd/events/{event_id}/health")]
pub async fn event_health(
    _auth: AwdInternalAuth,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> actix_web::HttpResponse {
    let event_id = path.into_inner();
    match event_repo::find_by_event_id(ctx.db.get_ref(), event_id).await {
        Ok(Some(event)) => actix_web::HttpResponse::Ok().json(serde_json::json!({
            "status": "ok",
            "event_id": event_id,
            "awd_status": format!("{:?}", event.status),
            "phase": format!("{:?}", event.phase),
        })),
        Ok(None) => actix_web::HttpResponse::NotFound().json(serde_json::json!({
            "error": "event not found"
        })),
        Err(e) => actix_web::HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}
