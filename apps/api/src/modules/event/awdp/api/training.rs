//! AWDP Training Ground 路由（挂 /api/service/*，全部 UserJwtGuard）。
//!
//! 目录：GET /api/service/gameboxes?capability=awdp（安全白名单字段）
//! Start Training：POST /api/service/gameboxes/{gamebox_id}/awdp/runs（幂等）
//! Practice Run（run-scoped，owner-only）：
//!   GET /api/service/awdp/runs/{run_id}（+ stop / reset / restart-training / rounds /
//!       evaluations / scores / stream）
//!   POST/GET .../gameboxes/{gamebox_id}/instance[/stop|/reset]
//!   POST .../gameboxes/{gamebox_id}/break|patch|test-check；GET .../source

use actix_web::web::{self, Json};
use uuid::Uuid;

use actix_multipart::form::{MultipartForm, tempfile::TempFile};

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::UserJwtGuard, prelude::*},
    entity::{awdp_runs, gameboxes, sea_orm_active_enums::AwdpPhase},
    modules::event::awdp::{
        api::dto::*,
        repo::{evaluation_repo, event_gamebox_repo, round_repo, run_repo, score_repo},
        service::{
            break_service, evaluation, patch_service, practice_service,
            runtime::{self, Subject},
        },
    },
};

// ────────────────────────────────────────────────────────────────────────────
// helpers
// ────────────────────────────────────────────────────────────────────────────

/// 校验 run 属于当前用户（practice owner-only）。
async fn require_owned_run(
    db: &sea_orm::DatabaseConnection,
    run_id: Uuid,
    user_id: Uuid,
) -> Result<awdp_runs::Model, AppError> {
    let run = run_repo::require_by_id(db, run_id)
        .await
        .map_err(AppError::from)?;
    if run.owner_user_id != Some(user_id) {
        return Err(AppError::Forbidden("该训练 run 不属于你".into()));
    }
    Ok(run)
}

async fn flag_prefix(ctx: &ReqCtx) -> String {
    crate::infrastructure::settings::get_setting(ctx.db.get_ref(), "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into())
}

// ────────────────────────────────────────────────────────────────────────────
// Catalog / Start Training
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CatalogQuery {
    capability: Option<String>,
}

/// GET /api/service/gameboxes?capability=awdp —— AWDP-capable 安全目录。
/// 只返回安全展示字段（禁止 exploit/source key/source_code_dir/credentials）。
#[get("gameboxes")]
pub async fn list_catalog(
    user: UserJwtGuard,
    ctx: ReqCtx,
    query: Query<CatalogQuery>,
) -> UniResult<Vec<GameBoxCatalogDto>> {
    let user = user.into_inner();
    if let Some(cap) = &query.capability {
        if cap != "awdp" {
            return UniResponse::ok(vec![].into()).into();
        }
    }
    let db = ctx.db.get_ref();

    let rows = gameboxes::Entity::find()
        .filter(gameboxes::Column::Hidden.eq(false))
        .filter(gameboxes::Column::BuildStatus.eq(Some(
            crate::modules::gamebox::BUILD_STATUS_READY.to_string(),
        )))
        .order_by_asc(gameboxes::Column::Name)
        .all(db)
        .await?;

    let mut out = Vec::new();
    for gb in rows {
        // AWDP capability = 完整五列（source.zip 产物存在，DB CHECK 保证全有/全无）。
        if gb.awdp_source_artifact_key.is_none() {
            continue;
        }
        let active_training = match run_repo::find_active_practice_for(db, gb.id, user.id).await? {
            Some(run) => {
                let score = score_repo::my_total(db, run.id, Some(user.id), None).await?;
                Some(ActiveTrainingDto {
                    run_id: run.id,
                    phase: run.phase,
                    score,
                })
            }
            None => None,
        };
        out.push(GameBoxCatalogDto {
            id: gb.id,
            name: gb.name.clone(),
            description: gb.description.clone(),
            category: gb.category.clone(),
            version: gb.version.clone(),
            awdp_capable: true,
            recommended_cpu_millis: gb.recommended_cpu_millis,
            recommended_memory_bytes: gb.recommended_memory_bytes,
            recommended_pids_limit: gb.recommended_pids_limit,
            active_training,
        });
    }
    UniResponse::ok(out.into()).into()
}

/// POST /api/service/gameboxes/{gamebox_id}/awdp/runs —— Start Training（幂等）。
#[post("gameboxes/{gamebox_id}/awdp/runs")]
pub async fn start_training(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpRunDto> {
    let gamebox_id = path.into_inner();
    let user = user.into_inner();
    let run = practice_service::start_training(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        user.id,
        gamebox_id,
        &flag_prefix(&ctx).await,
    )
    .await
    .map_err(AppError::from)?;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id).await?;
    UniResponse::ok(dto.into()).into()
}

// ────────────────────────────────────────────────────────────────────────────
// Practice Run
// ────────────────────────────────────────────────────────────────────────────

/// GET /api/service/awdp/runs/{run_id} —— run 统一 view-model。
#[get("awdp/runs/{run_id}")]
pub async fn get_run_overview(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpRunDto> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id).await?;
    UniResponse::ok(dto.into()).into()
}

/// POST /api/service/awdp/runs/{run_id}/stop —— 停止 run 内全部实例（保留 run/端点）。
#[post("awdp/runs/{run_id}/stop")]
pub async fn stop_run(user: UserJwtGuard, ctx: ReqCtx, path: web::Path<Uuid>) -> UniResult<()> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let _ = run;
    let views = runtime::list_instances(ctx.db.get_ref(), run_id).await?;
    for v in views {
        runtime::stop_instance(
            ctx.db.get_ref(),
            ctx.docker.get_ref(),
            v.instance_id,
            Subject::user(user.id),
        )
        .await?;
    }
    UniResponse::ok_none().into()
}

/// POST /api/service/awdp/runs/{run_id}/reset —— run 内全部实例 reset pristine（保留 run/端点）。
#[post("awdp/runs/{run_id}/reset")]
pub async fn reset_run(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpRunDto> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    crate::modules::event::awdp::service::event_service::reset_all_run_instances(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        run.id,
    )
    .await?;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id).await?;
    UniResponse::ok(dto.into()).into()
}

/// POST /api/service/awdp/runs/{run_id}/restart-training —— Train Again（ended → 新 run）。
#[post("awdp/runs/{run_id}/restart-training")]
pub async fn train_again(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpRunDto> {
    let old_run_id = path.into_inner();
    let user = user.into_inner();
    let run = practice_service::train_again(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        user.id,
        old_run_id,
        &flag_prefix(&ctx).await,
    )
    .await
    .map_err(AppError::from)?;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id).await?;
    UniResponse::ok(dto.into()).into()
}

/// POST /api/service/awdp/runs/{run_id}/gameboxes/{gamebox_id}/break —— Break flag 提交。
#[post("awdp/runs/{run_id}/gameboxes/{gamebox_id}/break")]
pub async fn submit_break_flag(
    user: UserJwtGuard,
    ctx: ReqCtx,
    state: web::Data<crate::bootstrap::AppState>,
    path: web::Path<(Uuid, Uuid)>,
    body: Json<BreakSubmitRequest>,
) -> UniResult<BreakSubmitResponse> {
    let (run_id, gamebox_id) = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let result = break_service::submit_flag(
        ctx.db.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        run.id,
        gamebox_id,
        &body.flag,
        Subject::user(user.id),
    )
    .await
    .map_err(AppError::from)?;
    if result.scored {
        crate::modules::event::awdp::realtime::run_score_changed(
            &state,
            run.id,
            "break",
            run.break_score,
        );
    }
    UniResponse::ok(
        BreakSubmitResponse {
            accepted: result.accepted,
            scored: result.scored,
            already_broken: result.already_broken,
        }
        .into(),
    )
    .into()
}

/// GET /api/service/awdp/runs/{run_id}/gameboxes/{gamebox_id}/source —— source.zip（Fix only）。
#[get("awdp/runs/{run_id}/gameboxes/{gamebox_id}/source")]
pub async fn download_source(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<String> {
    let (run_id, gamebox_id) = path.into_inner();
    let _user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, _user.id).await?;
    if run.phase != AwdpPhase::Fix {
        return Err(AppError::InvalidState(
            "source.zip 仅在 Fix 阶段可下载".into(),
        ));
    }
    if run.gamebox_id != Some(gamebox_id) {
        return Err(AppError::Validation("gamebox 不属于该训练 run".into()));
    }
    let gamebox = event_gamebox_repo::find_gamebox_identity(ctx.db.get_ref(), gamebox_id).await?;
    let key = gamebox.awdp_source_artifact_key.ok_or_else(|| {
        AppError::NotFound("该 GameBox 没有 source.zip 产物（无 [awdp] capability）".into())
    })?;
    let url = crate::modules::platform::files::download::presign_private_download_url(
        ctx.rustfs.clone(),
        &key,
        300,
    )
    .await
    .map_err(|e| AppError::BadRequest(e.to_string()))?;
    UniResponse::ok(url.into()).into()
}

#[derive(Debug, MultipartForm)]
struct PatchUploadForm {
    #[multipart(rename = "patch_file")]
    patch_file: TempFile,
}

/// POST .../patch —— 上传并应用 patch.sh（Fix only；multipart）。
#[post("awdp/runs/{run_id}/gameboxes/{gamebox_id}/patch")]
pub async fn upload_patch(
    user: UserJwtGuard,
    ctx: ReqCtx,
    state: web::Data<crate::bootstrap::AppState>,
    path: web::Path<(Uuid, Uuid)>,
    MultipartForm(form): MultipartForm<PatchUploadForm>,
) -> UniResult<PatchSubmitResponse> {
    let (run_id, gamebox_id) = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, gamebox_id, Subject::user(user.id))
            .await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    let bytes = std::fs::read(form.patch_file.file.path())
        .map_err(|e| AppError::BadRequest(format!("read patch_file: {e}")))?;
    let script = String::from_utf8(bytes)
        .map_err(|_| AppError::Validation("patch.sh 必须是 UTF-8 文本脚本".into()))?;
    if script.len() > patch_service::MAX_PATCH_BYTES {
        return Err(AppError::Validation(format!(
            "patch.sh 超过 {} KiB 上限",
            patch_service::MAX_PATCH_BYTES / 1024
        )));
    }
    let result = patch_service::apply_patch(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        run.id,
        view.instance_id,
        &script,
        Subject::user(user.id),
    )
    .await
    .map_err(AppError::from)?;
    let status = match result {
        patch_service::PatchResult::Applied => "applied",
        patch_service::PatchResult::Failed => "failed",
    };
    crate::modules::event::awdp::realtime::run_patch_applied(
        &state,
        run.id,
        view.instance_id,
        status,
    );
    UniResponse::ok(
        PatchSubmitResponse {
            status: status.into(),
        }
        .into(),
    )
    .into()
}

/// POST .../test-check —— 手动 Test Check（healthcheck + judge，不计分）。
#[post("awdp/runs/{run_id}/gameboxes/{gamebox_id}/test-check")]
pub async fn manual_test_check(
    user: UserJwtGuard,
    ctx: ReqCtx,
    state: web::Data<crate::bootstrap::AppState>,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<ManualCheckDto> {
    let (run_id, gamebox_id) = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, gamebox_id, Subject::user(user.id))
            .await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    let result = evaluation::manual_check(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        run.id,
        view.instance_id,
        Subject::user(user.id),
    )
    .await
    .map_err(AppError::from)?;
    crate::modules::event::awdp::realtime::run_manual_check_completed(
        &state,
        run.id,
        view.instance_id,
        result.healthcheck_ok && result.judge_ok,
    );
    UniResponse::ok(
        ManualCheckDto {
            healthcheck_ok: result.healthcheck_ok,
            healthcheck_detail: result.healthcheck_detail,
            judge_ok: result.judge_ok,
            judge_detail: result.judge_detail,
        }
        .into(),
    )
    .into()
}

/// POST .../instance —— 启动/复用实例。
#[post("awdp/runs/{run_id}/gameboxes/{gamebox_id}/instance")]
pub async fn start_my_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<InstanceViewDto> {
    let (run_id, gamebox_id) = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let view = runtime::start_instance(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        run.id,
        gamebox_id,
        Subject::user(user.id),
        &flag_prefix(&ctx).await,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(InstanceViewDto::from(&view).into()).into()
}

/// GET .../instance —— 实例详情。
#[get("awdp/runs/{run_id}/gameboxes/{gamebox_id}/instance")]
pub async fn get_my_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<Option<InstanceViewDto>> {
    let (run_id, gamebox_id) = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let view =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, gamebox_id, Subject::user(user.id))
            .await?;
    UniResponse::ok(view.map(|v| InstanceViewDto::from(&v)).into()).into()
}

/// POST .../instance/stop —— 停止实例（保留逻辑实例/端点）。
#[post("awdp/runs/{run_id}/gameboxes/{gamebox_id}/instance/stop")]
pub async fn stop_my_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (run_id, gamebox_id) = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, gamebox_id, Subject::user(user.id))
            .await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    runtime::stop_instance(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        view.instance_id,
        Subject::user(user.id),
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// POST .../instance/reset —— 玩家 Reset（pristine 重建，保留端点）。
#[post("awdp/runs/{run_id}/gameboxes/{gamebox_id}/instance/reset")]
pub async fn reset_my_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<InstanceViewDto> {
    let (run_id, gamebox_id) = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, gamebox_id, Subject::user(user.id))
            .await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    let view = runtime::reset_instance(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        view.instance_id,
        Subject::user(user.id),
        &flag_prefix(&ctx).await,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(InstanceViewDto::from(&view).into()).into()
}

/// GET .../rounds —— Fix 回合时间线。
#[get("awdp/runs/{run_id}/rounds")]
pub async fn get_rounds(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<AwdpRoundDto>> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let rounds = round_repo::list_for_run(ctx.db.get_ref(), run.id).await?;
    UniResponse::ok(
        rounds
            .into_iter()
            .map(|r| AwdpRoundDto {
                id: r.id,
                sequence: r.sequence,
                starts_at: r.starts_at,
                cutoff_at: r.cutoff_at,
                status: r.status,
            })
            .collect::<Vec<_>>()
            .into(),
    )
    .into()
}

/// GET .../evaluations —— 我的官方评估历史。
#[get("awdp/runs/{run_id}/evaluations")]
pub async fn get_my_evaluations(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<AwdpEvaluationDto>> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let all = evaluation_repo::list_for_run(ctx.db.get_ref(), run.id).await?;
    let mut out = Vec::new();
    for ev in all {
        // practice run 全部实例属于本人；仍按实例归属过滤（与 competition 语义一致）。
        let owned = crate::modules::event::awdp::repo::instance_repo::find_by_instance_id(
            ctx.db.get_ref(),
            ev.instance_id,
        )
        .await
        .map(|(inst, ext)| {
            let ok = inst.owner_user_id == Some(user.id);
            (ok, ext.gamebox_id)
        })
        .unwrap_or((false, Uuid::nil()));
        if !owned.0 {
            continue;
        }
        let round_seq = match ev.fix_round_id {
            Some(rid) => round_repo::find_by_id(ctx.db.get_ref(), rid)
                .await
                .map(|r| r.sequence)
                .ok(),
            None => None,
        };
        out.push(AwdpEvaluationDto {
            id: ev.id,
            instance_id: ev.instance_id,
            gamebox_id: owned.1,
            fix_round_id: ev.fix_round_id,
            round_sequence: round_seq,
            kind: ev.kind,
            status: ev.status,
            healthcheck_result: ev.healthcheck_result,
            judge_result: ev.judge_result,
            finished_at: ev.finished_at,
        });
    }
    UniResponse::ok(out.into()).into()
}

/// GET .../scores —— 我的总分 + 明细历史。
#[get("awdp/runs/{run_id}/scores")]
pub async fn get_my_scores(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpRunScoresDto> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let total = score_repo::my_total(ctx.db.get_ref(), run.id, Some(user.id), None).await?;
    let history = score_repo::my_history(ctx.db.get_ref(), run.id, Some(user.id), None).await?;
    UniResponse::ok(
        AwdpRunScoresDto {
            total,
            history: history
                .into_iter()
                .map(|s| AwdpScoreEventDto {
                    id: s.id,
                    gamebox_id: s.gamebox_id,
                    score_type: s.score_type,
                    fix_round_id: s.fix_round_id,
                    delta: s.delta,
                    created_at: s.created_at,
                })
                .collect(),
        }
        .into(),
    )
    .into()
}

/// GET .../stream —— run-scoped SSE（只对 run 属主可见）。
#[get("awdp/runs/{run_id}/stream")]
pub async fn run_stream(
    user: UserJwtGuard,
    ctx: ReqCtx,
    hub: web::Data<crate::infrastructure::realtime::BroadcastEventPublisher>,
    path: web::Path<Uuid>,
) -> actix_web::HttpResponse {
    use futures_util::stream::unfold;

    let run_id = path.into_inner();
    let user = user.into_inner();
    // 可见性：只对 run 属主开放。
    let owned = run_repo::find_by_id(ctx.db.get_ref(), run_id)
        .await
        .ok()
        .flatten()
        .map(|r| r.owner_user_id == Some(user.id))
        .unwrap_or(false);
    if !owned {
        return actix_web::HttpResponse::Forbidden().finish();
    }
    let rx = hub.subscribe();

    let body = unfold((rx, false, run_id), |(mut rx, primed, run_id)| async move {
        if !primed {
            return Some((
                Ok::<_, actix_web::Error>(web::Bytes::from(": connected\n\n")),
                (rx, true, run_id),
            ));
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if ev.run_id != Some(run_id) {
                        continue;
                    }
                    match serde_json::to_string(&ev) {
                        Ok(json) => {
                            return Some((
                                Ok(web::Bytes::from(format!("data: {json}\n\n"))),
                                (rx, true, run_id),
                            ));
                        }
                        Err(_) => continue,
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let payload = serde_json::json!({
                        "type": "stream.lagged",
                        "run_id": run_id,
                    });
                    return Some((
                        Ok(web::Bytes::from(format!("data: {payload}\n\n"))),
                        (rx, true, run_id),
                    ));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    actix_web::HttpResponse::Ok()
        .insert_header((actix_web::http::header::CONTENT_TYPE, "text/event-stream"))
        .insert_header((actix_web::http::header::CACHE_CONTROL, "no-cache"))
        .insert_header((actix_web::http::header::CONNECTION, "keep-alive"))
        .streaming(body)
}

// ────────────────────────────────────────────────────────────────────────────
// DTO builder
// ────────────────────────────────────────────────────────────────────────────

/// 构建 run view-model（practice：单 gamebox + 单实例）。
async fn build_run_dto(
    db: &sea_orm::DatabaseConnection,
    run: &awdp_runs::Model,
    user_id: Uuid,
) -> Result<AwdpRunDto, AppError> {
    let gamebox_id = run
        .gamebox_id
        .ok_or_else(|| AppError::BadRequest("practice run 必须携带 gamebox".into()))?;
    let gamebox = event_gamebox_repo::find_gamebox_identity(db, gamebox_id).await?;
    let my_score = score_repo::my_total(db, run.id, Some(user_id), None).await?;
    let instances = runtime::list_instances(db, run.id).await?;
    Ok(AwdpRunDto {
        run_id: run.id,
        gamebox_id,
        gamebox_name: gamebox.name.clone(),
        gamebox_category: gamebox.category.clone(),
        gamebox_description: gamebox.description.clone(),
        event_id: run.event_id,
        phase: run.phase.clone(),
        break_duration_secs: run.break_duration_secs,
        fix_duration_secs: run.fix_duration_secs,
        fix_round_interval_secs: run.fix_round_interval_secs,
        break_score: run.break_score,
        fix_round_score: run.fix_round_score,
        total_rounds: run.total_rounds,
        started_at: run.started_at,
        break_ends_at: run.break_ends_at,
        fix_started_at: run.fix_started_at,
        fix_ends_at: run.fix_ends_at,
        finished_at: run.finished_at,
        current_round: run.current_round,
        next_action_at: run.next_action_at,
        my_score,
        source_code_dir: if run.phase == AwdpPhase::Fix {
            gamebox.awdp_source_code_dir.clone()
        } else {
            None
        },
        instances: instances.iter().map(RunInstanceDto::from).collect(),
    })
}

/// 路由注册（挂 /api/service scope；与 /events scope 平级，见 bootstrap/routes.rs）。
pub fn configure_training_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(list_catalog)
        .service(start_training)
        .service(get_run_overview)
        .service(stop_run)
        .service(reset_run)
        .service(train_again)
        .service(submit_break_flag)
        .service(download_source)
        .service(upload_patch)
        .service(manual_test_check)
        .service(start_my_instance)
        .service(get_my_instance)
        .service(stop_my_instance)
        .service(reset_my_instance)
        .service(get_rounds)
        .service(get_my_evaluations)
        .service(get_my_scores)
        .service(run_stream);
}
