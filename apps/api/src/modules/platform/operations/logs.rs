use crate::api::extractor::auth::RequirePermission;
use crate::core::permission::guards::LogsReadGuard;
use std::str::FromStr;

use sea_orm::Condition;

use crate::api::dto::map_dto_vec;

use super::logs_dto::LogsDto;
use crate::{
    api::{FilterMapping, prelude::*, sea_orm_utils::query_query},
    entity::logs,
};

/// GET /api/admin/logs
#[get("")]
pub async fn get_logs(
    _user: RequirePermission<LogsReadGuard>,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<LogsDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all().add(logs::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        // Wave 6：主体统一为单列。v1 的 `user_id` / `superadmin_id` 两个过滤器
        // 合并为一个 `actor_user_id`（它们查询的是同一个身份空间，
        // 分列只会让「按主体筛选」漏掉一半记录）。
        FilterMapping {
            key: "actor_user_id",
            column: Box::new(|v| {
                Condition::all()
                    .add(logs::Column::ActorUserId.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "ip_address",
            column: Box::new(|v| Condition::all().add(logs::Column::IpAddress.contains(v))),
        },
        FilterMapping {
            key: "category",
            column: Box::new(|v| Condition::all().add(logs::Column::Category.eq(v.to_string()))),
        },
        FilterMapping {
            key: "action",
            column: Box::new(|v| Condition::all().add(logs::Column::Action.contains(v))),
        },
        FilterMapping {
            key: "level",
            column: Box::new(|v| Condition::all().add(logs::Column::Level.eq(v.to_string()))),
        },
    ];
    let (items, total_items) = query_query::<logs::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| stmt.order_by_desc(logs::Column::CreatedAt))),
    )
    .await?;

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/admin/logs/{log_id}
#[get("/{log_id}")]
pub async fn get_log(
    _user: RequirePermission<LogsReadGuard>,
    ctx: ReqCtx,
    log_id: Path<Uuid>,
) -> UniResult<LogsDto> {
    let log_id = log_id.into_inner();
    let model = logs::Entity::find_by_id(log_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("{} 不存在", log_id)))?;

    UniResponse::ok(Some(model.into())).into()
}
