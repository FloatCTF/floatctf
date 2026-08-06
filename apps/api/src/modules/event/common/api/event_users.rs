use std::str::FromStr;

use sea_orm::Condition;

use crate::{
    api::{
        FilterMapping,
        dto::DeleteItemsRequest,
        apply_filters,
        prelude::*,
        sea_orm_utils::{CrossFilterMapping, paginate_query, resolve_cross_filters},
    },
    entity::{event_users, events, users},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct AddUserRequest {
    pub user_id: Option<Uuid>,
    pub user_id_list: Option<Vec<Uuid>>,
}

/// POST /api/admin/events/{event_id}/users
#[post("")]
pub async fn add_user(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    aur: Json<AddUserRequest>,
) -> UniResult<()> {
    let user = user.into_inner();
    let aur = aur.into_inner();
    let event_id = event_id.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    let mut added_count = 0;

    if aur.user_id_list.is_some() {
        let user_id_list = aur.user_id_list.unwrap();
        added_count = user_id_list.len();
        for user_id in user_id_list {
            let u = users::Entity::find_by_id(user_id)
                .one(ctx.db.get_ref())
                .await?
                .ok_or(AppError::NotFound(format!(" {} not exist", user_id)))?;

            let new_event_user = event_users::ActiveModel {
                event_id: Set(event.id),
                user_id: Set(u.id),
                ..Default::default()
            };

            new_event_user.insert(ctx.db.get_ref()).await?;
        }

        ctx.log
            .add_log(
                "INFO",
                "EVENT_USERS",
                "ADD",
                format!(
                    "{} 为比赛 {} 添加 {} 名用户",
                    user.username, event.title, added_count
                )
                .as_str(),
                json!({"event_id": event.id, "count": added_count}),
                None,
                user.id.into(),
                Some(&ctx.req),
            )
            .await;

        return UniResponse::ok_none().into();
    }
    if aur.user_id.is_some() {
        let user_id = aur.user_id.unwrap();
        let u = users::Entity::find_by_id(user_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(" {} not exist", user_id)))?;

        let new_event_user = event_users::ActiveModel {
            event_id: Set(event.id),
            user_id: Set(u.id),
            ..Default::default()
        };

        new_event_user.insert(ctx.db.get_ref()).await?;

        ctx.log
            .add_log(
                "INFO",
                "EVENT_USERS",
                "ADD",
                format!("{} 为比赛 {} 添加用户", user.username, event.title).as_str(),
                json!({"event_id": event.id, "user_id": user_id}),
                None,
                user.id.into(),
                Some(&ctx.req),
            )
            .await;

        return UniResponse::ok_none().into();
    }

    AppError::BadRequest("user_id or user_id_list is required".to_string()).into()
}

/// DELETE /api/admin/events/{event_id}/users
#[delete("")]
pub async fn remove_user(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<Uuid>,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let event_id = path.into_inner();
    let dir = dir.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    let deleted_count = event_users::Entity::delete_many()
        .filter(event_users::Column::EventId.eq(event.id))
        .filter(event_users::Column::UserId.is_in(dir.id_list))
        .exec(ctx.db.get_ref())
        .await?
        .rows_affected;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_USERS",
            "REMOVE",
            format!("{} 从比赛移除 {} 名用户", user.username, deleted_count).as_str(),
            json!({"event_id": event_id, "deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventUserResult {
    pub user: users::Model,
    pub event_user: event_users::Model,
}

/// GET /api/admin/events/{event_id}/users
#[get("")]
pub async fn get_users(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<EventUserResult>> {
    let event_id = event_id.into_inner();
    let mut query_params = query_params.0;

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    // 跨表过滤：username / nickname 属于 users 表
    let cross_ids = resolve_cross_filters::<users::Entity>(
        ctx.db.get_ref(),
        &query_params.filter,
        &[
            CrossFilterMapping {
                key: "username",
                column: Box::new(|v| Condition::all().add(users::Column::Username.contains(v))),
            },
            CrossFilterMapping {
                key: "nickname",
                column: Box::new(|v| Condition::all().add(users::Column::Nickname.contains(v))),
            },
        ],
        |m| m.id,
    )
    .await?;

    // event_users 本表字段过滤
    let mappings = [
        FilterMapping {
            key: "user_id",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_users::Column::UserId.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "points",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_users::Column::Points.eq(v.parse::<f64>().unwrap_or(0.0)))
            }),
        },
        FilterMapping {
            key: "banned",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_users::Column::Banned.eq(v.parse::<bool>().unwrap_or(false)))
            }),
        },
    ];

    let mut stmt = event_users::Entity::find().filter(event_users::Column::EventId.eq(event.id));

    if let Some(ids) = cross_ids {
        stmt = stmt.filter(event_users::Column::UserId.is_in(ids));
    }

    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(event_users::Column::JoinedAt);

    let (items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            paginate_query(stmt, ctx.db.get_ref(), limit, page).await?
        } else {
            let items = stmt.all(ctx.db.get_ref()).await?;
            (items.clone(), items.len())
        };

    let mut result = Vec::with_capacity(items.len());
    for eu in items {
        let user = users::Entity::find_by_id(eu.user_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(" {} not exist", eu.user_id)))?;
        result.push(EventUserResult {
            user,
            event_user: eu,
        });
    }

    query_params.total = Some(total_items);

    UniResponse::ok_meta(result.into(), query_params.into()).into()
}

/// POST /api/admin/events/{event_id}/users/{user_id}/banned
#[post("/{user_id}/banned")]

pub async fn banned_user(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let user = user.into_inner();
    let (event_id, user_id) = path.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    let u = users::Entity::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", user_id)))?;

    let event_user = event_users::Entity::find_by_id((event.id, u.id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            " {} not exist in {}",
            user_id, event_id
        )))?;

    let mut event_user: event_users::ActiveModel = event_user.into();
    event_user.banned = Set(true);
    event_user.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_USERS",
            "BAN",
            format!("{} 封禁用户: {}", user.username, u.username).as_str(),
            json!({"event_id": event_id, "user_id": user_id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/users/{user_id}/unbanned
#[post("/{user_id}/unbanned")]
pub async fn unbanned_user(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let user = user.into_inner();
    let (event_id, user_id) = path.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    let u = users::Entity::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", user_id)))?;

    let event_user = event_users::Entity::find_by_id((event.id, u.id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            " {} not exist in {}",
            user_id, event_id
        )))?;

    let mut event_user: event_users::ActiveModel = event_user.into();
    event_user.banned = Set(false);
    event_user.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_USERS",
            "UNBAN",
            format!("{} 解禁用户: {}", user.username, u.username).as_str(),
            json!({"event_id": event_id, "user_id": user_id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
