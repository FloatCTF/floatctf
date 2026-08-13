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
use std::collections::HashSet;
use uuid::Uuid;

use actix_multipart::form::{MultipartForm, tempfile::TempFile};

use crate::api::FilterMapping;
use crate::api::sea_orm_utils::{apply_filters, paginate_query};
use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::UserJwtGuard, prelude::*},
    entity::{awdp_instances, awdp_runs, gameboxes, sea_orm_active_enums::AwdpPhase},
    modules::event::awdp::{
        AwdpError,
        api::dto::*,
        repo::{
            break_repo, evaluation_repo, event_gamebox_repo, round_repo, run_repo, score_repo,
            writeup_repo,
        },
        service::{
            break_service, evaluation, patch_service, practice_service,
            runtime::{self, Subject},
        },
    },
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Value, sea_query::Expr};

/// 当前用户是否训练过该 GameBox：该用户对该 gamebox 的练习 run 至少启动过一次实例
/// （目录点击只创建冻结 run（无实例）不算；点过「开始」/ 训练过即算）。
/// 用法：`Expr::cust_with_values(SQL, vec![Value::from(user_id)])`。
const SOLVED_GAMEBOX_CONDITION_SQL: &str = r#"EXISTS (
    SELECT 1 FROM public.awdp_instances i
    WHERE i.gamebox_id = gameboxes.id
      AND i.owner_user_id = $1
)"#;
const NOT_SOLVED_GAMEBOX_CONDITION_SQL: &str = r#"NOT EXISTS (
    SELECT 1 FROM public.awdp_instances i
    WHERE i.gamebox_id = gameboxes.id
      AND i.owner_user_id = $1
)"#;

/// 取当前用户训练过（至少启动过一次实例）的 gamebox id 集合。
pub async fn solved_gamebox_ids_for<C: sea_orm::ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<HashSet<Uuid>, sea_orm::DbErr> {
    let rows = awdp_instances::Entity::find()
        .filter(awdp_instances::Column::OwnerUserId.eq(user_id))
        .select_only()
        .column(awdp_instances::Column::GameboxId)
        .distinct()
        .into_tuple::<(Uuid,)>()
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|(gamebox_id,)| gamebox_id).collect())
}

/// 练习动作留痕：写入 run 所属（虚拟训练）赛事的 event_logs，供 admin Logs Tab 查看。
/// 留痕失败不阻塞用户操作。
async fn log_practice_action(
    ctx: &ReqCtx,
    event_id: Uuid,
    user_id: Uuid,
    action: &str,
    details: serde_json::Value,
) {
    crate::modules::event::common::application::event_log_service::insert_event_log(
        ctx.db.get_ref(),
        event_id,
        Some(user_id),
        None,
        "info",
        action,
        details,
    )
    .await;
}

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
    page: Option<u64>,
    limit: Option<u64>,
    filter: Option<String>,
}

/// GET /api/service/gameboxes?capability=awdp —— AWDP-capable 安全目录（支持分页/过滤）。
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

    // 一次批量查询取当前用户训练过的 gamebox 集合（solved 列 + solved 筛选共用）。
    let solved_gamebox_ids = solved_gamebox_ids_for(db, user.id).await?;

    // AWDP capability = 完整五列（source.zip 产物存在，DB CHECK 保证全有/全无），
    // 直接下沉到 WHERE，保证分页计数只统计 AWDP-capable 行。
    let stmt = gameboxes::Entity::find()
        .filter(gameboxes::Column::Hidden.eq(false))
        .filter(gameboxes::Column::BuildStatus.eq(Some(
            crate::modules::gamebox::BUILD_STATUS_READY.to_string(),
        )))
        .filter(gameboxes::Column::AwdpSourceArtifactKey.is_not_null())
        .order_by_asc(gameboxes::Column::Name);

    // 过滤：与 challenges 列表同款 FilterMapping（name/category/description）。
    let mappings = [
        FilterMapping {
            key: "name",
            column: Box::new(|v: &str| Condition::all().add(gameboxes::Column::Name.contains(v))),
        },
        FilterMapping {
            key: "category",
            column: Box::new(|v: &str| {
                Condition::all().add(gameboxes::Column::Category.contains(v))
            }),
        },
        FilterMapping {
            key: "description",
            column: Box::new(|v: &str| {
                Condition::all().add(gameboxes::Column::Description.contains(v))
            }),
        },
        FilterMapping {
            key: "solved",
            column: Box::new(move |v: &str| {
                let want_solved = matches!(
                    v.trim().to_lowercase().as_str(),
                    "true" | "1" | "yes" | "y" | "是"
                );
                Condition::all().add(Expr::cust_with_values(
                    if want_solved {
                        SOLVED_GAMEBOX_CONDITION_SQL
                    } else {
                        NOT_SOLVED_GAMEBOX_CONDITION_SQL
                    },
                    vec![Value::from(user.id)],
                ))
            }),
        },
    ];
    let stmt = apply_filters(stmt, query.filter.clone(), &mappings);

    let (rows, total) = if let (Some(limit), Some(page)) = (query.limit, query.page) {
        paginate_query(stmt, db, limit, page).await?
    } else {
        let rows = stmt.all(db).await?;
        let total = rows.len();
        (rows, total)
    };

    let mut out = Vec::new();
    for gb in rows {
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
            author: gb.username.clone(),
            updated_at: gb.updated_at,
            awdp_capable: true,
            recommended_cpu_millis: gb.recommended_cpu_millis,
            recommended_memory_bytes: gb.recommended_memory_bytes,
            recommended_pids_limit: gb.recommended_pids_limit,
            solved: solved_gamebox_ids.contains(&gb.id),
            active_training,
        });
    }

    let meta = QueryParams {
        offset: None,
        limit: query.limit,
        page: query.page,
        filter: query.filter.clone(),
        total: Some(total),
    };
    UniResponse::ok_meta(Some(out.into()), Some(meta)).into()
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
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.start",
        json!({ "run_id": run.id, "gamebox_id": gamebox_id }),
    )
    .await;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id, &ctx.config.awdp).await?;
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
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id, &ctx.config.awdp).await?;
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
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.stop",
        json!({ "run_id": run.id }),
    )
    .await;
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
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.reset",
        json!({ "run_id": run.id }),
    )
    .await;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id, &ctx.config.awdp).await?;
    UniResponse::ok(dto.into()).into()
}

/// POST /api/service/awdp/runs/{run_id}/start —— 练习「开始」：
/// 冻结 run（创建后/End 后 next_action_at=None）→ 回卷全新 Break 并解除冻结（重新计时）；
/// 已开始过的 run（next_action_at 非空，如手动停实例后恢复）→ 仅启动实例继续会话。
/// 返回 AwdpRunDto（实例已 running → 前端显现面板与内容，与 Challenge 练习 Launch 同效）。
#[post("awdp/runs/{run_id}/start")]
pub async fn start_run(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpRunDto> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    if matches!(run.phase, AwdpPhase::Pending | AwdpPhase::Ended) {
        return Err(AppError::from(AwdpError::InvalidState(format!(
            "只有 Break/Fix 阶段的练习 run 可以开始（当前 {:?}）",
            run.phase
        ))));
    }
    let gamebox_id = run.gamebox_id.ok_or_else(|| {
        AppError::from(AwdpError::InvalidState("只有练习 run 支持手动开始".into()))
    })?;

    // §55 Pending 语义：Pending = 未 Launch → Launch（Pending→Break，启动时钟）；
    // 已 Break/Fix（会话中途）→ 仅启动实例。
    if run.phase == AwdpPhase::Pending {
        run_repo::launch_practice_run(ctx.db.get_ref(), run.id).await?;
    }

    runtime::start_instance(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        &ctx.config.awdp,
        run.id,
        gamebox_id,
        Subject::user(user.id),
        &flag_prefix(&ctx).await,
    )
    .await
    .map_err(AppError::from)?;

    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.begin",
        json!({ "run_id": run.id, "gamebox_id": gamebox_id }),
    )
    .await;
    let run = run_repo::require_by_id(ctx.db.get_ref(), run.id).await?;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id, &ctx.config.awdp).await?;
    UniResponse::ok(dto.into()).into()
}

/// POST /api/service/awdp/runs/{run_id}/end —— 练习「End」：停止实例并结束 run。
/// §54：**保留** Score/Break 历史（End 不删除账本）；Train Again 另建新 run 从 0 计分。
#[post("awdp/runs/{run_id}/end")]
pub async fn end_run(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpRunDto> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    if run.gamebox_id.is_none() {
        return Err(AppError::from(AwdpError::InvalidState(
            "只有练习 run 支持手动结束".into(),
        )));
    }
    if matches!(run.phase, AwdpPhase::Pending | AwdpPhase::Ended) {
        return Err(AppError::from(AwdpError::InvalidState(format!(
            "只有 Break/Fix 阶段的练习 run 可以结束（当前 {:?}）",
            run.phase
        ))));
    }

    // 1. 停止全部实例（保留逻辑实例/端点）。
    let views = runtime::list_instances(ctx.db.get_ref(), run.id).await?;
    for v in views {
        runtime::stop_instance(
            ctx.db.get_ref(),
            ctx.docker.get_ref(),
            v.instance_id,
            Subject::user(user.id),
        )
        .await?;
    }

    // 2. run → Ended（§54：保留历史；Train Again 另建新 run）。
    run_repo::end_practice_session(ctx.db.get_ref(), run.id).await?;

    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.end",
        json!({ "run_id": run.id }),
    )
    .await;
    let run = run_repo::require_by_id(ctx.db.get_ref(), run.id).await?;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id, &ctx.config.awdp).await?;
    UniResponse::ok(dto.into()).into()
}

#[derive(Debug, Deserialize)]
pub struct SetPhaseForm {
    /// 目标阶段：break 或 fix（练习模式手动控制）。
    pub phase: String,
}

/// POST /api/service/awdp/runs/{run_id}/phase —— 练习模式手动控制阶段（直接 break / 直接 fix）。
/// 仅 practice run：break→fix 立即进入修复阶段（重置实例 pristine）；
/// fix→break 撤销整个 fix 会话（回合/评估/计分清零，重新物化全新时间线）。
#[post("awdp/runs/{run_id}/phase")]
pub async fn set_phase(
    user: UserJwtGuard,
    ctx: ReqCtx,
    state: web::Data<crate::bootstrap::AppState>,
    path: web::Path<Uuid>,
    form: Json<SetPhaseForm>,
) -> UniResult<AwdpRunDto> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    if run.gamebox_id.is_none() {
        return Err(AppError::InvalidState("手动控制阶段仅练习模式可用".into()));
    }
    let target = form.phase.as_str();
    match target {
        "fix" => match run.phase {
            AwdpPhase::Fix => {}
            AwdpPhase::Break => {
                crate::modules::event::awdp::service::event_service::transition_break_to_fix(
                    ctx.db.get_ref(),
                    ctx.docker.get_ref(),
                    ctx.config.auth.jwt_secret.expose().as_bytes(),
                    run.id,
                )
                .await
                .map_err(AppError::from)?;
                crate::modules::event::awdp::realtime::run_phase_changed(&state, run.id, "fix");
            }
            other => {
                return Err(AppError::InvalidState(format!(
                    "当前阶段 {:?} 不能直接进入 Fix",
                    other
                )));
            }
        },
        "break" => match run.phase {
            AwdpPhase::Break => {}
            AwdpPhase::Fix => {
                crate::modules::event::awdp::service::event_service::transition_fix_to_break(
                    ctx.db.get_ref(),
                    run.id,
                )
                .await
                .map_err(AppError::from)?;
                crate::modules::event::awdp::realtime::run_phase_changed(&state, run.id, "break");
            }
            other => {
                return Err(AppError::InvalidState(format!(
                    "当前阶段 {:?} 不能回到 Break",
                    other
                )));
            }
        },
        other => {
            return Err(AppError::Validation(format!(
                "phase 只能是 break 或 fix（got {other:?}）"
            )));
        }
    }
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.phase",
        json!({ "run_id": run.id, "phase": target }),
    )
    .await;
    let run = run_repo::require_by_id(ctx.db.get_ref(), run.id)
        .await
        .map_err(AppError::from)?;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id, &ctx.config.awdp).await?;
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
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.restart",
        json!({ "run_id": run.id, "old_run_id": old_run_id }),
    )
    .await;
    let dto = build_run_dto(ctx.db.get_ref(), &run, user.id, &ctx.config.awdp).await?;
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
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.break_submit",
        json!({ "run_id": run.id, "gamebox_id": gamebox_id, "scored": result.scored }),
    )
    .await;
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
    log_practice_action(
        &ctx,
        run.event_id,
        _user.id,
        "awdp.train.source_download",
        json!({ "run_id": run.id, "gamebox_id": gamebox_id }),
    )
    .await;
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
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.patch",
        json!({ "run_id": run.id, "gamebox_id": gamebox_id, "status": status }),
    )
    .await;
    UniResponse::ok(
        PatchSubmitResponse {
            status: status.into(),
        }
        .into(),
    )
    .into()
}

/// POST .../test-check —— 手动 Test Check 入队（异步；worker 执行 healthcheck + judge，不计分）。
#[post("awdp/runs/{run_id}/gameboxes/{gamebox_id}/test-check")]
pub async fn manual_test_check(
    user: UserJwtGuard,
    ctx: ReqCtx,
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
    let evaluation = evaluation::manual_check_enqueue(
        ctx.db.get_ref(),
        run.id,
        view.instance_id,
        Subject::user(user.id),
    )
    .await
    .map_err(AppError::from)?;
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.test_check",
        json!({ "run_id": run.id, "gamebox_id": gamebox_id, "evaluation_id": evaluation.id }),
    )
    .await;
    UniResponse::ok(
        ManualCheckDto {
            evaluation_id: evaluation.id,
            status: "pending".to_string(),
            healthcheck_ok: None,
            healthcheck_detail: None,
            judge_ok: None,
            judge_detail: None,
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
        &ctx.config.awdp,
        run.id,
        gamebox_id,
        Subject::user(user.id),
        &flag_prefix(&ctx).await,
    )
    .await
    .map_err(AppError::from)?;
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.instance_start",
        json!({ "run_id": run.id, "gamebox_id": gamebox_id }),
    )
    .await;
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
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.instance_stop",
        json!({ "run_id": run.id, "gamebox_id": gamebox_id }),
    )
    .await;
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
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.instance_reset",
        json!({ "run_id": run.id, "gamebox_id": gamebox_id }),
    )
    .await;
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

/// GET .../writeup —— 我的 Writeup（练习 run 属主可读写；无记录返回空内容）。
#[get("awdp/runs/{run_id}/writeup")]
pub async fn get_run_writeup(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpRunWriteupDto> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let row = writeup_repo::find_by_run(ctx.db.get_ref(), run.id).await?;
    UniResponse::ok(
        AwdpRunWriteupDto {
            run_id: run.id,
            content: row.as_ref().map(|r| r.content.clone()).unwrap_or_default(),
            updated_at: row.map(|r| r.updated_at),
        }
        .into(),
    )
    .into()
}

#[derive(Debug, Deserialize)]
pub struct SaveAwdpRunWriteupForm {
    pub content: String,
}

/// PUT .../writeup —— 保存我的 Writeup（一 run 一份，upsert）。
#[put("awdp/runs/{run_id}/writeup")]
pub async fn save_run_writeup(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    form: Json<SaveAwdpRunWriteupForm>,
) -> UniResult<AwdpRunWriteupDto> {
    let run_id = path.into_inner();
    let user = user.into_inner();
    let run = require_owned_run(ctx.db.get_ref(), run_id, user.id).await?;
    let db = ctx.db.get_ref();
    let existed = writeup_repo::find_by_run(db, run.id).await?.is_some();
    let row = writeup_repo::upsert(db, run.id, user.id, form.content.clone()).await?;
    let action = if existed { "UPDATE" } else { "CREATE" };
    ctx.log
        .add_log(
            "INFO",
            "WRITEUP",
            action,
            format!(
                "{} AWDP Run {} 的 Writeup",
                if existed { "更新" } else { "创建" },
                run.id
            )
            .as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;
    log_practice_action(
        &ctx,
        run.event_id,
        user.id,
        "awdp.train.writeup",
        json!({ "run_id": run.id }),
    )
    .await;
    UniResponse::ok(
        AwdpRunWriteupDto {
            run_id: run.id,
            content: row.content,
            updated_at: Some(row.updated_at),
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
    awdp_config: &crate::core::config::AwdpStaticConfig,
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
        event_id: Some(run.event_id),
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
        judge_endpoint: Some(JudgeEndpointDto {
            base_url: format!(
                "http://{}:{}",
                awdp_config.practice_judge_data_host,
                crate::modules::event::awdp::domain::judge::PRACTICE_JUDGE_PORT
            ),
            flag_url: format!(
                "http://{}:{}/flag",
                awdp_config.practice_judge_data_host,
                crate::modules::event::awdp::domain::judge::PRACTICE_JUDGE_PORT
            ),
            scope: "gamebox_internal".to_string(),
        }),
    })
}

/// 路由注册（挂 /api/service scope；与 /events scope 平级，见 bootstrap/routes.rs）。
pub fn configure_training_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(list_catalog)
        .service(start_training)
        .service(get_run_overview)
        .service(stop_run)
        .service(start_run)
        .service(end_run)
        .service(reset_run)
        .service(set_phase)
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
        .service(get_run_writeup)
        .service(save_run_writeup)
        .service(run_stream);
}
