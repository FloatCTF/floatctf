//! Player challenge set handlers.

use std::str::FromStr;

use sea_orm::Condition;

use crate::modules::challenge::catalog::ChallengesDto;
use crate::modules::challenge::set::ChallengeSetsDto;
use crate::{
    api::{FilterMapping, map_dto_vec, prelude::*, sea_orm_utils::query_query},
    entity::{challenge_set_items, challenge_sets, challenges},
};

/// GET /api/challenge_sets
#[get("")]
pub async fn get_challenge_sets(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<ChallengeSetsDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(challenge_sets::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "name",
            column: Box::new(|v| Condition::all().add(challenge_sets::Column::Name.contains(v))),
        },
        FilterMapping {
            key: "description",
            column: Box::new(|v| {
                Condition::all().add(challenge_sets::Column::Description.contains(v))
            }),
        },
    ];

    let (items, total_items) = query_query::<challenge_sets::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| {
            stmt.order_by_desc(challenge_sets::Column::CreatedAt)
        })),
    )
    .await?;

    query_params.total = Some(total_items);
    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/challenge_sets/{challenge_set_id}
#[get("/{challenge_set_id}")]
pub async fn get_challenge_set(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    challenge_set_id: Path<Uuid>,
) -> UniResult<Vec<ChallengesDto>> {
    let challenge_set_id = challenge_set_id.into_inner();
    let challenges = challenges::Entity::find()
        // 只查在该 set 里的 challenge
        .join_rev(
            sea_orm::JoinType::InnerJoin,
            challenge_set_items::Entity::belongs_to(challenges::Entity)
                .from(challenge_set_items::Column::ChallengeId)
                .to(challenges::Column::Id)
                .into(),
        )
        .filter(challenge_set_items::Column::SetId.eq(challenge_set_id))
        // 只查未隐藏的
        .filter(challenges::Column::Hidden.eq(false))
        .all(ctx.db.get_ref())
        .await?;

    UniResponse::ok(Some(
        ChallengesDto::from_models(ctx.db.get_ref(), &challenges).await?,
    ))
    .into()
}
