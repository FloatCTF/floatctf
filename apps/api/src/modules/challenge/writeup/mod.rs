//! Writeup 相关能力。

pub mod dto;
pub use dto::{ChallengeWriteupDto, UnifiedWriteupDetail, UnifiedWriteupResult};

use crate::{
    api::{FilterMapping, apply_filters, prelude::*, sea_orm_utils::paginate_query},
    entity::{awdp_run_writeups, awdp_runs, challenge_writeup, challenges, gameboxes, users},
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

/// GET /api/writeups/{writeup_id} —— challenge 或 gamebox 的 Writeup 统一详情。
/// 优先按 challenge_writeup.id 解析；找不到再按 awdp_run_writeups.run_id 解析
/// （practice run writeup 属主可见，非本人 403）。
#[get("/{writeup_id}")]
pub async fn get_writeup(
    user: UserJwtGuard,
    ctx: ReqCtx,
    writeup_id: Path<Uuid>,
) -> UniResult<UnifiedWriteupDetail> {
    let me = user.into_inner();
    let db = ctx.db.get_ref();
    let writeup_id = writeup_id.into_inner();

    // 1. challenge writeup（公开）。
    if let Some((writeup, challenge)) = challenge_writeup::Entity::find_by_id(writeup_id)
        .find_also_related(challenges::Entity)
        .one(db)
        .await?
    {
        let challenge = challenge.ok_or(AppError::NotFound(format!(
            "Challenge of writeup {} not found",
            writeup_id
        )))?;
        let user = users::Entity::find_by_id(writeup.user_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound(format!(
                "User {} not found",
                writeup.user_id
            )))?;
        return Ok(UniResponse::ok(Some(UnifiedWriteupDetail {
            writeup_type: "challenge".into(),
            id: writeup.id,
            content_id: challenge.id,
            content_name: challenge.name,
            category: Some(challenge.category),
            nickname: user.nickname,
            avatar: user.avatar.clone(),
            email: user.email,
            content: writeup.content,
            created_at: writeup.created_at,
            updated_at: writeup.updated_at,
        }))
        .into());
    }

    // 2. gamebox（practice run）writeup：owner-only，与 run 系接口一致。
    if let Some((wp, run)) = awdp_run_writeups::Entity::find_by_id(writeup_id)
        .find_also_related(awdp_runs::Entity)
        .one(db)
        .await?
    {
        if wp.user_id != me.id {
            return Err(AppError::Forbidden("该训练 writeup 不属于你".into()).into());
        }
        let run = run.ok_or(AppError::NotFound(format!(
            "Run of writeup {} not found",
            writeup_id
        )))?;
        let gb_id = run
            .gamebox_id
            .ok_or(AppError::NotFound("该 run 无 gamebox".into()))?;
        let gb = gameboxes::Entity::find_by_id(gb_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound(format!("GameBox {} not found", gb_id)))?;
        return Ok(UniResponse::ok(Some(UnifiedWriteupDetail {
            writeup_type: "gamebox".into(),
            id: wp.run_id,
            content_id: gb.id,
            content_name: gb.name,
            category: None,
            nickname: me.nickname,
            avatar: me.avatar.clone(),
            email: me.email,
            content: wp.content,
            created_at: wp.created_at,
            updated_at: wp.updated_at,
        }))
        .into());
    }

    Err(AppError::NotFound(format!("Writeup {} not found", writeup_id)).into())
}

/// GET /api/writeups
#[get("")]
pub async fn get_writeups(
    user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<UnifiedWriteupResult>> {
    let me = user.into_inner();
    let mut query_params = query_params.0;
    let db = ctx.db.get_ref();

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

    // 1. challenge writeups（公开）。
    let stmt = challenge_writeup::Entity::find();
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(challenge_writeup::Column::CreatedAt);
    let items = stmt.all(db).await?;

    let mut results: Vec<UnifiedWriteupResult> = Vec::new();
    for writeup in items {
        let challenge = challenges::Entity::find_by_id(writeup.challenge_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound(format!(
                "Challenge {} not found",
                writeup.challenge_id
            )))?;
        let user = users::Entity::find_by_id(writeup.user_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound(format!(
                "User {} not found",
                writeup.user_id
            )))?;
        results.push(UnifiedWriteupResult {
            id: writeup.id,
            writeup_type: "challenge".into(),
            nickname: user.nickname,
            avatar: user.avatar.clone(),
            email: user.email,
            content_id: challenge.id,
            content_name: challenge.name,
            updated_at: writeup.updated_at,
        });
    }

    // 2. 我自己的 gamebox（practice run）writeups：练习 writeup 仅属主可见（
    //    run 系接口 owner-only），全局列表也只对本人出现（challenge writeup 是
    //    公开的，练习不是）。竞赛 run 无 gamebox → 跳过。
    let my_wps = awdp_run_writeups::Entity::find()
        .filter(awdp_run_writeups::Column::UserId.eq(me.id))
        .find_also_related(awdp_runs::Entity)
        .all(db)
        .await?;
    for (wp, run) in my_wps {
        let Some(run) = run else {
            continue;
        };
        let Some(gb_id) = run.gamebox_id else {
            continue;
        };
        let Some(gb) = gameboxes::Entity::find_by_id(gb_id).one(db).await? else {
            continue;
        };
        results.push(UnifiedWriteupResult {
            id: wp.run_id,
            writeup_type: "gamebox".into(),
            nickname: me.nickname.clone(),
            avatar: me.avatar.clone(),
            email: me.email.clone(),
            content_id: gb.id,
            content_name: gb.name,
            updated_at: wp.updated_at,
        });
    }

    // 3. 合并排序 + 内存分页（两类行合并后无法用 SQL 分页）。
    results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let total: usize = results.len();
    let total_u64 = total as u64;
    let page = query_params.page.unwrap_or(1).max(1);
    let limit = query_params.limit.unwrap_or(total_u64.max(1)).max(1);
    let start = ((page - 1) as usize) * (limit as usize);
    let page_items = results
        .into_iter()
        .skip(start)
        .take(limit as usize)
        .collect::<Vec<_>>();

    query_params.total = Some(total);
    UniResponse::ok_meta(page_items.into(), query_params.into()).into()
}
