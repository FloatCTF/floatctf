//! AWD 内部服务接口（裁判机/探针等，Wave 3 — Pull Judge）。

use actix_web::web;

use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, prelude::*},
    entity::sea_orm_active_enums::{AwdEventStatus, JudgeTaskStatus, ScoreEventType},
    modules::event::awd::{
        AwdError,
        api::auth::{AwdInternalAuth, AwdInternalPrincipal},
        domain::{AwdPhaseExt, IdempotencyKey, JudgeTaskStatusExt},
        repo::{ban_repo, event_repo, judge_repo, score_repo},
        service::{event_service, flag_service},
    },
};

use super::dto::*;

use actix_web::post;

/// POST /internal/awd/events/{event_id}/flags/issue
/// 由 FlagServer 调用，为 GameBox 发放 Flag。
/// 需要 FlagServer 令牌。
#[post("/internal/awd/events/{event_id}/flags/issue")]
pub async fn issue_flag(
    auth: AwdInternalAuth,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
    body: web::Json<IssueFlagInternalRequest>,
) -> UniResult<FlagIssueResponse> {
    let event_id = match auth.principal {
        AwdInternalPrincipal::FlagServer { event_id } => event_id,
        _ => return Err(AppError::Forbidden("Not enough permission".into())),
    };

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

    use crate::modules::event::awd::crypto::AwdCrypto;
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
            round_id: Uuid::nil(),
            gamebox_instance_id: Uuid::nil(),
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

// ── Wave 3: Judge Pull + Lease 端点 ──

/// POST /internal/awd/events/{event_id}/judge/claim
/// JudgeServer 轮询认领 Pending 任务。
#[post("/internal/awd/events/{event_id}/judge/claim")]
pub async fn judge_claim(
    auth: AwdInternalAuth,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
    body: web::Json<JudgeClaimRequest>,
) -> UniResult<JudgeClaimResponse> {
    let event_id = match auth.principal {
        AwdInternalPrincipal::JudgeServer { event_id } => event_id,
        _ => return Err(AppError::Forbidden("Not enough permission".into())),
    };

    awd.rate_limiter
        .check(
            ctx.db.get_ref(),
            crate::infrastructure::ratelimit::RateScope::Internal,
            &event_id.to_string(),
        )
        .await
        .map_err(AppError::from)?;

    let _path_event_id = path.into_inner();
    let req = body.into_inner();

    let worker_id = req.worker_id.trim().to_string();
    if worker_id.is_empty() || worker_id.len() > 256 {
        return Err(AppError::BadRequest("worker_id must be 1-256 chars".into()));
    }
    let limit = req.limit.unwrap_or(5).min(20).max(1);

    let now = chrono::Utc::now();
    // 先清理过期任务
    let _ = judge_repo::terminalize_past_deadline(ctx.db.get_ref(), now).await;

    let lease_ttl_secs: i64 = 120;
    let claimed = judge_repo::claim_tasks(
        ctx.db.get_ref(),
        event_id,
        &worker_id,
        limit,
        lease_ttl_secs,
    )
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let mut tasks = Vec::new();
    for ct in &claimed {
        let mut script_content = String::new();
        let mut script_args_json = None;
        let mut target_ip = String::new();
        let mut timeout_secs: i32 = 30;

        // Resolve execution payload
        if let Some(eg_id) = ct.event_gamebox_id {
            if let Ok(resolved) =
                crate::modules::event::awd::service::gamebox_service::resolve_event_gamebox_spec(
                    ctx.db.get_ref(),
                    eg_id,
                )
                .await
            {
                script_content = resolved.judge_script_content().unwrap_or("").to_string();
                if let Some(args) = resolved.judge_args_json() {
                    if let Ok(arr) = serde_json::from_value::<Vec<String>>(args.clone()) {
                        script_args_json = Some(serde_json::to_string(&arr).unwrap_or_default());
                    }
                }
                timeout_secs = resolved.effective_judge_timeout_secs.unwrap_or(30);
            }
        }

        // Look up instance IP
        use crate::entity::event_gamebox_instances;
        if let Ok(Some(inst)) = event_gamebox_instances::Entity::find_by_id(ct.gamebox_instance_id)
            .one(ctx.db.get_ref())
            .await
        {
            target_ip = inst.gamebox_ip.ip().to_string();
        }

        tasks.push(ClaimedTaskDto {
            task_id: ct.task_id,
            batch_id: ct.batch_id,
            event_id: ct.event_id,
            round_id: ct.round_id,
            gamebox_instance_id: ct.gamebox_instance_id,
            event_gamebox_id: ct.event_gamebox_id,
            team_id: ct.team_id,
            attempt: ct.attempt,
            lease_token: ct.lease_token.clone(),
            lease_expires_at: ct.lease_expires_at.into(),
            deadline_at: ct.deadline_at.into(),
            script_content,
            script_args_json,
            target_ip,
            timeout_secs,
        });
    }

    UniResponse::ok(JudgeClaimResponse { tasks }.into()).into()
}

/// POST /internal/awd/events/{event_id}/judge/tasks/{task_id}/heartbeat
/// JudgeServer 心跳续租。
#[post("/internal/awd/events/{event_id}/judge/tasks/{task_id}/heartbeat")]
pub async fn judge_heartbeat(
    auth: AwdInternalAuth,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<JudgeHeartbeatRequest>,
) -> UniResult<()> {
    let event_id = match auth.principal {
        AwdInternalPrincipal::JudgeServer { event_id } => event_id,
        _ => return Err(AppError::Forbidden("Not enough permission".into())),
    };

    let (_path_event_id, task_id) = path.into_inner();
    let req = body.into_inner();

    let lease_ttl_secs: i64 = 120;
    let now = chrono::Utc::now();
    let result = judge_repo::heartbeat_task(
        ctx.db.get_ref(),
        task_id,
        &req.worker_id,
        req.attempt,
        &req.lease_token,
        lease_ttl_secs,
        now,
    )
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    match result {
        judge_repo::HeartbeatResult::Ok => UniResponse::ok_none().into(),
        judge_repo::HeartbeatResult::Stale => {
            Err(AppError::Conflict("stale lease or attempt".into()))
        }
        judge_repo::HeartbeatResult::NotFound => Err(AppError::NotFound("task not found".into())),
    }
}

/// POST /internal/awd/events/{event_id}/judge/tasks/{task_id}/result
/// JudgeServer 提交执行结果。
#[post("/internal/awd/events/{event_id}/judge/tasks/{task_id}/result")]
pub async fn judge_result(
    auth: AwdInternalAuth,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<JudgeResultRequest>,
) -> UniResult<()> {
    let event_id = match auth.principal {
        AwdInternalPrincipal::JudgeServer { event_id } => event_id,
        _ => return Err(AppError::Forbidden("Not enough permission".into())),
    };

    let (_path_event_id, task_id) = path.into_inner();
    let req = body.into_inner();

    // Look up task for context
    let task = judge_repo::find_task_by_id(ctx.db.get_ref(), task_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Judge task not found".into()))?;

    if task.event_id != event_id {
        return Err(AppError::Forbidden("Task does not belong to event".into()));
    }

    // Map outcome to status
    let status = match req.outcome.as_str() {
        "up" => JudgeTaskStatus::Up,
        "down" => JudgeTaskStatus::Down,
        "target_timeout" => JudgeTaskStatus::Down, // target timeout = competition Down
        "worker_error" => {
            // Worker error: release back to Pending if retries remain
            if task.attempt_count < task.max_attempts {
                let now = chrono::Utc::now();
                let deadline = task.deadline_at.with_timezone(&chrono::Utc);
                if now < deadline {
                    // Release back to Pending
                    release_task_to_pending(ctx.db.get_ref(), task_id)
                        .await
                        .map_err(|e| AppError::Database(e.to_string()))?;
                    return UniResponse::ok_none().into();
                }
            }
            JudgeTaskStatus::JudgeError
        }
        _ => {
            return Err(AppError::BadRequest(format!(
                "unknown outcome: {}",
                req.outcome
            )));
        }
    };

    // Submit result
    let now = chrono::Utc::now();
    let result = judge_repo::submit_result(
        ctx.db.get_ref(),
        task_id,
        &req.worker_id,
        req.attempt,
        &req.lease_token,
        &req.result_id,
        status.clone(),
        req.exit_code,
        req.stdout.as_deref(),
        req.stderr.as_deref(),
        req.duration_ms,
        now,
    )
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let submit_response: UniResult<()> = match result {
        judge_repo::SubmitResult::Ok | judge_repo::SubmitResult::Idempotent => {
            // Score only if Down (§17.1: Up → no score)
            let is_down = status.is_down();

            if is_down {
                let awd_event = event_repo::find_by_event_id(ctx.db.get_ref(), event_id)
                    .await
                    .map_err(|e| AppError::Database(e.to_string()))?
                    .ok_or_else(|| AppError::NotFound("AWD event not found".into()))?;
                let frozen =
                    awd_event.status != AwdEventStatus::Running || !awd_event.phase.allows_judge();

                // Check if team is currently banned (in-flight ban: ban after task creation)
                let team_banned = if !frozen {
                    ban_repo::find_active_ban(ctx.db.get_ref(), event_id, task.team_id)
                        .await
                        .map_err(|e| AppError::Database(e.to_string()))?
                        .is_some()
                } else {
                    false
                };

                if !frozen && !team_banned {
                    if let Some(eg_id) = task.event_gamebox_id {
                        if let Ok(resolved) = crate::modules::event::awd::service::gamebox_service::resolve_event_gamebox_spec(
                            ctx.db.get_ref(),
                            eg_id,
                        )
                        .await
                        {
                            let delta = -resolved.event_gamebox.judge_down_penalty;
                            let idempotency_key = IdempotencyKey::judge_down(&task_id.to_string());

                            match score_repo::create_score_event(
                                ctx.db.get_ref(),
                                event_id,
                                Some(task.round_id),
                                task.team_id,
                                ScoreEventType::JudgeDown,
                                delta,
                                &idempotency_key,
                                None,
                                Some(task.gamebox_instance_id),
                                Some(eg_id),
                                Some("judge check"),
                            )
                            .await
                            {
                                Ok(_) => {}
                                Err(e) => {
                                    let msg = e.to_string().to_lowercase();
                                    if !(msg.contains("23505") || msg.contains("duplicate")) {
                                        tracing::error!(
                                            "[Judge] score write failed for task {}: {}",
                                            task_id, e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            UniResponse::ok_none().into()
        }
        judge_repo::SubmitResult::Stale => Err(AppError::Conflict("stale lease or attempt".into())),
        judge_repo::SubmitResult::NotFound => Err(AppError::NotFound("task not found".into())),
    };

    // After any terminal result, attempt to finish the event if final settlement is complete
    if submit_response.is_ok() {
        let _ = event_service::maybe_finish_event(
            ctx.db.get_ref(),
            awd.network.as_ref(),
            awd.firewall.as_ref(),
            awd.publisher.as_ref(),
            event_id,
        )
        .await;
    }

    submit_response
}

/// 将任务释放回 Pending（清除 lease 字段）。
async fn release_task_to_pending(
    db: &sea_orm::DatabaseConnection,
    task_id: Uuid,
) -> Result<(), sea_orm::DbErr> {
    use crate::entity::awd_judge_tasks;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    awd_judge_tasks::ActiveModel {
        id: Set(task_id),
        status: Set(JudgeTaskStatus::Pending),
        worker_id: Set(None),
        lease_token_hash: Set(None),
        lease_expires_at: Set(None),
        heartbeat_at: Set(None),
        claimed_at: Set(None),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

/// GET /internal/awd/events/{event_id}/health
/// 需要任意有效内部令牌（FlagServer 或 JudgeServer）。
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
