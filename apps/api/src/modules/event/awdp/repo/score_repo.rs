//! awdp_score_events 仓储（append-only 幂等账本；run 作用域）。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::awdp_score_events;
use crate::modules::event::awdp::{AwdpError, AwdpResult, repo::run_repo};

/// 写入账本（ON CONFLICT DO NOTHING 语义：idempotency_key 冲突视为已写入）。
#[allow(clippy::too_many_arguments)]
pub async fn create_score_event(
    db: &DatabaseConnection,
    run_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
    gamebox_id: Uuid,
    score_type: &str,
    fix_round_id: Option<Uuid>,
    delta: i64,
    idempotency_key: &str,
) -> AwdpResult<bool> {
    let event_id = run_repo::event_id_for_team_fk(db, run_id).await?;
    let now = Utc::now().into();
    let model = awdp_score_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run_id),
        event_id: Set(event_id),
        user_id: Set(user_id),
        team_id: Set(team_id),
        gamebox_id: Set(gamebox_id),
        score_type: Set(score_type.to_string()),
        fix_round_id: Set(fix_round_id),
        delta: Set(delta),
        idempotency_key: Set(idempotency_key.to_string()),
        created_at: Set(now),
    };
    match model.insert(db).await {
        Ok(_) => Ok(true),
        // 幂等：重复执行不重复加分。
        Err(e)
            if e.to_string()
                .contains("awdp_score_events_idempotency_key_uidx") =>
        {
            Ok(false)
        }
        Err(e) => Err(AwdpError::Database(e.to_string())),
    }
}

/// run 的总分视图（按双主体）。
pub async fn total_for_run(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Vec<(Option<Uuid>, Option<Uuid>, i64)>> {
    use sea_orm::{QuerySelect, sea_query::Expr};

    let rows = awdp_score_events::Entity::find()
        .filter(awdp_score_events::Column::RunId.eq(run_id))
        .select_only()
        .column_as(awdp_score_events::Column::UserId, "user_id")
        .column_as(awdp_score_events::Column::TeamId, "team_id")
        .column_as(
            Expr::col(awdp_score_events::Column::Delta)
                .sum()
                .cast_as(sea_orm::sea_query::Alias::new("bigint")),
            "total",
        )
        .group_by(awdp_score_events::Column::UserId)
        .group_by(awdp_score_events::Column::TeamId)
        .into_model::<ScoreAggRow>()
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| (r.user_id, r.team_id, r.total.unwrap_or(0)))
        .collect())
}

#[derive(Debug, serde::Deserialize, sea_orm::FromQueryResult)]
struct ScoreAggRow {
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
    total: Option<i64>,
}

/// 我的得分（user 或 team）。
pub async fn my_total(
    db: &DatabaseConnection,
    run_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
) -> AwdpResult<i64> {
    let mut q =
        awdp_score_events::Entity::find().filter(awdp_score_events::Column::RunId.eq(run_id));
    q = match (user_id, team_id) {
        (Some(u), None) => q.filter(awdp_score_events::Column::UserId.eq(u)),
        (None, Some(t)) => q.filter(awdp_score_events::Column::TeamId.eq(t)),
        _ => return Err(AwdpError::Internal("exactly-one owner required".into())),
    };
    use sea_orm::{QuerySelect, sea_query::Expr};
    #[derive(sea_orm::FromQueryResult)]
    struct TotalRow {
        total: Option<i64>,
    }
    let row = q
        .select_only()
        .column_as(
            Expr::col(awdp_score_events::Column::Delta)
                .sum()
                .cast_as(sea_orm::sea_query::Alias::new("bigint")),
            "total",
        )
        .into_model::<TotalRow>()
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(row.and_then(|r| r.total).unwrap_or(0))
}

/// 我的全部得分明细（展示 Break/Fix 历史）。
pub async fn my_history(
    db: &DatabaseConnection,
    run_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
) -> AwdpResult<Vec<awdp_score_events::Model>> {
    let mut q =
        awdp_score_events::Entity::find().filter(awdp_score_events::Column::RunId.eq(run_id));
    q = match (user_id, team_id) {
        (Some(u), None) => q.filter(awdp_score_events::Column::UserId.eq(u)),
        (None, Some(t)) => q.filter(awdp_score_events::Column::TeamId.eq(t)),
        _ => return Err(AwdpError::Internal("exactly-one owner required".into())),
    };
    q.order_by_desc(awdp_score_events::Column::CreatedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 事件总分视图（admin scoreboard 用；经 run 聚合）。
pub async fn total_for_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Vec<(Option<Uuid>, Option<Uuid>, i64)>> {
    use sea_orm::{QuerySelect, sea_query::Expr};
    let runs = run_repo::list_for_event(db, event_id).await?;
    if runs.is_empty() {
        return Ok(Vec::new());
    }
    let run_ids: Vec<Uuid> = runs.iter().map(|r| r.id).collect();
    let rows = awdp_score_events::Entity::find()
        .filter(awdp_score_events::Column::RunId.is_in(run_ids))
        .select_only()
        .column_as(awdp_score_events::Column::UserId, "user_id")
        .column_as(awdp_score_events::Column::TeamId, "team_id")
        .column_as(
            Expr::col(awdp_score_events::Column::Delta)
                .sum()
                .cast_as(sea_orm::sea_query::Alias::new("bigint")),
            "total",
        )
        .group_by(awdp_score_events::Column::UserId)
        .group_by(awdp_score_events::Column::TeamId)
        .into_model::<ScoreAggRow>()
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| (r.user_id, r.team_id, r.total.unwrap_or(0)))
        .collect())
}
