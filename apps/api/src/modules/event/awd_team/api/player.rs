//! AWD player-facing API handlers.

use actix_web::{HttpResponse, web};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::UserJwtGuard, prelude::*},
    modules::event::awd_team::{
        repo::{gamebox_repo, round_repo},
        service::{flag_service, score_service, submission_service},
    },
    modules::event::common::infrastructure::event_repository as repo,
};

use super::dto::*;

use actix_web::{get, post};

/// GET /api/events/{event_id}/awd/gameboxes
#[get("/events/{event_id}/awd/gameboxes")]
pub async fn get_my_gameboxes(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<GameBoxResponse>> {
    let event_id = path.into_inner();
    let user = user.into_inner();

    // Find user's team for this event (using centralized repository)
    let membership = repo::find_user_team_membership(ctx.db.get_ref(), event_id, user.id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("You are not in a team for this event".into()))?;

    let instances =
        gamebox_repo::find_instances_by_team(ctx.db.get_ref(), event_id, membership.team_id)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

    let response: Vec<GameBoxResponse> = instances
        .into_iter()
        .map(|i| GameBoxResponse {
            id: i.id,
            team_id: i.team_id,
            event_gamebox_id: i.event_gamebox_id,
            status: format!("{:?}", i.status).to_lowercase(),
            gamebox_ip: i.gamebox_ip,
            container_name: i.container_name,
            health_status: i.health_status,
        })
        .collect();

    UniResponse::ok(response.into()).into()
}

/// POST /api/events/{event_id}/awd/gameboxes/{instance_id}/reset
#[post("/events/{event_id}/awd/gameboxes/{instance_id}/reset")]
pub async fn reset_my_gamebox(
    user: UserJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (event_id, instance_id) = path.into_inner();
    let user = user.into_inner();

    // Find user's team for this event (using centralized repository)
    let membership = repo::find_user_team_membership(ctx.db.get_ref(), event_id, user.id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("You are not in a team for this event".into()))?;

    // P5-10 限流：reset（每队伍每小时）
    awd.rate_limiter
        .check(
            ctx.db.get_ref(),
            crate::infrastructure::ratelimit::RateScope::Reset,
            &membership.team_id.to_string(),
        )
        .await
        .map_err(AppError::from)?;

    // P4-2：player reset 接入完整 execute_reset 流程（同 IP/同密码/Ready/ResetFailed）
    crate::modules::event::awd_team::service::reset_service::execute_reset(
        ctx.db.get_ref(),
        awd.containers.as_ref(),
        crate::modules::event::awd_team::service::reset_service::ResetContext {
            event_id,
            instance_id,
            team_id: membership.team_id,
            actor: crate::modules::event::awd_team::service::reset_service::ResetActor::Player {
                user_id: user.id,
                team_id: membership.team_id,
            },
        },
    )
    .await
    .map_err(AppError::from)?;

    UniResponse::ok_none().into()
}

/// POST /api/events/{event_id}/awd/submissions
#[post("/events/{event_id}/awd/submissions")]
pub async fn submit_flag(
    user: UserJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
    body: web::Json<SubmitFlagRequest>,
) -> UniResult<SubmissionResponse> {
    let event_id = path.into_inner();
    let user = user.into_inner();

    // P5-10 限流：submit（每用户每分钟）
    awd.rate_limiter
        .check(
            ctx.db.get_ref(),
            crate::infrastructure::ratelimit::RateScope::Submit,
            &user.id.to_string(),
        )
        .await
        .map_err(AppError::from)?;

    // Find user's team for this event (using centralized repository)
    let membership = repo::find_user_team_membership(ctx.db.get_ref(), event_id, user.id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("You are not in a team for this event".into()))?;

    let attacker_team_id = membership.team_id;

    // Validate the submission
    let (flag_issue_id, victim_team_id, gamebox_instance_id) = flag_service::validate_submission(
        ctx.db.get_ref(),
        event_id,
        &body.flag,
        attacker_team_id,
        user.id,
    )
    .await
    .map_err(AppError::from)?;

    // Get active round
    let round = round_repo::find_active_round(ctx.db.get_ref(), event_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("No active round".into()))?;

    // 从 Instance → EventGameBox 解析计分配置（§28：攻击分属于 EventGameBox）
    let instance = gamebox_repo::find_instance_by_id(ctx.db.get_ref(), gamebox_instance_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .unwrap();
    let resolved =
        crate::modules::event::awd_team::service::gamebox_service::resolve_event_gamebox_spec(
            ctx.db.get_ref(),
            instance.event_gamebox_id,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Process the submission
    let result = submission_service::process_submission(
        ctx.db.get_ref(),
        event_id,
        round.id,
        flag_issue_id,
        attacker_team_id,
        victim_team_id,
        gamebox_instance_id,
        user.id,
        resolved.event_gamebox.break_points,
        resolved.event_gamebox.loss_points,
        resolved.event_gamebox.first_bonus,
        resolved.event_gamebox.id,
        awd.publisher.as_ref(),
    )
    .await
    .map_err(AppError::from)?;

    UniResponse::ok(
        SubmissionResponse {
            success: true,
            attack_score: result.attack_score_delta,
            victim_loss: result.victim_loss_delta,
            first_bonus: result.first_bonus_delta,
            was_first_blood: result.was_first_blood,
        }
        .into(),
    )
    .into()
}

/// GET /api/events/{event_id}/awd/scores
#[get("/events/{event_id}/awd/scores")]
pub async fn get_scores(
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<super::super::domain::TeamScore>> {
    let event_id = path.into_inner();

    // Get all teams for the event
    use crate::entity::event_teams;
    let teams = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .all(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let team_info: Vec<(Uuid, String)> = teams.into_iter().map(|t| (t.id, t.name)).collect();

    let scores = score_service::get_scoreboard(ctx.db.get_ref(), event_id, &team_info)
        .await
        .map_err(AppError::from)?;

    UniResponse::ok(scores.into()).into()
}

// ── WireGuard endpoints ──

/// GET /api/events/{event_id}/awd/wireguard/config
#[get("/events/{event_id}/awd/wireguard/config")]
pub async fn get_wireguard_config(
    user: UserJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::AwdDependencies>,
    path: web::Path<Uuid>,
) -> UniResult<super::dto::WireGuardConfigResponse> {
    let event_id = path.into_inner();
    let user = user.into_inner();

    use crate::entity::{awd_events, event_team_members};

    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user.id))
        .one(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Not in a team".into()))?;

    let (peer, peer_privkey) =
        crate::modules::event::awd_team::service::wireguard_service::ensure_peer_for_user(
            ctx.db.get_ref(),
            awd.crypto.as_ref(),
            awd.network.as_ref(),
            &ctx.config.awd,
            event_id,
            user.id,
            membership.team_id,
        )
        .await
        .map_err(AppError::from)?;

    // P1-15 私钥一次返回：首次拉取才返回私钥（此后 config_fetched_at 置位）。
    // 需要轮换/重新获取走 admin 轮换路径。
    if peer.config_fetched_at.is_some() {
        return Err(AppError::Forbidden(
            "WireGuard 私钥仅首次拉取返回；如需重新获取请联系管理员轮换密钥".into(),
        ));
    }
    crate::modules::event::awd_team::repo::wireguard_repo::mark_wg_config_fetched(
        ctx.db.get_ref(),
        peer.id,
    )
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let awd_event = awd_events::Entity::find()
        .filter(awd_events::Column::EventId.eq(event_id))
        .one(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("AWD event not found".into()))?;

    let server_pubkey = awd_event
        .wg_server_public_key
        .ok_or_else(|| AppError::Internal("Server public key not configured".into()))?;

    use crate::entity::awd_team_networks;
    let team_net = awd_team_networks::Entity::find()
        .filter(awd_team_networks::Column::EventId.eq(event_id))
        .filter(awd_team_networks::Column::TeamId.eq(membership.team_id))
        .one(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Team network not configured".into()))?;

    let endpoint_host = crate::infrastructure::settings::get_setting(ctx.db.get_ref(), "NODE_IP")
        .await
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    let config = crate::modules::event::awd_team::service::wireguard_service::build_client_config(
        &peer.assigned_ip,
        &peer_privkey,
        &server_pubkey,
        &endpoint_host,
        awd_event.wireguard_listen_port as u16,
        &awd_event.gamebox_cidr,
        &team_net.wireguard_subnet,
    );

    UniResponse::ok(super::dto::WireGuardConfigResponse { config }.into()).into()
}

/// GET /api/events/{event_id}/awd/stream
/// GET /api/events/{event_id}/awd/stream
/// Server-Sent Events from the in-process `BroadcastEventPublisher` hub.
/// Clients filter by `event_id` in the envelope; reconnect + REST snapshot
/// is the responsibility of the frontend (`useAwdEventStream`).
#[get("/events/{event_id}/awd/stream")]
pub async fn event_stream(
    _user: UserJwtGuard,
    hub: web::Data<crate::infrastructure::realtime::BroadcastEventPublisher>,
    path: web::Path<Uuid>,
) -> HttpResponse {
    use futures_util::stream::unfold;

    let event_id = path.into_inner();
    let rx = hub.subscribe();

    let body = unfold(
        (rx, false, event_id),
        |(mut rx, primed, event_id)| async move {
            if !primed {
                return Some((
                    Ok::<_, actix_web::Error>(web::Bytes::from(": connected\n\n")),
                    (rx, true, event_id),
                ));
            }
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        if ev.event_id != event_id {
                            continue;
                        }
                        match serde_json::to_string(&ev) {
                            Ok(json) => {
                                return Some((
                                    Ok(web::Bytes::from(format!("data: {json}\n\n"))),
                                    (rx, true, event_id),
                                ));
                            }
                            Err(_) => continue,
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let payload = serde_json::json!({
                            "type": "stream.lagged",
                            "event_id": event_id,
                        });
                        return Some((
                            Ok(web::Bytes::from(format!("data: {payload}\n\n"))),
                            (rx, true, event_id),
                        ));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );

    HttpResponse::Ok()
        .insert_header((actix_web::http::header::CONTENT_TYPE, "text/event-stream"))
        .insert_header((actix_web::http::header::CACHE_CONTROL, "no-cache"))
        .insert_header((actix_web::http::header::CONNECTION, "keep-alive"))
        .streaming(body)
}
