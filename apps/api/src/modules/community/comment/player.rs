//! 评论选手端接口。

use std::collections::HashMap;

use super::{CommentWithAuthor, CreateCommentRequest, PatchCommentRequest};
use crate::modules::community::comment::DiscussionCommentsDto;
use crate::{
    api::{prelude::*, sea_orm_utils::paginate_query},
    entity::{discussion_comments, discussions, users},
};

/// GET /api/discussions/{discussion_id}/comments
#[get("/{discussion_id}/comments")]
pub async fn get_discussion_comments(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    path: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<CommentWithAuthor>> {
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

    // Fetch author info
    let authors: Vec<users::Model> = if !items.is_empty() {
        let author_ids: Vec<Uuid> = items.iter().map(|c| c.author_id).collect();
        users::Entity::find()
            .filter(users::Column::Id.is_in(author_ids))
            .all(ctx.db.get_ref())
            .await?
    } else {
        Vec::new()
    };
    let author_map: HashMap<Uuid, &users::Model> = authors.iter().map(|u| (u.id, u)).collect();

    let results: Vec<CommentWithAuthor> = items
        .into_iter()
        .map(|c| {
            let author = author_map.get(&c.author_id);
            CommentWithAuthor {
                author_nickname: author
                    .map_or_else(|| c.author_id.to_string(), |u| u.nickname.clone()),
                author_avatar: author.and_then(|u| u.avatar.clone()),
                comment: c,
            }
        })
        .collect();

    query_params.total = Some(total_items);

    UniResponse::ok_meta(results.into(), query_params.into()).into()
}

/// POST /api/discussions/{discussion_id}/comments
#[post("/{discussion_id}/comments")]
pub async fn create_comment(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: Path<Uuid>,
    ccr: Json<CreateCommentRequest>,
) -> UniResult<DiscussionCommentsDto> {
    let discussion_id = path.into_inner();
    let ccr = ccr.into_inner();
    let user = user.into_inner();

    // Check if discussion exists
    let discussion = discussions::Entity::find_by_id(discussion_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Discussion {} not exist",
            discussion_id
        )))?;

    // If parent_id is provided, check it exists and belongs to same discussion
    if let Some(parent_id) = ccr.parent_id {
        let parent = discussion_comments::Entity::find_by_id(parent_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(
                "Parent comment {} not exist",
                parent_id
            )))?;
        if parent.discussion_id != discussion_id {
            return AppError::BadRequest(
                "Parent comment does not belong to this discussion".to_string(),
            )
            .into();
        }
    }

    let comment = discussion_comments::ActiveModel {
        discussion_id: Set(discussion_id),
        author_id: Set(user.id),
        content: Set(ccr.content),
        parent_id: Set(ccr.parent_id),
        ..Default::default()
    };
    let comment = comment.insert(ctx.db.get_ref()).await?;

    // Update comment count
    let discussion_title = discussion.title.clone();
    let new_comment_count = discussion.comment_count + 1;
    let mut m = discussion.into_active_model();
    m.comment_count = Set(new_comment_count);
    m.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "COMMENT",
            "CREATE",
            format!("评论讨论: {}", discussion_title).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(comment.into())).into()
}

/// PATCH /api/discussions/{discussion_id}/comments/{comment_id}
#[patch("/{discussion_id}/comments/{comment_id}")]
pub async fn patch_comment(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
    pcr: Json<PatchCommentRequest>,
) -> UniResult<DiscussionCommentsDto> {
    let (discussion_id, comment_id) = path.into_inner();
    let pcr = pcr.into_inner();
    let user = user.into_inner();

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

    if comment.author_id != user.id {
        return AppError::Forbidden("Not enough permission".into()).into();
    }

    let mut m = comment.into_active_model();
    if let Some(content) = pcr.content {
        m.content = Set(content);
    }
    let comment = m.update(ctx.db.get_ref()).await?;
    ctx.log
        .add_log(
            "INFO",
            "COMMENT",
            "UPDATE",
            format!("编辑评论 {}", comment_id).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;
    UniResponse::ok(Some(comment.into())).into()
}

/// DELETE /api/discussions/{discussion_id}/comments/{comment_id}
#[delete("/{discussion_id}/comments/{comment_id}")]
pub async fn delete_comment(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (discussion_id, comment_id) = path.into_inner();
    let user = user.into_inner();

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

    if comment.author_id != user.id {
        return AppError::Forbidden("Not enough permission".into()).into();
    }

    comment.delete(ctx.db.get_ref()).await?;

    // Update comment count
    let discussion = discussions::Entity::find_by_id(discussion_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Discussion {} not exist",
            discussion_id
        )))?;

    let discussion_title = discussion.title.clone();
    let new_comment_count = (discussion.comment_count - 1).max(0);
    let mut m = discussion.into_active_model();
    m.comment_count = Set(new_comment_count);
    m.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "COMMENT",
            "DELETE",
            format!("删除评论 {} (讨论: {})", comment_id, discussion_title).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
