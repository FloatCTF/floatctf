//! Admin discussion handlers — `/api/admin/discussions`.

use std::str::FromStr;

use sea_orm::Condition;

use crate::api::dto::map_dto_vec;

use crate::modules::community::discussion::DiscussionsDto;
use crate::{
    api::{
        FilterMapping, dto::DeleteItemsRequest, apply_filters, prelude::*,
        sea_orm_utils::paginate_query,
    },
    entity::discussions,
};

/// GET /api/admin/discussions
#[get("")]
pub async fn get_discussions(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<DiscussionsDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(discussions::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "title",
            column: Box::new(|v| Condition::all().add(discussions::Column::Title.contains(v))),
        },
        FilterMapping {
            key: "author_id",
            column: Box::new(|v| {
                Condition::all().add(
                    discussions::Column::AuthorId.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
    ];

    let stmt = discussions::Entity::find();
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(discussions::Column::UpdatedAt);

    let (items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            paginate_query(stmt, ctx.db.get_ref(), limit, page).await?
        } else {
            let items = stmt.all(ctx.db.get_ref()).await?;
            (items.clone(), items.len())
        };

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/admin/discussions/{discussion_id}
#[get("/{discussion_id}")]
pub async fn get_discussion(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<Uuid>,
) -> UniResult<DiscussionsDto> {
    let discussion_id = path.into_inner();

    let discussion = discussions::Entity::find_by_id(discussion_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Discussion {} not exist",
            discussion_id
        )))?;

    UniResponse::ok(Some(discussion.into())).into()
}

/// DELETE /api/admin/discussions
#[delete("")]
pub async fn delete_discussions(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();
    let mut deleted_count = 0;
    for discussion_id in dir.id_list {
        let discussion = discussions::Entity::find_by_id(discussion_id)
            .one(ctx.db.get_ref())
            .await?;
        if let Some(discussion) = discussion {
            let r = discussion.delete(ctx.db.get_ref()).await?;
            deleted_count += r.rows_affected;
        }
    }

    ctx.log
        .add_log(
            "INFO",
            "DISCUSSIONS",
            "DELETE",
            format!("{} 删除 {} 条讨论", user.username, deleted_count).as_str(),
            json!({"deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}
