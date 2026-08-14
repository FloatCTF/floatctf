//! AWDP 管理端路由（挂 /api/admin/events/{event_id}/awdp/*）。
//!
//! URL 保持 event-oriented；配置读写走 awdp_events（纯配置），
//! 生命周期操作（start/break-to-fix/finish）经 active competition run。

use actix_web::web::{self, Json};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard, prelude::*},
    entity::sea_orm_active_enums::{AwdpPhase, EventFamily},
    modules::event::awdp::{
        api::dto::*,
        domain::{AwdpConfig, AwdpConfigPatch},
        repo::{event_gamebox_repo, event_repo, instance_repo, run_repo, score_repo},
        service::runtime,
    },
};

async fn ensure_awdp_event(ctx: &ReqCtx, event_id: Uuid) -> Result<(), AppError> {
    let event = crate::entity::events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound("event not found".into()))?;
    if event.family != EventFamily::Awdp {
        return Err(AppError::Validation("not an AWDP event".into()));
    }
    event_repo::ensure_by_event_id(ctx.db.get_ref(), event_id, &AwdpConfig::default()).await?;
    Ok(())
}

async fn require_active_run(
    ctx: &ReqCtx,
    event_id: Uuid,
) -> Result<crate::entity::awdp_runs::Model, AppError> {
    run_repo::find_active_competition_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未启动（无 active run）".into()))
}

/// GET /api/admin/events/{event_id}/awdp —— 配置 + 阶段（运行态来自 active run）。
#[get("{event_id}/awdp")]
pub async fn get_awdp_config(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpEventConfigDto> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let row = event_repo::require_by_event_id(ctx.db.get_ref(), event_id).await?;
    let run = run_repo::find_active_competition_for_event(ctx.db.get_ref(), event_id).await?;
    UniResponse::ok(AwdpEventConfigDto::from_config_and_run(&row, run.as_ref()).into()).into()
}

/// PATCH /api/admin/events/{event_id}/awdp —— 配置更新（无 active run 可改，乐观锁）。
#[patch("{event_id}/awdp")]
pub async fn patch_awdp_config(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: Json<AwdpConfigPatchRequest>,
) -> UniResult<AwdpEventConfigDto> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let patch = AwdpConfigPatch {
        expected_updated_at: Some(body.expected_updated_at),
        break_duration_secs: body.break_duration_secs,
        fix_duration_secs: body.fix_duration_secs,
        fix_round_interval_secs: body.fix_round_interval_secs,
        break_score: body.break_score,
        fix_round_score: body.fix_round_score,
    };
    let row = event_repo::update_config(ctx.db.get_ref(), event_id, patch).await?;
    let run = run_repo::find_active_competition_for_event(ctx.db.get_ref(), event_id).await?;
    UniResponse::ok(AwdpEventConfigDto::from_config_and_run(&row, run.as_ref()).into()).into()
}

/// POST /api/admin/events/{event_id}/awdp/start —— 手动开始（create_competition_run + 立即 Break）。
#[post("{event_id}/awdp/start")]
pub async fn start_awdp_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    state: web::Data<crate::bootstrap::AppState>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let row = event_repo::require_by_event_id(ctx.db.get_ref(), event_id).await?;
    let config = AwdpConfig {
        break_duration_secs: row.break_duration_secs,
        fix_duration_secs: row.fix_duration_secs,
        fix_round_interval_secs: row.fix_round_interval_secs,
        break_score: row.break_score,
        fix_round_score: row.fix_round_score,
    };
    // 快照自 awdp_events 配置（幂等：已有 active run 直接返回）。
    let run = run_repo::create_competition_run(ctx.db.get_ref(), event_id, &config).await?;
    if run.phase == AwdpPhase::Pending {
        // 立即 pending → Break。
        let now = Utc::now();
        let break_ends = now + chrono::Duration::seconds(config.break_duration_secs as i64);
        run_repo::transition_phase(
            ctx.db.get_ref(),
            run.id,
            AwdpPhase::Pending,
            AwdpPhase::Break,
            run_repo::PhaseTransitionPatch {
                started_at: Some(now),
                break_ends_at: Some(break_ends),
                next_action_at: Some(break_ends),
                ..Default::default()
            },
        )
        .await?;
        crate::modules::event::awdp::realtime::phase_changed(&state, event_id, "break");
    }
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awdp/break-to-fix —— Break 到期 → Fix（重置全部实例 pristine）。
#[post("{event_id}/awdp/break-to-fix")]
pub async fn break_to_fix(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    state: web::Data<crate::bootstrap::AppState>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let run = require_active_run(&ctx, event_id).await?;
    crate::modules::event::awdp::service::event_service::transition_break_to_fix(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        run.id,
    )
    .await?;
    crate::modules::event::awdp::realtime::phase_changed(&state, event_id, "fix");
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awdp/finish —— 手动结束（Fix → Ended）。
#[post("{event_id}/awdp/finish")]
pub async fn finish_awdp_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    state: web::Data<crate::bootstrap::AppState>,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let run = require_active_run(&ctx, event_id).await?;
    run_repo::transition_phase(
        ctx.db.get_ref(),
        run.id,
        AwdpPhase::Fix,
        AwdpPhase::Ended,
        run_repo::PhaseTransitionPatch {
            finished_at: Some(Utc::now()),
            next_action_at: None,
            ..Default::default()
        },
    )
    .await?;
    crate::modules::event::awdp::realtime::phase_changed(&state, event_id, "ended");
    UniResponse::ok_none().into()
}

/// GET /api/admin/events/{event_id}/awdp/runs —— run 历史汇总（inspect）。
#[get("{event_id}/awdp/runs")]
pub async fn list_runs(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<AwdpAdminRunDto>> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let runs = run_repo::list_for_event(ctx.db.get_ref(), event_id).await?;
    let mut out = Vec::new();
    for run in runs {
        let instances = instance_repo::list_for_run(ctx.db.get_ref(), run.id).await?;
        let score_sum = score_repo::total_for_run(ctx.db.get_ref(), run.id)
            .await?
            .into_iter()
            .map(|(_, _, total)| total)
            .sum::<i64>();
        out.push(AwdpAdminRunDto {
            run_id: run.id,
            phase: run.phase,
            current_round: run.current_round,
            total_rounds: run.total_rounds,
            started_at: run.started_at,
            finished_at: run.finished_at,
            next_action_at: run.next_action_at,
            instance_count: instances.len(),
            score_sum,
        });
    }
    UniResponse::ok(out.into()).into()
}

/// POST /api/admin/events/{event_id}/awdp/gameboxes —— attach（仅完整 [awdp] capability）。
#[post("{event_id}/awdp/gameboxes")]
pub async fn attach_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: Json<AttachGameBoxRequest>,
) -> UniResult<AwdpAdminEventGameBoxDto> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let eg = event_gamebox_repo::attach_gamebox(
        ctx.db.get_ref(),
        event_id,
        body.gamebox_id,
        body.hidden.unwrap_or(false),
    )
    .await?;
    let gb = event_gamebox_repo::find_gamebox_identity(ctx.db.get_ref(), eg.gamebox_id).await?;
    UniResponse::ok(AwdpAdminEventGameBoxDto::from_join(&eg, &gb).into()).into()
}

/// DELETE /api/admin/events/{event_id}/awdp/gameboxes/{eg_id} —— detach。
#[delete("{event_id}/awdp/gameboxes/{eg_id}")]
pub async fn detach_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (_event_id, eg_id) = path.into_inner();
    event_gamebox_repo::detach_gamebox(ctx.db.get_ref(), eg_id).await?;
    UniResponse::ok_none().into()
}

/// GET /api/admin/events/{event_id}/awdp/gameboxes —— 已挂载列表。
#[get("{event_id}/awdp/gameboxes")]
pub async fn list_event_gameboxes(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<AwdpAdminEventGameBoxDto>> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let egs = event_gamebox_repo::list_for_event(ctx.db.get_ref(), event_id).await?;
    let mut out = Vec::new();
    for eg in egs {
        let gb = event_gamebox_repo::find_gamebox_identity(ctx.db.get_ref(), eg.gamebox_id).await?;
        out.push(AwdpAdminEventGameBoxDto::from_join(&eg, &gb));
    }
    UniResponse::ok(out.into()).into()
}

/// GET /api/admin/events/{event_id}/awdp/instances —— 实例 inspect（经 run 聚合）。
#[get("{event_id}/awdp/instances")]
pub async fn list_instances(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<AwdpAdminInstanceDto>> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let rows = instance_repo::list_for_event(ctx.db.get_ref(), event_id).await?;
    let mut out = Vec::new();
    for (instance, ext) in rows {
        let gb =
            event_gamebox_repo::find_gamebox_identity(ctx.db.get_ref(), ext.gamebox_id).await?;
        let endpoints = runtime::instance_endpoints_for(ctx.db.get_ref(), instance.id).await?;
        out.push(AwdpAdminInstanceDto {
            instance_id: instance.id,
            event_gamebox_id: ext.gamebox_id,
            gamebox_name: gb.name.clone(),
            owner_user_id: ext.owner_user_id,
            owner_team_id: ext.owner_team_id,
            runtime_state: instance.runtime_state.clone(),
            runtime_generation: instance.runtime_generation,
            container_name: instance.container_name.clone(),
            endpoints: endpoints
                .into_iter()
                .map(|e| EndpointDto {
                    protocol: e.protocol,
                    container_port: e.container_port as u16,
                    public_host: e.public_host,
                    public_port: e.public_port as u16,
                })
                .collect(),
        });
    }
    UniResponse::ok(out.into()).into()
}

/// GET /api/admin/events/{event_id}/awdp/scores —— 管理端积分榜（同 player 服务）。
#[get("{event_id}/awdp/scores")]
pub async fn get_scores(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<crate::modules::event::awdp::service::scoreboard::AwdpScoreRow>> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let event = crate::entity::events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound("event not found".into()))?;
    let rows =
        crate::modules::event::awdp::service::scoreboard::get_scoreboard(ctx.db.get_ref(), &event)
            .await
            .map_err(AppError::from)?;
    UniResponse::ok(rows.into()).into()
}

/// 路由注册（挂进 /api/admin/events scope，与 common 同组）。
pub fn admin_events_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_awdp_config)
        .service(patch_awdp_config)
        .service(start_awdp_event)
        .service(finish_awdp_event)
        .service(list_runs)
        .service(attach_gamebox)
        .service(detach_gamebox)
        .service(list_event_gameboxes)
        .service(list_instances)
        .service(get_scores)
        .service(break_to_fix);
}
