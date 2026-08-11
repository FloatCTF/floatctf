//! awdp_fix_rounds 仓储（确定性回合时间线）。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::entity::awdp_fix_rounds;
use crate::modules::event::awdp::{AwdpError, AwdpResult, domain::round_windows, repo::event_repo};

/// Fix 开始时预生成全部回合（幂等：已存在的 sequence 跳过）。
pub async fn materialize_rounds(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Vec<awdp_fix_rounds::Model>> {
    let awdp = event_repo::require_by_event_id(db, event_id).await?;
    let fix_start = awdp
        .fix_started_at
        .ok_or_else(|| AwdpError::InvalidState("fix_started_at is not set".into()))?
        .with_timezone(&Utc);
    let count = awdp.fix_duration_secs / awdp.fix_round_interval_secs;
    let windows = round_windows(
        fix_start,
        chrono::Duration::seconds(awdp.fix_round_interval_secs as i64),
        count,
    );

    let mut created = Vec::new();
    for w in windows {
        let exists = awdp_fix_rounds::Entity::find()
            .filter(awdp_fix_rounds::Column::EventId.eq(event_id))
            .filter(awdp_fix_rounds::Column::Sequence.eq(w.sequence))
            .count(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            > 0;
        if exists {
            continue;
        }
        let now = Utc::now().into();
        let model = awdp_fix_rounds::ActiveModel {
            id: Set(Uuid::new_v4()),
            event_id: Set(event_id),
            sequence: Set(w.sequence),
            starts_at: Set(w.starts_at.into()),
            cutoff_at: Set(w.cutoff_at.into()),
            status: Set("pending".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
        created.push(model);
    }
    Ok(created)
}

/// 当前进行中（starts_at <= now < cutoff_at）或最近到期的回合。
pub async fn current_open_round(
    db: &DatabaseConnection,
    event_id: Uuid,
    now: DateTime<Utc>,
) -> AwdpResult<Option<awdp_fix_rounds::Model>> {
    let rows = awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::EventId.eq(event_id))
        .filter(awdp_fix_rounds::Column::StartsAt.lte(now))
        .filter(awdp_fix_rounds::Column::CutoffAt.gt(now))
        .order_by_asc(awdp_fix_rounds::Column::Sequence)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(rows.into_iter().next())
}

/// 下一个尚未 cut 的回合（含已过 starts_at 的，用于 tick 到期判定）。
pub async fn next_due_round(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Option<awdp_fix_rounds::Model>> {
    let row = awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::EventId.eq(event_id))
        .filter(awdp_fix_rounds::Column::Status.ne("completed"))
        .order_by_asc(awdp_fix_rounds::Column::Sequence)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(row)
}

pub async fn find_by_sequence(
    db: &DatabaseConnection,
    event_id: Uuid,
    sequence: i32,
) -> AwdpResult<Option<awdp_fix_rounds::Model>> {
    awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::EventId.eq(event_id))
        .filter(awdp_fix_rounds::Column::Sequence.eq(sequence))
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

pub async fn find_by_id(db: &DatabaseConnection, id: Uuid) -> AwdpResult<awdp_fix_rounds::Model> {
    awdp_fix_rounds::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("awdp fix round not found".into()))
}

pub async fn list_for_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Vec<awdp_fix_rounds::Model>> {
    awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::EventId.eq(event_id))
        .order_by_asc(awdp_fix_rounds::Column::Sequence)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 标记回合状态（evaluating / completed）。
pub async fn set_status(db: &DatabaseConnection, round_id: Uuid, status: &str) -> AwdpResult<()> {
    let now = Utc::now().into();
    let mut am: awdp_fix_rounds::ActiveModel = find_by_id(db, round_id).await?.into();
    am.status = Set(status.to_string());
    if status == "evaluating" && am.started_at.is_not_set() {
        am.started_at = Set(Some(now));
    }
    if status == "completed" {
        am.finished_at = Set(Some(now));
    }
    am.updated_at = Set(now);
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}
