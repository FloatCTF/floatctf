//! 选手题目目录处理器。

use std::collections::HashSet;
use std::str::FromStr;

use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryTrait, sea_query::Expr};

use crate::modules::challenge::catalog::ChallengesDto;
use crate::modules::event::jeopardy::api::InstancesDto;
use crate::{
    api::{FilterMapping, apply_filters, prelude::*, sea_orm_utils::paginate_query},
    entity::{
        challenges, event_challenge_instance, event_instances, event_team_members,
        jeopardy_challenge_solves,
    },
};

/// 当前用户是否解过该题（含团队题：本人提交，或本人所在队伍已解）。
/// 用法：`Expr::cust_with_values(SQL, vec![Value::from(user_id), Value::from(user_id)])`。
const SOLVED_CHALLENGE_CONDITION_SQL: &str = r#"EXISTS (
    SELECT 1 FROM public.jeopardy_challenge_solves s
    WHERE s.challenge_id = challenges.id
      AND (s.user_id = $1 OR s.team_id IN (
        SELECT m.team_id FROM public.event_team_members m
        WHERE m.user_id = $2 AND m.event_id = s.event_id))
)"#;
const NOT_SOLVED_CHALLENGE_CONDITION_SQL: &str = r#"NOT EXISTS (
    SELECT 1 FROM public.jeopardy_challenge_solves s
    WHERE s.challenge_id = challenges.id
      AND (s.user_id = $1 OR s.team_id IN (
        SELECT m.team_id FROM public.event_team_members m
        WHERE m.user_id = $2 AND m.event_id = s.event_id))
)"#;

/// 取当前用户已解题 id 集合（去重）。
pub async fn solved_challenge_ids_for<C: sea_orm::ConnectionTrait>(
    db: &C,
    user_id: uuid::Uuid,
) -> Result<HashSet<uuid::Uuid>, sea_orm::DbErr> {
    let rows = jeopardy_challenge_solves::Entity::find()
        .filter(
            Condition::any()
                .add(jeopardy_challenge_solves::Column::UserId.eq(user_id))
                .add(
                    jeopardy_challenge_solves::Column::TeamId.in_subquery(
                        event_team_members::Entity::find()
                            .select_only()
                            .column(event_team_members::Column::TeamId)
                            .filter(event_team_members::Column::UserId.eq(user_id))
                            .into_query(),
                    ),
                ),
        )
        .select_only()
        .column(jeopardy_challenge_solves::Column::ChallengeId)
        .distinct()
        .into_tuple::<(uuid::Uuid,)>()
        .all(db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(challenge_id,)| challenge_id)
        .collect())
}

/// GET /api/challenges
#[get("")]
pub async fn get_challenges(
    user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<ChallengesDto>> {
    let user = user.into_inner();
    let mut query_params = query_params.0;

    // 一次批量查询取当前用户已解题集合（solved 列 + solved 筛选共用）。
    let solved_challenge_ids = solved_challenge_ids_for(ctx.db.get_ref(), user.id).await?;

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
        FilterMapping {
            key: "solved",
            column: Box::new(move |v: &str| {
                let want_solved = matches!(
                    v.trim().to_lowercase().as_str(),
                    "true" | "1" | "yes" | "y" | "是"
                );
                Condition::all().add(Expr::cust_with_values(
                    if want_solved {
                        SOLVED_CHALLENGE_CONDITION_SQL
                    } else {
                        NOT_SOLVED_CHALLENGE_CONDITION_SQL
                    },
                    vec![sea_orm::Value::from(user.id), sea_orm::Value::from(user.id)],
                ))
            }),
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

    // 玩家侧返回 enriched DTO（当前 package 摘要 + 附件元数据 + solved），
    // 附件链接由前端按 /static/challenges/... 构造。
    let dtos: Vec<ChallengesDto> = items
        .into_iter()
        .map(|m| {
            let mut dto = ChallengesDto::from(&m);
            dto.solved = solved_challenge_ids.contains(&dto.id);
            dto
        })
        .collect();
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
    let instance = event_challenge_instance::Entity::find()
        .filter(event_challenge_instance::Column::ChallengeId.eq(challenge_id))
        .filter(event_challenge_instance::Column::UserId.eq(user.id))
        .filter(event_challenge_instance::Column::EventId.eq(practice.id))
        .find_also_related(event_instances::Entity)
        .filter(event_instances::Column::RuntimeState.eq("running"))
        .one(ctx.db.get_ref())
        .await?;

    let dto = instance.and_then(|(inst, runtime)| {
        runtime.map(|runtime| InstancesDto::from_pair(&inst, &runtime))
    });

    UniResponse::ok(dto).into()
}
