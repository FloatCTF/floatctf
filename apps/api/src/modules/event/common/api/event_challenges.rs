use std::str::FromStr;

use sea_orm::Condition;

use crate::api::dto::map_dto_vec;

use crate::modules::event::common::api::EventChallengesDto;
use crate::{
    api::{
        FilterMapping, apply_filters,
        dto::DeleteItemsRequest,
        prelude::*,
        sea_orm_utils::{CrossFilterMapping, paginate_query, resolve_cross_filters},
    },
    entity::{challenges, event_challenges, events},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct AddChallengeRequest {
    pub challenge_id: Option<Uuid>,
    pub challenge_id_list: Option<Vec<Uuid>>,
}

/// POST /api/admin/events/{event_id}/challenges
#[post("")]
pub async fn add_challenge(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    acr: Json<AddChallengeRequest>,
) -> UniResult<Vec<EventChallengesDto>> {
    let user = user.into_inner();
    let acr = acr.into_inner();
    let event_id = event_id.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("event {} not exist", event_id)))?;

    let mut event_challenges_list = Vec::new();

    // 把单个 id 和多个 id 合并成一个 Vec
    let challenge_ids: Vec<Uuid> = acr
        .challenge_id
        .into_iter()
        .chain(acr.challenge_id_list.unwrap_or_default())
        .collect();

    for challenge_id in challenge_ids {
        // 先检查 challenge 是否存在
        let challenge = challenges::Entity::find_by_id(challenge_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(
                "challenge {} not exist",
                challenge_id
            )))?;

        // 查询是否已存在 event_challenge
        if let Some(existing) = event_challenges::Entity::find()
            .filter(event_challenges::Column::EventId.eq(event.id))
            .filter(event_challenges::Column::ChallengeId.eq(challenge.id))
            .one(ctx.db.get_ref())
            .await?
        {
            // 已存在，直接放进结果
            event_challenges_list.push(existing);
        } else {
            // 不存在，执行插入
            let points = {
                match toml::from_str::<toml::Value>(&challenge.toml_str) {
                    Ok(value) => value
                        .get("points")
                        .and_then(|v| v.as_float())
                        .unwrap_or(0.0) as f64,
                    Err(_err) => {
                        println!("Error parsing TOML: {}", _err);
                        100 as f64
                    }
                }
            };
            let new_event_challenge = event_challenges::ActiveModel {
                event_id: Set(event.id),
                challenge_id: Set(challenge.id),
                points: Set(points),
                ..Default::default()
            };

            let inserted = new_event_challenge.insert(ctx.db.get_ref()).await?;
            event_challenges_list.push(inserted);
        }
    }

    ctx.log
        .add_log(
            "INFO",
            "EVENT_CHALLENGES",
            "ADD",
            format!(
                "{} 为比赛 {} 添加 {} 道题目",
                user.username,
                event.title,
                event_challenges_list.len()
            )
            .as_str(),
            json!({"event_id": event.id, "count": event_challenges_list.len()}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(map_dto_vec(event_challenges_list))).into()
}

/// DELETE /api/admin/events/{event_id}/challenges
#[delete("")]
pub async fn remove_challenge(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let event_id = event_id.into_inner();
    let dir = dir.into_inner();

    let deleted_count = event_challenges::Entity::delete_many()
        .filter(event_challenges::Column::EventId.eq(event_id))
        .filter(event_challenges::Column::ChallengeId.is_in(dir.id_list))
        .exec(ctx.db.get_ref())
        .await?
        .rows_affected;

    ctx.log
        .add_log(
            "INFO",
            "EVENT_CHALLENGES",
            "REMOVE",
            format!("{} 从比赛移除 {} 道题目", user.username, deleted_count).as_str(),
            json!({"event_id": event_id, "deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventChallengeResult {
    pub event_challenge: event_challenges::Model,
    pub challenge: challenges::Model,
}

/// GET /api/admin/events/{event_id}/challenges
///
/// 注意：不能直接用 query_query<E>，因为它返回 Vec<E::Model> 只支持单表。
/// 这里返回的是 EventChallengeResult（event_challenges + challenges 两表 join 的复合结构），
/// 所以只能在 event_challenges 上做过滤 + 分页，再逐条取关联的 challenge。
/// name/category 属于 challenges 表的列，通过 resolve_cross_filters 预查出匹配的
/// challenge ID 列表，再作为 is_in 约束加到主查询上。
#[get("")]
pub async fn get_challenges(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<EventChallengeResult>> {
    let event_id = event_id.into_inner();
    let mut query_params = query_params.0;

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    // 跨表过滤：name / category 属于 challenges 表，通过 resolve_cross_filters 预查 ID
    let cross_ids = resolve_cross_filters::<challenges::Entity>(
        ctx.db.get_ref(),
        &query_params.filter,
        &[
            CrossFilterMapping {
                key: "name",
                column: Box::new(|v| Condition::all().add(challenges::Column::Name.contains(v))),
            },
            CrossFilterMapping {
                key: "category",
                column: Box::new(|v| {
                    Condition::all().add(challenges::Column::Category.contains(v))
                }),
            },
        ],
        |m| m.id,
    )
    .await?;

    // 主表 event_challenges 字段通过 FilterMapping 过滤
    let mappings = [
        FilterMapping {
            key: "challenge_id",
            column: Box::new(|v| {
                Condition::all().add(
                    event_challenges::Column::ChallengeId
                        .eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
        FilterMapping {
            key: "hidden",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_challenges::Column::Hidden.eq(v.parse::<bool>().unwrap_or(false)))
            }),
        },
    ];

    let mut stmt = event.find_related(event_challenges::Entity);

    // 将跨表过滤结果作为 is_in 约束
    if let Some(ids) = cross_ids {
        stmt = stmt.filter(event_challenges::Column::ChallengeId.is_in(ids));
    }

    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);

    let (items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            paginate_query(stmt, ctx.db.get_ref(), limit, page).await?
        } else {
            let items = stmt.all(ctx.db.get_ref()).await?;
            (items.clone(), items.len())
        };

    let mut result = Vec::with_capacity(items.len());
    for ec in items {
        let challenge = challenges::Entity::find_by_id(ec.challenge_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(
                "challenge {} not exist",
                ec.challenge_id
            )))?;
        result.push(EventChallengeResult {
            event_challenge: ec,
            challenge,
        });
    }

    query_params.total = Some(total_items);

    UniResponse::ok_meta(result.into(), query_params.into()).into()
}

pub type HiddenChallengeRequest = AddChallengeRequest;

/// POST /api/admin/events/{event_id}/challenges/hidden
#[post("/hidden")]
pub async fn hidden_challenges(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    hcr: Json<HiddenChallengeRequest>,
) -> UniResult<Vec<EventChallengesDto>> {
    let user = user.into_inner();
    let hcr = hcr.into_inner();
    let event_id = event_id.into_inner();
    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    let mut event_challenges_list = Vec::new();

    if let Some(challenge_id) = hcr.challenge_id {
        let challenge = challenges::Entity::find_by_id(challenge_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

        let event_challenge = event_challenges::Entity::find()
            .filter(event_challenges::Column::EventId.eq(event.id))
            .filter(event_challenges::Column::ChallengeId.eq(challenge.id))
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

        let mut event_challenge: event_challenges::ActiveModel = event_challenge.into();
        event_challenge.hidden = Set(true);

        let event_challenge = event_challenge.update(ctx.db.get_ref()).await?;
        event_challenges_list.push(event_challenge);
    }

    if let Some(challenge_id_list) = hcr.challenge_id_list {
        for challenge_id in challenge_id_list {
            let challenge = challenges::Entity::find_by_id(challenge_id)
                .one(ctx.db.get_ref())
                .await?
                .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

            let event_challenge = event_challenges::Entity::find()
                .filter(event_challenges::Column::EventId.eq(event.id))
                .filter(event_challenges::Column::ChallengeId.eq(challenge.id))
                .one(ctx.db.get_ref())
                .await?
                .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

            let mut event_challenge: event_challenges::ActiveModel = event_challenge.into();
            event_challenge.hidden = Set(true);

            let event_challenge = event_challenge.update(ctx.db.get_ref()).await?;
            event_challenges_list.push(event_challenge);
        }
    }

    ctx.log
        .add_log(
            "INFO",
            "EVENT_CHALLENGES",
            "HIDE",
            format!(
                "{} 隐藏 {} 道题目",
                user.username,
                event_challenges_list.len()
            )
            .as_str(),
            json!({"event_id": event.id, "count": event_challenges_list.len()}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(map_dto_vec(event_challenges_list))).into()
}

pub type OpenChallengeRequest = AddChallengeRequest;
/// POST /api/admin/events/{event_id}/challenges/open
#[post("/open")]
pub async fn open_challenges(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    ocr: Json<OpenChallengeRequest>,
) -> UniResult<Vec<EventChallengesDto>> {
    let user = user.into_inner();
    let ocr = ocr.into_inner();
    let event_id = event_id.into_inner();

    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    let mut event_challenges_list = Vec::new();

    if let Some(challenge_id) = ocr.challenge_id {
        let challenge = challenges::Entity::find_by_id(challenge_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

        let event_challenge = event_challenges::Entity::find()
            .filter(event_challenges::Column::EventId.eq(event.id))
            .filter(event_challenges::Column::ChallengeId.eq(challenge.id))
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

        let mut event_challenge: event_challenges::ActiveModel = event_challenge.into();
        event_challenge.hidden = Set(false);

        let event_challenge = event_challenge.update(ctx.db.get_ref()).await?;
        event_challenges_list.push(event_challenge);
    }

    if let Some(challenge_id_list) = ocr.challenge_id_list {
        for challenge_id in challenge_id_list {
            let challenge = challenges::Entity::find_by_id(challenge_id)
                .one(ctx.db.get_ref())
                .await?
                .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

            let event_challenge = event_challenges::Entity::find()
                .filter(event_challenges::Column::EventId.eq(event.id))
                .filter(event_challenges::Column::ChallengeId.eq(challenge.id))
                .one(ctx.db.get_ref())
                .await?
                .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

            let mut event_challenge: event_challenges::ActiveModel = event_challenge.into();
            event_challenge.hidden = Set(false);

            let event_challenge = event_challenge.update(ctx.db.get_ref()).await?;
            event_challenges_list.push(event_challenge);
        }
    }

    ctx.log
        .add_log(
            "INFO",
            "EVENT_CHALLENGES",
            "OPEN",
            format!(
                "{} 开放 {} 道题目",
                user.username,
                event_challenges_list.len()
            )
            .as_str(),
            json!({"event_id": event.id, "count": event_challenges_list.len()}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(map_dto_vec(event_challenges_list))).into()
}
