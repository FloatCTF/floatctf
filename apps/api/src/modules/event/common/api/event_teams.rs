use std::str::FromStr;

use sea_orm::Condition;

use crate::modules::event::common::api::EventTeamMembersDto;
use crate::modules::event::common::api::EventTeamsDto;
use crate::{
    api::{
        FilterMapping, dto::DeleteItemsRequest, apply_filters, prelude::*,
        sea_orm_utils::paginate_query,
    },
    entity::{
        event_team_members, event_teams, event_users, events,
        sea_orm_active_enums::EventTeamMemberRole, users,
    },
};

#[derive(Debug, Serialize, Deserialize)]
pub struct AddTeamRequest {
    pub name: String,
    pub description: Option<String>,
}
/// POST /api/admin/events/{event_id}/teams
#[post("")]
pub async fn add_team(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    atr: Json<AddTeamRequest>,
) -> UniResult<EventTeamsDto> {
    let user = user.into_inner();
    let atr = atr.into_inner();
    let event_id = event_id.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    let new_event_team = event_teams::ActiveModel {
        event_id: Set(event.id),
        name: Set(atr.name),
        description: Set(atr.description),
        ..Default::default()
    };

    let event_team = new_event_team.insert(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_TEAMS",
            "CREATE",
            format!(
                "{} 为比赛 {} 创建队伍: {}",
                user.username, event.title, event_team.name
            )
            .as_str(),
            json!({"event_id": event.id, "team_name": event_team.name}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(event_team.into())).into()
}

/// DELETE /api/admin/events/{event_id}/teams
#[delete("")]
pub async fn remove_team(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<Uuid>,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let event_id = path.into_inner();
    let dir = dir.into_inner();
    let deleted_count = event_teams::Entity::delete_many()
        .filter(event_teams::Column::EventId.eq(event_id))
        .filter(event_teams::Column::Id.is_in(dir.id_list.clone()))
        .exec(ctx.db.get_ref())
        .await?
        .rows_affected;

    let d = event_users::Entity::delete_many()
        .filter(event_users::Column::EventId.eq(event_id))
        .exec(ctx.db.get_ref())
        .await?
        .rows_affected;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_TEAMS",
            "DELETE",
            format!("{} 删除 {} 支队伍", user.username, deleted_count).as_str(),
            json!({"event_id": event_id, "deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok((deleted_count + d).into()).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TeamMemberResult {
    pub username: String,
    pub nickname: String,
    pub role: EventTeamMemberRole,
    pub points: f64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct TeamResult {
    pub team: event_teams::Model,
    pub captain: String,
    pub members: Vec<TeamMemberResult>,
}

/// GET /api/admin/events/{event_id}/teams
#[get("")]
pub async fn get_teams(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<TeamResult>> {
    let event_id = event_id.into_inner();
    let mut query_params = query_params.0;

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_teams::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "name",
            column: Box::new(|v| Condition::all().add(event_teams::Column::Name.contains(v))),
        },
        FilterMapping {
            key: "points",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_teams::Column::Points.eq(v.parse::<f64>().unwrap_or(0.0)))
            }),
        },
        FilterMapping {
            key: "banned",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_teams::Column::Banned.eq(v.parse::<bool>().unwrap_or(false)))
            }),
        },
    ];

    let stmt = event.find_related(event_teams::Entity);
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(event_teams::Column::UpdatedAt);

    let (items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            paginate_query(stmt, ctx.db.get_ref(), limit, page).await?
        } else {
            let items = stmt.all(ctx.db.get_ref()).await?;
            (items.clone(), items.len())
        };

    let mut result = Vec::with_capacity(items.len());
    for team in items {
        let members = team
            .find_related(event_team_members::Entity)
            .find_also_related(users::Entity)
            .all(ctx.db.get_ref())
            .await?;
        let mut team_members = Vec::new();
        let mut captain = String::new();
        for (member, user) in members {
            if let Some(user) = user {
                if member.role == EventTeamMemberRole::Captain {
                    captain = user.username.clone();
                }
                let event_user = event_users::Entity::find()
                    .filter(event_users::Column::EventId.eq(event.id))
                    .filter(event_users::Column::UserId.eq(user.id))
                    .one(ctx.db.get_ref())
                    .await?
                    .ok_or(AppError::NotFound(format!(
                        "EventUser {} not exist",
                        user.id
                    )))?;

                team_members.push(TeamMemberResult {
                    username: user.username,
                    nickname: user.nickname,
                    role: member.role,
                    points: event_user.points,
                });
            }
        }
        result.push(TeamResult {
            team,
            captain,
            members: team_members,
        });
    }

    query_params.total = Some(total_items);

    UniResponse::ok_meta(result.into(), query_params.into()).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetTeamMembersResult {
    pub team: event_teams::Model,
    pub members: Vec<users::Model>,
}

/// GET /api/admin/events/{event_id}/teams/{team_id}/members
#[get("/{team_id}")]
pub async fn get_team_members(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
    query_params: Query<QueryParams>,
) -> UniResult<GetTeamMembersResult> {
    let mut query_params = query_params.0;
    let (event_id, team_id) = path.into_inner();

    let event_team = event_teams::Entity::find_by_id(team_id)
        .filter(event_teams::Column::EventId.eq(event_id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", team_id)))?;

    let stmt = event_team
        .find_related(event_team_members::Entity)
        .find_also_related(users::Entity);

    if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
        let paginator = stmt.paginate(ctx.db.get_ref(), limit);
        let items: Vec<(event_team_members::Model, Option<users::Model>)> =
            paginator.fetch_page(page.saturating_sub(1)).await?;
        query_params.total = Some(paginator.num_items().await? as usize);
        let items: Vec<users::Model> = items
            .into_iter()
            .filter_map(|(_team_member, user_opt)| user_opt)
            .collect();
        let result = GetTeamMembersResult {
            team: event_team,
            members: items,
        };
        UniResponse::ok_meta(result.into(), query_params.into()).into()
    } else {
        let items = stmt.all(ctx.db.get_ref()).await?;
        let items: Vec<users::Model> = items
            .into_iter()
            .filter_map(|(_team_member, user_opt)| user_opt)
            .collect();
        query_params.total = Some(items.len());
        let result = GetTeamMembersResult {
            team: event_team,
            members: items,
        };
        UniResponse::ok_meta(result.into(), query_params.into()).into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddUserToTeamRequest {
    pub user_id: Uuid,
}
/// POST /api/admin/events/{event_id}/teams/{team_id}/users
#[post("/{team_id}/users")]
pub async fn add_user_to_team(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
    utt: Json<AddUserToTeamRequest>,
) -> UniResult<EventTeamMembersDto> {
    let user = user.into_inner();
    let (event_id, team_id) = path.into_inner();
    let user_id = utt.into_inner().user_id;

    let event_team = event_teams::Entity::find_by_id(team_id)
        .filter(event_teams::Column::EventId.eq(event_id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", team_id)))?;

    let user_model = users::Entity::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", user_id)))?;

    let new_event_user = event_users::ActiveModel {
        event_id: Set(event_team.event_id),
        user_id: Set(user_model.id),
        ..Default::default()
    };

    let _event_user = new_event_user.insert(ctx.db.get_ref()).await?;

    let new_team_user = event_team_members::ActiveModel {
        event_id: Set(event_team.event_id),
        team_id: Set(event_team.id),
        user_id: Set(user_model.id),
        ..Default::default()
    };

    let team_user = new_team_user.insert(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_TEAMS",
            "ADD_MEMBER",
            format!(
                "{} 将用户 {} 加入队伍 {}",
                user.username, user_model.username, event_team.name
            )
            .as_str(),
            json!({"team_id": team_id, "user_id": user_id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(team_user.into())).into()
}

/// DELETE /api/admin/events/{event_id}/teams/{team_id}/users
#[delete("/{team_id}/users")]
pub async fn remove_user_from_team(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let (event_id, team_id) = path.into_inner();
    let dir = dir.into_inner();
    let deleted_count = event_team_members::Entity::delete_many()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::TeamId.eq(team_id))
        .filter(event_team_members::Column::UserId.is_in(dir.id_list))
        .exec(ctx.db.get_ref())
        .await?
        .rows_affected;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_TEAMS",
            "REMOVE_MEMBER",
            format!("{} 从队伍移除 {} 名成员", user.username, deleted_count).as_str(),
            json!({"team_id": team_id, "deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}

/// POST /api/admin/events/{event_id}/teams/{team_id}/banned
#[post("/{team_id}/banned")]
pub async fn ban_team(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let user = user.into_inner();
    let (event_id, team_id) = path.into_inner();

    let event_team = event_teams::Entity::find_by_id(team_id)
        .filter(event_teams::Column::EventId.eq(event_id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", team_id)))?;

    let mut event_team: event_teams::ActiveModel = event_team.into();
    event_team.banned = Set(true);

    let event_team = event_team.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_TEAMS",
            "BAN",
            format!("{} 封禁队伍: {}", user.username, event_team.name).as_str(),
            json!({"team_id": team_id, "team_name": event_team.name}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/teams/{team_id}/unbanned
#[post("/{team_id}/unbanned")]
pub async fn unbanned_team(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let user = user.into_inner();
    let (event_id, team_id) = path.into_inner();

    let event_team = event_teams::Entity::find_by_id(team_id)
        .filter(event_teams::Column::EventId.eq(event_id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", team_id)))?;

    let mut event_team: event_teams::ActiveModel = event_team.into();
    event_team.banned = Set(false);

    let event_team = event_team.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_TEAMS",
            "UNBAN",
            format!("{} 解禁队伍: {}", user.username, event_team.name).as_str(),
            json!({"team_id": team_id, "team_name": event_team.name}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
