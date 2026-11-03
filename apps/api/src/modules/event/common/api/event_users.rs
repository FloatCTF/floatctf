use crate::api::extractor::auth::RequirePermission;
use crate::core::permission::guards::{EventsParticipantsManageGuard, EventsReadGuard};
use std::str::FromStr;

use sea_orm::Condition;

use crate::{
    api::{
        FilterMapping, apply_filters,
        dto::DeleteItemsRequest,
        prelude::*,
        sea_orm_utils::{CrossFilterMapping, paginate_query, resolve_cross_filters},
    },
    // 响应体**必须**用账号 DTO，不能用生成实体 —— 见 `EventUserResult` 的说明。
    entity::{event_users, events, users},
    modules::identity::user::dto::UsersDto,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct AddUserRequest {
    pub user_id: Option<Uuid>,
    pub user_id_list: Option<Vec<Uuid>>,
}

/// POST /api/admin/events/{event_id}/users
#[post("")]
pub async fn add_user(
    user: RequirePermission<EventsParticipantsManageGuard>,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    aur: Json<AddUserRequest>,
) -> UniResult<()> {
    let user = user.into_auth();
    let aur = aur.into_inner();
    let event_id = event_id.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("未找到 {}", event_id)))?;
    if event.participant_mode == crate::entity::sea_orm_active_enums::ParticipantMode::Team {
        return AppError::BadRequest(
            "team event users must be added through the team membership endpoint".to_string(),
        )
        .into();
    }

    let mut added_count = 0;

    if aur.user_id_list.is_some() {
        let user_id_list = aur.user_id_list.unwrap();
        added_count = user_id_list.len();
        for user_id in user_id_list {
            let u = users::Entity::find_by_id(user_id)
                .one(ctx.db.get_ref())
                .await?
                .ok_or(AppError::NotFound(format!("未找到 {}", user_id)))?;

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
                    user.username(),
                    event.title,
                    added_count
                )
                .as_str(),
                json!({"event_id": event.id, "count": added_count}),
                user.id().into(),
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
            .ok_or(AppError::NotFound(format!("未找到 {}", user_id)))?;

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
                format!("{} 为比赛 {} 添加用户", user.username(), event.title).as_str(),
                json!({"event_id": event.id, "user_id": user_id}),
                user.id().into(),
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
    user: RequirePermission<EventsParticipantsManageGuard>,
    ctx: ReqCtx,
    path: Path<Uuid>,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_auth();
    let event_id = path.into_inner();
    let dir = dir.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("未找到 {}", event_id)))?;
    if event.participant_mode == crate::entity::sea_orm_active_enums::ParticipantMode::Team {
        return AppError::BadRequest(
            "team event users must be removed through the team membership endpoint".to_string(),
        )
        .into();
    }

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
            format!("{} 从比赛移除 {} 名用户", user.username(), deleted_count).as_str(),
            json!({"event_id": event_id, "deleted_count": deleted_count}),
            user.id().into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}

/// 赛事参与用户条目。
///
/// # 为什么 `user` 是 `UsersDto` 而不是 `users::Model`
///
/// `users::Model` 是 **sea-orm-codegen 生成的 DB 列镜像**，它派生 `Serialize`
/// 且 `password` 列**没有** `#[serde(skip_serializing)]`（全仓 entity 都没有，
/// 因为实体是生成物、不该手改）。因此任何把它放进响应结构体的地方，
/// 都会把 argon2id 密码哈希回传给客户端。
///
/// 这**曾经真实发生过**（`GET /api/admin/events/{id}/users` 仅需 `events.read`）：
/// 当时的字段就是 `pub user: users::Model`。这正是安全不变量 #1
/// （响应 DTO 永不含 password，见 `modules/identity/user/dto.rs` 的文件头）
/// 所禁止的形态，`identity` 模块当时改用独立 DTO 修好了，但**这一处漏了**。
///
/// 现在统一复用 `UsersDto` —— 全平台只有一个"账号出现在响应里"的表示，
/// 不再有第二个可能与 DB 列同构的入口。
///
/// 只派生 `Serialize`：这是**响应**类型，没有任何地方反序列化它；
/// 而且 `UsersDto` 本身也只派生 `Serialize`（响应 DTO 不该可反序列化，
/// 输入走 `UserWriteRequest`）。多派生一个 `Deserialize` 只会扩大它的使用面。
#[derive(Debug, Serialize)]
pub struct EventUserResult {
    /// 行标识 = 账号 id。`event_users` 是 (event_id, user_id) 复合主键，
    /// 没有单一 `id` 列，因此这里显式给出 `user.id` 作为稳定的行键。
    pub id: Uuid,
    pub user: UsersDto,
    pub event_user: event_users::Model,
}

/// GET /api/admin/events/{event_id}/users
#[get("")]
pub async fn get_users(
    _user: RequirePermission<EventsReadGuard>,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<EventUserResult>> {
    let event_id = event_id.into_inner();
    let mut query_params = query_params.0;

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("未找到 {}", event_id)))?;

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
            .ok_or(AppError::NotFound(format!("未找到 {}", eu.user_id)))?;
        // `id` 在转换前取出：`UsersDto::from` 会消费 `user`。
        let id = user.id;
        result.push(EventUserResult {
            id,
            // 显式转换到 DTO —— 这一步就是"不含 password"的保证本身。
            user: user.into(),
            event_user: eu,
        });
    }

    query_params.total = Some(total_items);

    UniResponse::ok_meta(result.into(), query_params.into()).into()
}

/// POST /api/admin/events/{event_id}/users/{user_id}/banned
#[post("/{user_id}/banned")]

pub async fn banned_user(
    user: RequirePermission<EventsParticipantsManageGuard>,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let user = user.into_auth();
    let (event_id, user_id) = path.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("未找到 {}", event_id)))?;

    let u = users::Entity::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("未找到 {}", user_id)))?;

    let event_user = event_users::Entity::find_by_id((event.id, u.id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "在 {} 中未找到 {}",
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
            format!("{} 封禁用户: {}", user.username(), u.username).as_str(),
            json!({"event_id": event_id, "user_id": user_id}),
            user.id().into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// POST /api/admin/events/{event_id}/users/{user_id}/unbanned
#[post("/{user_id}/unbanned")]
pub async fn unbanned_user(
    user: RequirePermission<EventsParticipantsManageGuard>,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let user = user.into_auth();
    let (event_id, user_id) = path.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("未找到 {}", event_id)))?;

    let u = users::Entity::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("未找到 {}", user_id)))?;

    let event_user = event_users::Entity::find_by_id((event.id, u.id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "在 {} 中未找到 {}",
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
            format!("{} 解禁用户: {}", user.username(), u.username).as_str(),
            json!({"event_id": event_id, "user_id": user_id}),
            user.id().into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
