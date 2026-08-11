//! awdp_events 仓储：配置读写 + 阶段迁移（CAS）。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QuerySelect, TransactionTrait, sea_query::LockType,
};
use uuid::Uuid;

use crate::entity::{awdp_events, events, sea_orm_active_enums::AwdpPhase};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::{AwdpConfig, AwdpConfigPatch, AwdpPhaseExt},
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
/// pending 且未排期 → next_action_at = events.start_time（tick 驱动开始）。
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
    let next_action_at = crate::entity::events::Entity::find_by_id(event_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .map(|e| e.start_time.with_timezone(&Utc));
    awdp_events::ActiveModel {
        event_id: Set(event_id),
        phase: Set(AwdpPhase::Pending),
        break_duration_secs: Set(config.break_duration_secs),
        fix_duration_secs: Set(config.fix_duration_secs),
        fix_round_interval_secs: Set(config.fix_round_interval_secs),
        break_score: Set(config.break_score),
        fix_round_score: Set(config.fix_round_score),
        configuration_generation: Set(1),
        current_round: Set(0),
        next_action_at: Set(next_action_at.map(|t| t.into())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 当前阶段是否为 pending（未开始）。
pub async fn is_pending(db: &DatabaseConnection, event_id: Uuid) -> AwdpResult<bool> {
    Ok(require_by_event_id(db, event_id).await?.phase == AwdpPhase::Pending)
}

/// 乐观锁配置更新：仅 phase=pending 可改；expected_updated_at 必填。
/// 锁序：events → awdp_events。
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
    if event.family != crate::entity::sea_orm_active_enums::EventFamily::Awdp {
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

    if current.phase != AwdpPhase::Pending {
        return Err(AwdpError::InvalidState(format!(
            "AWDP config is locked in phase {:?}",
            current.phase
        )));
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

    // events.end_time 与 start + break + fix 保持一致（plan §4，事务性）。
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

/// 阶段迁移要同步写入的时间戳/进度字段。
#[derive(Debug, Default, Clone)]
pub struct PhaseTransitionPatch {
    pub started_at: Option<DateTime<Utc>>,
    pub break_ends_at: Option<DateTime<Utc>>,
    pub fix_started_at: Option<DateTime<Utc>>,
    pub fix_ends_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub current_round: Option<i32>,
    pub next_action_at: Option<DateTime<Utc>>,
}

/// 阶段迁移：lock → can_transition_to → CAS → 原子 UPDATE（单写者）。
pub async fn transition_phase(
    db: &DatabaseConnection,
    event_id: Uuid,
    expected: AwdpPhase,
    target: AwdpPhase,
    patch: PhaseTransitionPatch,
) -> AwdpResult<()> {
    expected
        .can_transition_to(target.clone())
        .map_err(AwdpError::InvalidState)?;

    let txn = db
        .begin()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    let current = awdp_events::Entity::find_by_id(event_id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("awdp event not configured".into()))?;

    // CAS：并发 phase 迁移拒绝。
    if current.phase != expected {
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Err(AwdpError::Conflict(format!(
            "concurrent AWDP phase transition: current={:?} expected={expected:?}",
            current.phase
        )));
    }

    let now = Utc::now();
    let mut am: awdp_events::ActiveModel = current.clone().into();
    am.phase = Set(target);
    if let Some(v) = patch.started_at {
        am.started_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.break_ends_at {
        am.break_ends_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.fix_started_at {
        am.fix_started_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.fix_ends_at {
        am.fix_ends_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.finished_at {
        am.finished_at = Set(Some(v.into()));
    }
    if let Some(v) = patch.current_round {
        am.current_round = Set(v);
    }
    if let Some(v) = patch.next_action_at {
        am.next_action_at = Set(Some(v.into()));
    }
    am.updated_at = Set(now.into());
    am.update(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    txn.commit()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

/// 轻量更新 next_action_at / current_round（tick 推进用）。
pub async fn touch_tick_state(
    db: &DatabaseConnection,
    event_id: Uuid,
    current_round: i32,
    next_action_at: DateTime<Utc>,
) -> AwdpResult<()> {
    awdp_events::ActiveModel {
        event_id: Set(event_id),
        current_round: Set(current_round),
        next_action_at: Set(Some(next_action_at.into())),
        updated_at: Set(Utc::now().into()),
        ..Default::default()
    }
    .update(db)
    .await
    .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

/// tick 扫描：due 事件（next_action_at <= now）FOR UPDATE SKIP LOCKED。
pub async fn find_due_events(
    db: &DatabaseConnection,
    now: DateTime<Utc>,
    limit: u64,
) -> AwdpResult<Vec<awdp_events::Model>> {
    use sea_orm::{QueryOrder, sea_query::LockBehavior};

    let rows = awdp_events::Entity::find()
        .filter(awdp_events::Column::NextActionAt.lte(now))
        .filter(awdp_events::Column::Phase.ne(AwdpPhase::Ended))
        .order_by_asc(awdp_events::Column::NextActionAt)
        .limit(limit)
        .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(rows)
}
