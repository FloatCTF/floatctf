//! AWDP 管理端路由（挂 /api/admin/events/{event_id}/awdp/*）。

use actix_web::web::{self, Json};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard, prelude::*},
    entity::sea_orm_active_enums::{AwdpPhase, EventFamily},
    modules::event::awdp::{
        api::dto::*,
        domain::{AwdpConfig, AwdpConfigPatch},
        repo::{event_gamebox_repo, event_repo, instance_repo},
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

/// GET /api/admin/events/{event_id}/awdp —— 配置 + 阶段。
#[get("{event_id}/awdp")]
pub async fn get_awdp_config(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpEventConfigDto> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    let row = event_repo::require_by_event_id(ctx.db.get_ref(), event_id).await?;
    UniResponse::ok(AwdpEventConfigDto::from(&row).into()).into()
}

/// PATCH /api/admin/events/{event_id}/awdp —— 配置更新（仅 pending 可改，乐观锁）。
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
    UniResponse::ok(AwdpEventConfigDto::from(&row).into()).into()
}

/// POST /api/admin/events/{event_id}/awdp/start —— 手动开始（pending → Break）。
#[post("{event_id}/awdp/start")]
pub async fn start_awdp_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
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
    let now = Utc::now();
    let break_ends = now + chrono::Duration::seconds(config.break_duration_secs as i64);
    event_repo::transition_phase(
        ctx.db.get_ref(),
        event_id,
        AwdpPhase::Pending,
        AwdpPhase::Break,
        event_repo::PhaseTransitionPatch {
            started_at: Some(now),
            break_ends_at: Some(break_ends),
            next_action_at: Some(break_ends),
            ..Default::default()
        },
    )
    .await?;
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awdp/break-to-fix —— Break 到期 → Fix（重置全部实例到 pristine）。
#[post("{event_id}/awdp/break-to-fix")]
pub async fn break_to_fix(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    crate::modules::event::awdp::service::event_service::transition_break_to_fix(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        event_id,
    )
    .await?;
    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/awdp/finish —— 手动结束（Fix → Ended）。
#[post("{event_id}/awdp/finish")]
pub async fn finish_awdp_event(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let event_id = path.into_inner();
    ensure_awdp_event(&ctx, event_id).await?;
    event_repo::transition_phase(
        ctx.db.get_ref(),
        event_id,
        AwdpPhase::Fix,
        AwdpPhase::Ended,
        event_repo::PhaseTransitionPatch {
            finished_at: Some(Utc::now()),
            next_action_at: None,
            ..Default::default()
        },
    )
    .await?;
    UniResponse::ok_none().into()
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

/// GET /api/admin/events/{event_id}/awdp/instances —— 实例 inspect。
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
        let eg = event_gamebox_repo::require_by_id(ctx.db.get_ref(), ext.event_gamebox_id).await?;
        let gb = event_gamebox_repo::find_gamebox_identity(ctx.db.get_ref(), eg.gamebox_id).await?;
        let endpoints = runtime::instance_endpoints_for(ctx.db.get_ref(), instance.id).await?;
        out.push(AwdpAdminInstanceDto {
            instance_id: instance.id,
            event_gamebox_id: eg.id,
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

/// 路由注册（挂进 /api/admin/events scope，与 common 同组）。
pub fn admin_events_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_awdp_config)
        .service(patch_awdp_config)
        .service(start_awdp_event)
        .service(finish_awdp_event)
        .service(attach_gamebox)
        .service(detach_gamebox)
        .service(list_event_gameboxes)
        .service(list_instances)
        .service(break_to_fix);
}
