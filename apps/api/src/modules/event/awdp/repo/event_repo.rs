//! awdp_events 仓储：纯配置读写（运行态全部在 awdp_runs）。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait, sea_query::LockType,
};
use uuid::Uuid;

use crate::entity::{awdp_events, awdp_runs, events, sea_orm_active_enums::EventFamily};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::{AwdpConfig, AwdpConfigPatch},
    repo::run_repo,
};

pub async fn find_by_event_id<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
) -> Result<Option<awdp_events::Model>, sea_orm::DbErr> {
    awdp_events::Entity::find_by_id(event_id).one(db).await
}

pub async fn require_by_event_id<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
) -> AwdpResult<awdp_events::Model> {
    find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound(format!("awdp event {event_id} not configured")))
}

/// 确保 awdp_events 行存在（默认配置）。创建/读取事件时调用。
/// 不再承载 next_action_at（tick 排期在 run 上）。
pub async fn ensure_by_event_id(
    db: &DatabaseConnection,
    event_id: Uuid,
    config: &AwdpConfig,
) -> AwdpResult<awdp_events::Model> {
    if let Some(existing) = find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
    {
        return Ok(existing);
    }
    config.validate()?;
    let now = Utc::now().into();
    awdp_events::ActiveModel {
        event_id: Set(event_id),
        break_duration_secs: Set(config.break_duration_secs),
        fix_duration_secs: Set(config.fix_duration_secs),
        fix_round_interval_secs: Set(config.fix_round_interval_secs),
        break_score: Set(config.break_score),
        fix_round_score: Set(config.fix_round_score),
        configuration_generation: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 乐观锁配置更新：仅“尚无 active run”（事件未启动）可改；expected_updated_at 必填。
/// 锁序：events → awdp_events。成功时同步 events.end_time = start + break + fix（plan §4，事务性）。
pub async fn update_config(
    db: &DatabaseConnection,
    event_id: Uuid,
    patch: AwdpConfigPatch,
) -> AwdpResult<awdp_events::Model> {
    patch.validate()?;
    let expected = patch.expected_updated_at.ok_or_else(|| {
        AwdpError::Validation("expected_updated_at is required for config update".into())
    })?;

    let txn = db
        .begin()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    // 锁序：events → awdp_events。
    let event = events::Entity::find_by_id(event_id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("event not found".into()))?;
    if event.family != EventFamily::Awdp {
        return Err(AwdpError::Validation(format!(
            "event {event_id} is not an AWDP event"
        )));
    }

    let current = awdp_events::Entity::find_by_id(event_id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("awdp event not configured".into()))?;

    // 运行态已迁到 run：存在 active competition run = 事件已启动 → 配置冻结。
    if run_repo::find_active_competition_for_event(&txn, event_id)
        .await?
        .is_some()
    {
        return Err(AwdpError::InvalidState(
            "AWDP config is locked (event has an active run)".into(),
        ));
    }
    // 乐观锁。
    if expected != current.updated_at {
        return Err(AwdpError::Conflict(
            "concurrent awdp config update (expected_updated_at mismatch)".into(),
        ));
    }

    let base = AwdpConfig {
        break_duration_secs: current.break_duration_secs,
        fix_duration_secs: current.fix_duration_secs,
        fix_round_interval_secs: current.fix_round_interval_secs,
        break_score: current.break_score,
        fix_round_score: current.fix_round_score,
    };
    let next = patch.apply_to(&base);
    next.validate()?;

    let now = Utc::now().into();
    let model = awdp_events::ActiveModel {
        event_id: Set(event_id),
        break_duration_secs: Set(next.break_duration_secs),
        fix_duration_secs: Set(next.fix_duration_secs),
        fix_round_interval_secs: Set(next.fix_round_interval_secs),
        break_score: Set(next.break_score),
        fix_round_score: Set(next.fix_round_score),
        configuration_generation: Set(current.configuration_generation + 1),
        updated_at: Set(now),
        ..Default::default()
    }
    .update(&txn)
    .await
    .map_err(|e| AwdpError::Database(e.to_string()))?;

    // events.end_time 与 start + break + fix 保持一致。
    {
        let start = event.start_time;
        let fix_end = start
            + chrono::Duration::seconds(next.break_duration_secs as i64)
            + chrono::Duration::seconds(next.fix_duration_secs as i64);
        let mut ev: events::ActiveModel = event.clone().into();
        ev.end_time = Set(Some(fix_end));
        ev.update(&txn)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
    }

    txn.commit()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(model)
}

/// 扫描“应启动但尚无 run”的 AWDP 事件（start_time 已到、家族 awdp、无任何 run）。
/// tick 用它自动创建 pending competition run（随后 pending 分支到点转 Break）。
/// 并发由 create_competition_run 的 active-unique 幂等兜底。
pub async fn find_unstarted_awdp_events(
    db: &DatabaseConnection,
    now: chrono::DateTime<chrono::Utc>,
    limit: u64,
) -> AwdpResult<Vec<events::Model>> {
    use sea_orm::QuerySelect;
    use std::collections::HashSet;

    let rows = events::Entity::find()
        .filter(events::Column::Family.eq(EventFamily::Awdp))
        .filter(events::Column::StartTime.lte(now))
        .order_by_asc(events::Column::StartTime)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // 已存在 run 的事件跳过（含 ended —— 事件只跑一次）。
    let used: HashSet<Uuid> = awdp_runs::Entity::find()
        .select_only()
        .column(awdp_runs::Column::EventId)
        .into_tuple::<Option<Uuid>>()
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .into_iter()
        .flatten()
        .collect();

    Ok(rows
        .into_iter()
        .filter(|e| !used.contains(&e.id))
        .take(limit as usize)
        .collect())
}
