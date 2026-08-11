//! 选手端赛事 HTTP 处理器——薄适配 `player_service`。

use crate::api::dto::map_dto_vec;

use crate::modules::event::common::api::EventAnnouncementsDto;
use crate::modules::event::common::api::EventTeamsDto;
use crate::modules::event::common::api::EventUsersDto;
use crate::modules::event::jeopardy::api::InstancesDto;
use crate::{
    api::{apply_filters, prelude::*},
    entity::{challenge_instances, event_announcements, event_teams, event_users, events},
    modules::event::common::application::player_service::{self as svc},
    modules::platform::files::download::presign_private_download_url,
};

// Re-export DTOs / adapters for other handlers (admin dashboard, etc.).
pub use crate::modules::event::common::application::player_service::{
    CreateUserTeam, EventChallengeResult, EventInfo, EventInstanceResult, EventStatus,
    EventTeamMemberResult, EventTeamResult, get_scoreboard as __get_scoreboard,
    get_trend as __get_trend,
};
pub use crate::modules::event::jeopardy::domain::{
    scoreboard::ScoreboardItem,
    trend::{TrendItem, TrendPoint},
};

/// GET /api/events
#[get("")]
pub async fn get_events(
    user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<EventInfo>> {
    let user = user.into_inner();
    let query_params = query_params.0;
    let mappings = svc::player_event_filter_mappings();

    let stmt = events::Entity::find().filter(events::Column::Hidden.eq(false));
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(events::Column::UpdatedAt);

    let events_with_users = stmt
        .find_with_related(event_users::Entity)
        .all(ctx.db.get_ref())
        .await?;

    let result = svc::list_events_for_user(user.id, events_with_users);
    UniResponse::ok(result.into()).into()
}

/// GET /api/events/{event_id}
#[get("/{event_id}")]
pub async fn get_event(user: UserJwtGuard, ctx: ReqCtx, id: Path<Uuid>) -> UniResult<EventInfo> {
    let user = user.into_inner();
    let info = svc::get_event_info(ctx.db.get_ref(), *id, user.id).await?;
    UniResponse::ok(info.into()).into()
}

/// GET /api/events/{event_id}/capabilities
#[get("/{event_id}/capabilities")]
pub async fn get_event_capabilities(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    id: Path<Uuid>,
) -> UniResult<crate::modules::event::common::domain::capability::EventCapabilities> {
    let event = events::Entity::find_by_id(*id)
        .filter(events::Column::Hidden.eq(false))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;
    let caps = crate::modules::event::common::domain::capability::EventCapabilities::for_mode(
        &event.mode_unchecked(),
    );
    UniResponse::ok(Some(caps)).into()
}

/// GET /api/events/{event_id}/challenges
#[get("/{event_id}/challenges")]
pub async fn get_event_challenges(
    user: UserJwtGuard,
    ctx: ReqCtx,
    id: Path<Uuid>,
) -> UniResult<Vec<EventChallengeResult>> {
    let user = user.into_inner();
    let result = svc::list_event_challenges(&ctx.db, *id, &user).await?;
    UniResponse::ok(result.into()).into()
}

/// GET /api/events/{event_id}/instances
#[get("/{event_id}/instances")]
pub async fn get_event_instances(
    user: UserJwtGuard,
    ctx: ReqCtx,
    id: Path<Uuid>,
) -> UniResult<Vec<EventInstanceResult>> {
    let user = user.into_inner();
    let result = svc::list_event_instances(ctx.db.clone(), ctx.docker.clone(), *id, user).await?;
    UniResponse::ok(result.into()).into()
}

/// GET /api/events/{event_id}/challenges/{challenge_id}/instance
#[get("/{event_id}/challenges/{challenge_id}/instance")]
pub async fn get_event_challenge_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    id: Path<(Uuid, Uuid)>,
) -> UniResult<InstancesDto> {
    let user = user.into_inner();
    let (event_id, challenge_id) = id.into_inner();
    let instance = svc::get_challenge_instance(
        ctx.db.clone(),
        ctx.docker.clone(),
        event_id,
        challenge_id,
        user,
    )
    .await?;
    UniResponse::ok(Some(instance.into())).into()
}

/// POST /api/events/{event_id}/team
#[post("/{event_id}/team")]
pub async fn create_team(
    user: UserJwtGuard,
    ctx: ReqCtx,
    id: Path<Uuid>,
    cut: Json<CreateUserTeam>,
) -> UniResult<EventTeamsDto> {
    let user = user.into_inner();
    let cut = cut.into_inner();
    let event_id = *id;
    let team_name = cut.name.clone();

    let team = svc::create_team(&ctx.db, event_id, user.id, cut.name).await?;

    // Need event model for event_log; reload cheaply.
    if let Ok(Some(event)) = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await
    {
        ctx.log
            .add_event_log(
                &event,
                "INFO",
                "CREATE_TEAM",
                json!({"team_name": team_name}),
                Some(user.id),
                Some(team.id),
                Some(&ctx.req),
            )
            .await;
    }

    UniResponse::ok(Some(team.into())).into()
}

/// DELETE /api/events/{event_id}/team/{team_id}
#[delete("/{event_id}/team/{team_id}")]
pub async fn quit_team(user: UserJwtGuard, ctx: ReqCtx, id: Path<(Uuid, Uuid)>) -> UniResult<()> {
    let user = user.into_inner();
    let (event_id, team_id) = id.into_inner();
    svc::quit_team(ctx.db.get_ref(), event_id, team_id, user.id).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENT",
            "QUIT_TEAM",
            format!("退出赛事 {} 的团队 {}", event_id, team_id).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// POST /api/events/{event_id}/team/{team_id}/join
#[post("/{event_id}/team/{team_id}/join")]
pub async fn join_team(user: UserJwtGuard, ctx: ReqCtx, id: Path<(Uuid, Uuid)>) -> UniResult<()> {
    let user = user.into_inner();
    let (event_id, team_id) = id.into_inner();
    let event_team = svc::join_team(ctx.db.get_ref(), event_id, team_id, user.id).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENT",
            "JOIN_TEAM",
            format!("加入赛事 {} 的团队 {}", event_id, event_team.id).as_str(),
            json!({"team_name": event_team.name}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// POST /api/events/{event_id}/team/{team_id}/leave
#[post("/{event_id}/team/{team_id}/leave")]
pub async fn leave_team(user: UserJwtGuard, ctx: ReqCtx, id: Path<(Uuid, Uuid)>) -> UniResult<()> {
    let user = user.into_inner();
    let (event_id, team_id) = id.into_inner();
    svc::leave_team(ctx.db.get_ref(), event_id, team_id, user.id).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENT",
            "LEAVE_TEAM",
            format!("离开赛事 {} 的团队 {}", event_id, team_id).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// POST /api/events/{event_id}/join
#[post("/{event_id}/join")]
pub async fn join_event(
    user: UserJwtGuard,
    ctx: ReqCtx,
    id: Path<Uuid>,
) -> UniResult<EventUsersDto> {
    let user = user.into_inner();
    let (event, eu) = svc::join_event(&ctx.db, *id, user.id).await?;

    ctx.log
        .add_event_log(
            &event,
            "INFO",
            "JOIN_EVENT",
            json!({}),
            Some(eu.user_id),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(eu.into())).into()
}

/// DELETE /api/events/{event_id}/leave
#[delete("/{event_id}/leave")]
pub async fn leave_event(user: UserJwtGuard, ctx: ReqCtx, id: Path<Uuid>) -> UniResult<u64> {
    let user = user.into_inner();
    let (event, rows) = svc::leave_event(&ctx.db, *id, user.id).await?;

    ctx.log
        .add_event_log(
            &event,
            "INFO",
            "LEAVE_EVENT",
            json!({}),
            Some(user.id),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(rows.into()).into()
}

/// GET /api/events/{event_id}/scoreboard
#[get("/{event_id}/scoreboard")]
pub async fn get_scoreboard(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
) -> UniResult<Vec<ScoreboardItem>> {
    let event_id = event_id.into_inner();
    let scoreboard = svc::get_scoreboard_for_player(ctx.db.clone(), event_id).await?;
    UniResponse::ok(scoreboard.into()).into()
}

/// GET /api/events/{event_id}/trend
#[get("/{event_id}/trend")]
pub async fn get_trend(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
) -> UniResult<Vec<TrendItem>> {
    let trend_items = svc::get_trend(ctx.db, *event_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("{}", e)))?;
    UniResponse::ok(trend_items.into()).into()
}

/// GET /api/events/{event_id}/announcements
#[get("/{event_id}/announcements")]
pub async fn get_announcements(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
) -> UniResult<Vec<EventAnnouncementsDto>> {
    let announcements = svc::list_announcements(ctx.db.get_ref(), *event_id).await?;
    UniResponse::ok(Some(map_dto_vec(announcements))).into()
}

#[get("/{event_id}/own_wp")]
pub async fn get_own_wp(
    user: UserJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
) -> UniResult<String> {
    let user = user.into_inner();
    let file_url = svc::own_writeup_file_url(&ctx.db, *event_id, &user).await?;
    let proxy_url = presign_private_download_url(ctx.rustfs, &file_url, 5 * 60)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to generate signed URL: {}", e)))?;
    UniResponse::ok(Some(proxy_url)).into()
}
