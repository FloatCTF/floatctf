//! 点赞模块。

use crate::api::prelude::*;
use crate::entity::{discussion_likes, discussions};

/// POST /api/discussions/{discussion_id}/like
#[post("/{discussion_id}/like")]
pub async fn like_discussion(user: UserJwtGuard, ctx: ReqCtx, path: Path<Uuid>) -> UniResult<()> {
    let discussion_id = path.into_inner();
    let user = user.into_inner();

    // Check if discussion exists
    let discussion = discussions::Entity::find_by_id(discussion_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Discussion {} not exist",
            discussion_id
        )))?;

    // Check if already liked
    let existing = discussion_likes::Entity::find()
        .filter(discussion_likes::Column::DiscussionId.eq(discussion_id))
        .filter(discussion_likes::Column::UserId.eq(user.id))
        .one(ctx.db.get_ref())
        .await?;

    if existing.is_some() {
        return AppError::BadRequest("Already liked".to_string()).into();
    }

    // Create like
    let like = discussion_likes::ActiveModel {
        discussion_id: Set(discussion_id),
        user_id: Set(user.id),
        ..Default::default()
    };
    like.insert(ctx.db.get_ref()).await?;

    // Update like count
    let discussion_title = discussion.title.clone();
    let new_like_count = discussion.like_count + 1;
    let mut m = discussion.into_active_model();
    m.like_count = Set(new_like_count);
    m.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "DISCUSSION",
            "LIKE",
            format!("点赞讨论: {}", discussion_title).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

/// DELETE /api/discussions/{discussion_id}/like
#[delete("/{discussion_id}/like")]
pub async fn unlike_discussion(user: UserJwtGuard, ctx: ReqCtx, path: Path<Uuid>) -> UniResult<()> {
    let discussion_id = path.into_inner();
    let user = user.into_inner();

    let discussion = discussions::Entity::find_by_id(discussion_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Discussion {} not exist",
            discussion_id
        )))?;

    // Find and delete like
    let like = discussion_likes::Entity::find()
        .filter(discussion_likes::Column::DiscussionId.eq(discussion_id))
        .filter(discussion_likes::Column::UserId.eq(user.id))
        .one(ctx.db.get_ref())
        .await?;

    let discussion_title = discussion.title.clone();
    if let Some(like) = like {
        like.delete(ctx.db.get_ref()).await?;

        // Update like count
        let new_like_count = (discussion.like_count - 1).max(0);
        let mut m = discussion.into_active_model();
        m.like_count = Set(new_like_count);
        m.update(ctx.db.get_ref()).await?;

        ctx.log
            .add_log(
                "INFO",
                "DISCUSSION",
                "UNLIKE",
                format!("取消点赞讨论: {}", discussion_title).as_str(),
                json!({}),
                user.id.into(),
                None,
                Some(&ctx.req),
            )
            .await;
    }

    UniResponse::ok_none().into()
}
