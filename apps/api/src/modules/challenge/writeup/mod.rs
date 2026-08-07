//! Challenge writeup handlers (player).

pub mod dto;
pub use dto::ChallengeWriteupDto;

use crate::{
    api::{FilterMapping, apply_filters, prelude::*, sea_orm_utils::paginate_query},
    entity::{challenge_writeup, challenges, users},
};
use sea_orm::Condition;
use std::str::FromStr;

/// GET /api/challenges/{challenge_id}/my_writeups
#[get("/{challenge_id}/my_writeup")]
pub async fn get_challenge_writeup(
    user: UserJwtGuard,
    ctx: ReqCtx,
    challenge_id: Path<Uuid>,
) -> UniResult<ChallengeWriteupDto> {
    let user = user.into_inner();
    let challenge_id = challenge_id.into_inner();

    let writeup = challenge_writeup::Entity::find()
        .filter(challenge_writeup::Column::ChallengeId.eq(challenge_id))
        .filter(challenge_writeup::Column::UserId.eq(user.id))
        .one(ctx.db.get_ref())
        .await?;

    match writeup {
        Some(writeup) => UniResponse::ok(Some(writeup.into())).into(),
        None => {
            AppError::NotFound(format!("Writeup for challenge {} not found", challenge_id)).into()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CreateChallengeWriteup {
    pub content: String,
}

/// POST /api/challenges/{challenge_id}/my_writeup
#[post("/{challenge_id}/my_writeup")]
pub async fn create_challenge_writeup(
    user: UserJwtGuard,
    ctx: ReqCtx,
    challenge_id: Path<Uuid>,
    ccw: Json<CreateChallengeWriteup>,
) -> UniResult<ChallengeWriteupDto> {
    let user = user.into_inner();
    let ccw = ccw.into_inner();
    let challenge_id = challenge_id.into_inner();

    // 查找是否存在
    let existing = challenge_writeup::Entity::find()
        .filter(challenge_writeup::Column::ChallengeId.eq(challenge_id))
        .filter(challenge_writeup::Column::UserId.eq(user.id))
        .one(ctx.db.get_ref())
        .await?;

    let wp = match existing {
        Some(wp) => {
            let mut active = wp.into_active_model();
            active.content = Set(ccw.content);
            active.created_at = Set(chrono::Utc::now().into());
            let wp = active.update(ctx.db.get_ref()).await?;
            ctx.log
                .add_log(
                    "INFO",
                    "WRITEUP",
                    "UPDATE",
                    format!("更新题目 {} 的 Writeup", challenge_id).as_str(),
                    json!({}),
                    user.id.into(),
                    None,
                    Some(&ctx.req),
                )
                .await;
            wp
        }
        None => {
            let active = challenge_writeup::ActiveModel {
                challenge_id: Set(challenge_id),
                user_id: Set(user.id),
                content: Set(ccw.content),
                ..Default::default()
            };
            let wp = active.insert(ctx.db.get_ref()).await?;
            ctx.log
                .add_log(
                    "INFO",
                    "WRITEUP",
                    "CREATE",
                    format!("为题目 {} 创建 Writeup", challenge_id).as_str(),
                    json!({}),
                    user.id.into(),
                    None,
                    Some(&ctx.req),
                )
                .await;
            wp
        }
    };

    Ok(UniResponse::ok(Some(wp.into())).into())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeWriteupResult {
    pub nickname: String,
    pub avatar: Option<String>,
    pub email: String,
    pub challenge: challenges::Model,
    pub writeup: challenge_writeup::Model,
}

/// GET /api/challenges/{challenge_id}/writeups
#[get("/{challenge_id}/writeups")]
pub async fn get_challenge_writeups(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    challenge_id: Path<Uuid>,
) -> UniResult<Vec<ChallengeWriteupResult>> {
    let challenge_id = challenge_id.into_inner();

    let writeups = challenge_writeup::Entity::find()
        .filter(challenge_writeup::Column::ChallengeId.eq(challenge_id))
        .find_also_related(challenges::Entity)
        .order_by_desc(challenge_writeup::Column::CreatedAt)
        .all(ctx.db.get_ref())
        .await?;

    let mut results = Vec::new();

    for (writeup, challenge) in writeups {
        let user = users::Entity::find_by_id(writeup.user_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(
                "User {} not found",
                writeup.user_id
            )))?;

        let result = ChallengeWriteupResult {
            nickname: user.nickname,
            avatar: user.avatar.clone(),
            email: user.email,
            challenge: challenge.ok_or(AppError::NotFound(format!(
                "Challenge {} not found",
                writeup.challenge_id
            )))?,
            writeup,
        };

        results.push(result);
    }

    UniResponse::ok(results.into()).into()
}

/// GET /api/writeups/{writeup_id}
#[get("/{writeup_id}")]
pub async fn get_writeup(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    writeup_id: Path<Uuid>,
) -> UniResult<ChallengeWriteupResult> {
    let writeup_id = writeup_id.into_inner();

    let writeup = challenge_writeup::Entity::find_by_id(writeup_id)
        .find_also_related(challenges::Entity)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "Writeup {} not found",
            writeup_id
        )))?;

    let (writeup, challenge) = writeup;

    let user = users::Entity::find_by_id(writeup.user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "User {} not found",
            writeup.user_id
        )))?;

    let result = ChallengeWriteupResult {
        nickname: user.nickname,
        avatar: user.avatar.clone(),
        email: user.email,
        challenge: challenge.ok_or(AppError::NotFound(format!(
            "Challenge {} not found",
            writeup.challenge_id
        )))?,
        writeup,
    };

    UniResponse::ok(result.into()).into()
}

/// GET /api/writeups
#[get("")]
pub async fn get_writeups(
    user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<ChallengeWriteupResult>> {
    let user = user.into_inner();
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all().add(
                    challenge_writeup::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
        FilterMapping {
            key: "challenge_id",
            column: Box::new(|v| {
                Condition::all().add(
                    challenge_writeup::Column::ChallengeId
                        .eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
    ];

    let stmt = challenge_writeup::Entity::find();
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(challenge_writeup::Column::CreatedAt);

    let (items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            paginate_query(stmt, ctx.db.get_ref(), limit, page).await?
        } else {
            let items = stmt.all(ctx.db.get_ref()).await?;
            (items.clone(), items.len())
        };

    let mut results = Vec::new();

    for writeup in items {
        let challenge = challenges::Entity::find_by_id(writeup.challenge_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(
                "Challenge {} not found",
                writeup.challenge_id
            )))?;
        let user = users::Entity::find_by_id(writeup.user_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(
                "User {} not found",
                writeup.user_id
            )))?;

        let result = ChallengeWriteupResult {
            nickname: user.nickname,
            avatar: user.avatar.clone(),
            email: user.email,
            challenge,
            writeup,
        };

        results.push(result);
    }

    query_params.total = Some(total_items);
    UniResponse::ok_meta(results.into(), query_params.into()).into()
}
