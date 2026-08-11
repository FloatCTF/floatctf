//! Player practice challenge solves (catalog, not event scoring).

use std::{collections::HashMap, str::FromStr};

use sea_orm::{Condition, JoinType, QuerySelect, RelationTrait};

use crate::{
    api::{FilterMapping, apply_filters, prelude::*, sea_orm_utils::paginate_query},
    entity::{
        events, jeopardy_challenge_solves,
        sea_orm_active_enums::{EventFamily, EventPurpose},
        users,
    },
    modules::event::common::domain::event_mode::PRACTICE_JEOPARDY_SYSTEM_KEY,
};

use chrono::{DateTime, FixedOffset};

#[derive(Debug, Serialize, Deserialize)]
pub struct SolveResult {
    #[serde(flatten)]
    pub solve: jeopardy_challenge_solves::Model,
    pub nickname: String,
    pub avatar: Option<String>,
}

/// GET /api/challenge_solves (scope `/solves`)
#[get("")]
pub async fn get_solves(
    user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<SolveResult>> {
    let user = user.into_inner();
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all().add(
                    jeopardy_challenge_solves::Column::Id
                        .eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
        FilterMapping {
            key: "challenge_id",
            column: Box::new(|v| {
                Condition::all().add(
                    jeopardy_challenge_solves::Column::ChallengeId
                        .eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
        FilterMapping {
            key: "event_id",
            column: Box::new(|v| {
                Condition::all().add(
                    jeopardy_challenge_solves::Column::EventId
                        .eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
    ];

    // Default: practice solves only (join events purpose=practice)
    let stmt = jeopardy_challenge_solves::Entity::find()
        .join(
            JoinType::InnerJoin,
            jeopardy_challenge_solves::Relation::Events.def(),
        )
        .filter(events::Column::Purpose.eq(EventPurpose::Practice))
        .filter(events::Column::Family.eq(EventFamily::Jeopardy))
        .filter(jeopardy_challenge_solves::Column::UserId.eq(user.id));
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(jeopardy_challenge_solves::Column::CreatedAt);

    let (items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            paginate_query(stmt, ctx.db.get_ref(), limit, page).await?
        } else {
            let items = stmt.all(ctx.db.get_ref()).await?;
            (items.clone(), items.len())
        };

    query_params.total = Some(total_items);

    let nickname = user.nickname.clone();
    let avatar = user.avatar.clone();

    let results: Vec<SolveResult> = items
        .into_iter()
        .map(|s| SolveResult {
            nickname: nickname.clone(),
            avatar: avatar.clone(),
            solve: s,
        })
        .collect();

    UniResponse::ok_meta(results.into(), query_params.into()).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopUser {
    no: usize,
    nickname: String,
    avatar: Option<String>,
    solved_count: u64,
    solved_last_at: DateTime<FixedOffset>,
}

/// GET /api/challenge_solves/top15users (scope `/solves`)
#[get("/top15users")]
pub async fn get_top_15_users(_user: UserJwtGuard, ctx: ReqCtx) -> UniResult<Vec<TopUser>> {
    let practice = events::Entity::find()
        .filter(events::Column::SystemKey.eq(PRACTICE_JEOPARDY_SYSTEM_KEY))
        .one(ctx.db.get_ref())
        .await?;

    let Some(practice) = practice else {
        return UniResponse::ok(Some(Vec::new())).into();
    };

    let solves = jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(practice.id))
        .all(ctx.db.get_ref())
        .await?;

    let mut stats: HashMap<Uuid, (u64, DateTime<FixedOffset>)> = HashMap::new();

    for s in solves {
        stats
            .entry(s.user_id)
            .and_modify(|(cnt, last)| {
                *cnt += 1;
                if s.created_at > *last {
                    *last = s.created_at;
                }
            })
            .or_insert((1, s.created_at));
    }

    let mut result = Vec::new();
    for (uid, (count, last)) in stats {
        if let Some(user) = users::Entity::find_by_id(uid).one(ctx.db.get_ref()).await? {
            result.push((user.nickname, user.avatar, count, last));
        }
    }

    result.sort_by(|a, b| {
        b.2.cmp(&a.2) // solved_count
            .then_with(|| b.3.cmp(&a.3)) // last solve time
    });
    result.truncate(15);

    let result: Vec<TopUser> = result
        .into_iter()
        .enumerate()
        .map(|(idx, (nickname, avatar, count, last))| TopUser {
            no: idx + 1,
            nickname,
            avatar,
            solved_count: count,
            solved_last_at: last,
        })
        .collect();
    UniResponse::ok(result.into()).into()
}
