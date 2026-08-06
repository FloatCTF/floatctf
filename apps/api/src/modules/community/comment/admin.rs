//! Admin comment handlers — `/api/admin/discussions/{id}/comments`.

use crate::api::dto::map_dto_vec;

use crate::modules::community::comment::DiscussionCommentsDto;
use crate::{
    api::{prelude::*, sea_orm_utils::paginate_query},
    entity::discussion_comments,
};

/// GET /api/admin/discussions/{discussion_id}/comments
#[get("/{discussion_id}/comments")]
pub async fn get_discussion_comments(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<DiscussionCommentsDto>> {
    let discussion_id = path.into_inner();
    let mut query_params = query_params.0;

    let stmt = discussion_comments::Entity::find()
        .filter(discussion_comments::Column::DiscussionId.eq(discussion_id));
    let stmt = stmt.order_by_desc(discussion_comments::Column::CreatedAt);

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

/// DELETE /api/admin/discussions/{discussion_id}/comments/{comment_id}
#[delete("/{discussion_id}/comments/{comment_id}")]
pub async fn delete_comment(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let user = user.into_inner();
    let (discussion_id, comment_id) = path.into_inner();

    let comment = discussion_comments::Entity::find_by_id(comment_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Comment {} not exist",
            comment_id
        )))?;

    if comment.discussion_id != discussion_id {
        return AppError::BadRequest("Comment does not belong to this discussion".to_string())
            .into();
    }

    comment.delete(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "DISCUSSIONS",
            "DELETE_COMMENT",
            format!("{} 删除评论", user.username).as_str(),
            json!({"comment_id": comment_id, "discussion_id": discussion_id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
