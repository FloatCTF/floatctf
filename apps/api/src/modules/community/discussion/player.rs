//! Player discussion handlers — `/api/discussions`.

use std::{collections::HashMap, str::FromStr};

use sea_orm::Condition;

use super::{CreateDiscussionRequest, DiscussionWithAuthor, PatchDiscussionRequest};
use crate::modules::community::discussion::DiscussionsDto;
use crate::{
    api::{FilterMapping, apply_filters, prelude::*, sea_orm_utils::paginate_query},
    entity::{discussion_likes, discussions, users},
};

/// GET /api/discussions
#[get("")]
pub async fn get_discussions(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<DiscussionWithAuthor>> {
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

    // Fetch author info for all discussions (if any)
    let authors: Vec<users::Model> = if !items.is_empty() {
        let author_ids: Vec<Uuid> = items.iter().map(|d| d.author_id).collect();
        users::Entity::find()
            .filter(users::Column::Id.is_in(author_ids))
            .all(ctx.db.get_ref())
            .await?
    } else {
        Vec::new()
    };
    let author_map: HashMap<Uuid, &users::Model> = authors.iter().map(|u| (u.id, u)).collect();

    let result: Vec<DiscussionWithAuthor> = items
        .into_iter()
        .map(|d| {
            let author = author_map.get(&d.author_id);
            DiscussionWithAuthor {
                author_nickname: author
                    .map_or_else(|| d.author_id.to_string(), |u| u.nickname.clone()),
                author_avatar: author.and_then(|u| u.avatar.clone()),
                is_liked: false,
                discussion: d,
            }
        })
        .collect();

    query_params.total = Some(total_items);

    UniResponse::ok_meta(result.into(), query_params.into()).into()
}

/// GET /api/discussions/{discussion_id}
#[get("/{discussion_id}")]
pub async fn get_discussion(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: Path<Uuid>,
) -> UniResult<DiscussionWithAuthor> {
    let discussion_id = path.into_inner();
    let user = user.into_inner();

    let discussion = discussions::Entity::find_by_id(discussion_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Discussion {} not exist",
            discussion_id
        )))?;

    // Fetch author info
    let author = users::Entity::find_by_id(discussion.author_id)
        .one(ctx.db.get_ref())
        .await?;

    let author_nickname = author
        .as_ref()
        .map_or_else(|| discussion.author_id.to_string(), |u| u.nickname.clone());
    let author_avatar = author.and_then(|u| u.avatar.clone());

    // Check if current user has liked this discussion
    let existing_like = discussion_likes::Entity::find()
        .filter(discussion_likes::Column::DiscussionId.eq(discussion_id))
        .filter(discussion_likes::Column::UserId.eq(user.id))
        .one(ctx.db.get_ref())
        .await?;
    let is_liked = existing_like.is_some();

    // Increment view count — only if viewer is not the author
    let is_author = discussion.author_id == user.id;
    let current_views = discussion.view_count;
    let mut m = discussion.into_active_model();
    if !is_author {
        m.view_count = Set(current_views + 1);
    }
    let updated = m.update(ctx.db.get_ref()).await?;

    let result = DiscussionWithAuthor {
        author_nickname,
        author_avatar,
        is_liked,
        discussion: updated,
    };

    UniResponse::ok(result.into()).into()
}

/// POST /api/discussions
#[post("")]
pub async fn create_discussion(
    user: UserJwtGuard,
    ctx: ReqCtx,
    cdr: Json<CreateDiscussionRequest>,
) -> UniResult<DiscussionsDto> {
    let cdr = cdr.into_inner();
    let user = user.into_inner();

    let discussion_title = cdr.title.clone();
    let discussion = discussions::ActiveModel {
        title: Set(cdr.title),
        content: Set(cdr.content),
        author_id: Set(user.id),
        view_count: Set(0),
        like_count: Set(0),
        comment_count: Set(0),
        ..Default::default()
    };
    let discussion = discussion.insert(ctx.db.get_ref()).await?;
    ctx.log
        .add_log(
            "INFO",
            "DISCUSSION",
            "CREATE",
            format!("创建讨论: {}", discussion_title).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;
    UniResponse::ok(Some(discussion.into())).into()
}

/// PATCH /api/discussions/{discussion_id}
#[patch("/{discussion_id}")]
pub async fn patch_discussion(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: Path<Uuid>,
    pdr: Json<PatchDiscussionRequest>,
) -> UniResult<DiscussionsDto> {
    let discussion_id = path.into_inner();
    let pdr = pdr.into_inner();
    let user = user.into_inner();

    let discussion = discussions::Entity::find_by_id(discussion_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Discussion {} not exist",
            discussion_id
        )))?;

    // Check if user is the author
    if discussion.author_id != user.id {
        return AppError::Forbidden("Not enough permission".into()).into();
    }

    let mut m = discussion.into_active_model();
    if let Some(title) = pdr.title {
        m.title = Set(title);
    }
    if let Some(content) = pdr.content {
        m.content = Set(content);
    }
    let discussion = m.update(ctx.db.get_ref()).await?;
    ctx.log
        .add_log(
            "INFO",
            "DISCUSSION",
            "UPDATE",
            format!("编辑讨论: {}", discussion.title).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;
    UniResponse::ok(Some(discussion.into())).into()
}

/// DELETE /api/discussions/{discussion_id}
#[delete("/{discussion_id}")]
pub async fn delete_discussion(user: UserJwtGuard, ctx: ReqCtx, path: Path<Uuid>) -> UniResult<()> {
    let discussion_id = path.into_inner();
    let user = user.into_inner();

    let discussion = discussions::Entity::find_by_id(discussion_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Discussion {} not exist",
            discussion_id
        )))?;

    // Check if user is the author
    if discussion.author_id != user.id {
        return AppError::Forbidden("Not enough permission".into()).into();
    }

    let discussion_title = discussion.title.clone();
    discussion.delete(ctx.db.get_ref()).await?;
    ctx.log
        .add_log(
            "INFO",
            "DISCUSSION",
            "DELETE",
            format!("删除讨论: {}", discussion_title).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;
    UniResponse::ok_none().into()
}
