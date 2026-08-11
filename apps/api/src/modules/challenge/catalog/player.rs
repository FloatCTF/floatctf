//! Player challenge catalog handlers.

use std::str::FromStr;

use sea_orm::Condition;

use crate::modules::challenge::catalog::ChallengesDto;
use crate::modules::event::jeopardy::api::InstancesDto;
use crate::{
    api::{FilterMapping, apply_filters, prelude::*, sea_orm_utils::paginate_query},
    entity::{challenge_instances, challenges, sea_orm_active_enums::InstanceStatus},
};

/// GET /api/challenges
#[get("")]
pub async fn get_challenges(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<ChallengesDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(challenges::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "name",
            column: Box::new(|v| Condition::all().add(challenges::Column::Name.contains(v))),
        },
        FilterMapping {
            key: "category",
            column: Box::new(|v| Condition::all().add(challenges::Column::Category.contains(v))),
        },
        FilterMapping {
            key: "description",
            column: Box::new(|v| Condition::all().add(challenges::Column::Description.contains(v))),
        },
    ];

    let stmt = challenges::Entity::find().filter(challenges::Column::Hidden.eq(false));
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(challenges::Column::UpdatedAt);

    let (items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            paginate_query(stmt, ctx.db.get_ref(), limit, page).await?
        } else {
            let items = stmt.all(ctx.db.get_ref()).await?;
            (items.clone(), items.len())
        };

    query_params.total = Some(total_items);

    // 玩家侧返回 enriched DTO（当前 package 摘要 + 附件元数据），
    // 附件链接由前端按 /static/challenges/... 构造。
    let dtos: Vec<ChallengesDto> = items.into_iter().map(Into::into).collect();
    UniResponse::ok_meta(Some(dtos), query_params.into()).into()
}

/// GET /api/challenges/{challenge_id}
#[get("/{challenge_id}")]
pub async fn get_challenge(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    challenge_id: Path<Uuid>,
) -> UniResult<ChallengesDto> {
    let challenge_id = challenge_id.into_inner();
    match challenges::Entity::find_by_id(challenge_id)
        .filter(challenges::Column::Hidden.eq(false))
        .one(ctx.db.get_ref())
        .await?
    {
        Some(model) => {
            let dto = ChallengesDto::from(&model);
            UniResponse::ok(Some(dto)).into()
        }
        None => AppError::NotFound(format!(" {} not exist", challenge_id)).into(),
    }
}

/// GET /api/challenges/{challenge_id}/instance
#[get("/{challenge_id}/instance")]
pub async fn get_challenge_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    challenge_id: Path<Uuid>,
) -> UniResult<InstancesDto> {
    let user = user.into_inner();
    let challenge_id = challenge_id.into_inner();

    let practice =
        crate::modules::event::common::domain::practice_event::require_practice_jeopardy_event(
            ctx.db.get_ref(),
        )
        .await
        .map_err(|e| crate::api::AppError::Internal(e.to_string()))?;
    let instance = challenge_instances::Entity::find()
        .filter(challenge_instances::Column::ChallengeId.eq(challenge_id))
        .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
        .filter(challenge_instances::Column::UserId.eq(user.id))
        .filter(challenge_instances::Column::EventId.eq(practice.id))
        .one(ctx.db.get_ref())
        .await?;

    UniResponse::ok(instance.map(Into::into)).into()
}
