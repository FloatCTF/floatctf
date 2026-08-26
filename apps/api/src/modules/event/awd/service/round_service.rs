//! 轮次生命周期服务（Wave 2）。
//!
//! 推进流程：
//! ```text
//! Round N End → Round N Completed
//!             → 创建 Judge batch/tasks for Round N
//!             → 若 N < round_count → 直接启动 Round N+1
//!             → 若 N == round_count → 无下一轮（Running + Attack + 无活跃轮次）
//! ```
//!
//! Judge 与 Round 独立推进：Round N+1 不等待 Round N Judge 完成。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::entity::{
    awd_events, awd_judge_tasks, awd_rounds, scheduled_tasks,
    sea_orm_active_enums::{
        AwdEventStatus, AwdPhase, RoundStatus,
    },
};
use crate::infrastructure::realtime::EventPublisher;
use crate::modules::event::awd::{
    AwdError, AwdResult,
    infrastructure::{firewall::FirewallRuntime, network::AwdNetworkRuntime},
    repo::{event_repo, round_repo},
    service::{firewall_service, judge_service},
    websocket,
};
use crate::scheduler::TaskKey;

/// Round 任务 payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundTaskPayload {
    pub event_id: Uuid,
    pub round_id: Option<Uuid>,
    /// RoundStart 任务携带期望 round_number（幂等键维度）。
    pub round_number: Option<i32>,
}

/// RoundStart 处理结果。
#[derive(Debug, Clone)]
pub struct RoundStarted {
    pub round_id: Uuid,
    pub round_number: i32,
    pub phase: AwdPhase,
    /// true = 本次实际创建了新 round；false = 幂等命中已存在 round（retry）。
    pub created: bool,
}

/// 在事务内创建一次性 round 任务。
async fn schedule_round_task<C: ConnectionTrait + Send>(
    txn: &C,
    event_id: Uuid,
    task_key: TaskKey,
    execute_at: chrono::DateTime<chrono::FixedOffset>,
    round_id: Option<Uuid>,
    round_number: Option<i32>,
) -> Result<(), sea_orm::DbErr> {
    let now = chrono::Utc::now();
    scheduled_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        group_id: Set(Some(event_id)),
        task_name: Set(format!("AWD round task {task_key} event {event_id}")),
        description: Set(Some(format!("round_id={}", round_id.unwrap_or_default()))),
        task_key: Set(task_key.to_string()),
        trigger_type: Set("once".into()),
        status: Set("pending".into()),
        execute_at: Set(Some(execute_at)),
        payload: Set(Some(
            serde_json::to_value(RoundTaskPayload {
                event_id,
                round_id,
                round_number,
            })
            .map_err(|e| sea_orm::DbErr::Json(e.to_string()))?,
        )),
        enabled: Set(true),
        protected: Set(true),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(txn)
    .await?;
    Ok(())
}

/// 开始一轮 Attack 轮次。
///
/// 幂等：同 (event_id, round_number) 已存在 → 返回既有 round（retry 安全）。
///
/// 所有轮次 phase 均为 Attack。Hardening 不再是 Round 1。
pub async fn start_round(
    db: &sea_orm::DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn EventPublisher,
    event_id: Uuid,
    expected_round_number: Option<i32>,
) -> AwdResult<RoundStarted> {
    let txn = db
        .begin()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let awd_event = event_repo::find_by_event_id(&txn, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if awd_event.status != AwdEventStatus::Running {
        warn!(
            "[Round] Event {} is not Running ({:?}) — skip round start",
            event_id, awd_event.status
        );
        txn.rollback()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        return Err(AwdError::InvalidState(format!(
            "Cannot start round in {:?} status",
            awd_event.status
        )));
    }

    if awd_event.phase != AwdPhase::Attack {
        warn!(
            "[Round] Event {} phase is {:?} — skip round start",
            event_id, awd_event.phase
        );
        txn.rollback()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        return Err(AwdError::InvalidState(
            "Cannot start round outside Attack phase".into(),
        ));
    }

    // round number：任务携带期望值则用之；否则 latest + 1（或 1）
    let round_number = match expected_round_number {
        Some(n) if n >= 1 => n,
        _ => {
            let latest = round_repo::find_latest_round(&txn, event_id)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
            latest.map(|r| r.round_number + 1).unwrap_or(1)
        }
    };

    // 幂等：同 round_number 已存在 → 直接返回
    let existing = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .filter(awd_rounds::Column::RoundNumber.eq(round_number))
        .one(&txn)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    if let Some(round) = existing {
        txn.rollback()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        return Ok(RoundStarted {
            round_id: round.id,
            round_number: round.round_number,
            phase: round.phase,
            created: false,
        });
    }

    let phase = AwdPhase::Attack;

    let now = chrono::Utc::now();
    let round_end = now + chrono::Duration::seconds(awd_event.round_duration_secs as i64);
    let round = round_repo::create_round(&txn, event_id, round_number, phase.clone(), round_end)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 插入 RoundEnd 任务
    schedule_round_task(
        &txn,
        event_id,
        TaskKey::AwdRoundEnd,
        round_end.fixed_offset(),
        Some(round.id),
        None,
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;

    txn.commit()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    info!(
        "[Round] Round {} (phase {:?}) started for event {}",
        round.round_number, phase, event_id
    );

    // ── COMMIT 后：网络 reconcile ──
    let revision = firewall_service::next_network_revision(db).await?;
    match firewall_service::reconcile_global(db, firewall, revision).await {
        Ok(_) => {
            let event_network =
                crate::modules::event::awd::repo::event_network_repo::require_by_event_id(
                    db, event_id,
                )
                .await?;
            firewall_service::flush_event_connections(
                network,
                event_id,
                &event_network.gamebox_cidr.to_string(),
            )
            .await;
            let phase_str = format!("{:?}", phase).to_lowercase();
            let _ = publisher
                .publish(
                    websocket::network_policy_applied(event_id, revision, revision, &phase_str)
                        .into_realtime(),
                )
                .await;
            let _ = publisher
                .publish(
                    websocket::round_started(event_id, round.round_number, &phase_str)
                        .into_realtime(),
                )
                .await;
        }
        Err(e) => {
            let phase_str = format!("{:?}", phase).to_lowercase();
            let _ = publisher
                .publish(
                    websocket::network_policy_failed(event_id, revision, None, &phase_str)
                        .into_realtime(),
                )
                .await;
            return Err(e);
        }
    }

    Ok(RoundStarted {
        round_id: round.id,
        round_number: round.round_number,
        phase,
        created: true,
    })
}

/// RoundEnd：立即完成轮次，创建 Judge 任务，若 N < round_count 立即启动 N+1。
///
/// 幂等：非 Active 状态 → 直接成功跳过。
///
/// **执行顺序（关键）**：
/// 1. COMMIT Round N Completed
/// 2. 创建 Judge batch（仅 DB）
/// 3. 若非最终轮：启动 Round N+1（含 Clock 调度）
/// 4. 调度 batch deadline 任务（Pull Judge 超时保护）
/// 5. 发布 SSE
///
/// Round N+1 计时在 Judge 回调之前就开始，确保轮次时钟不受远程 Judge 响应时间影响。
pub async fn end_round(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn EventPublisher,
) -> AwdResult<()> {
    // ── 事务内：完成轮次 ──
    let txn = db
        .begin()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let round = awd_rounds::Entity::find_by_id(round_id)
        .one(&txn)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("Round not found".into()))?;
    if round.event_id != event_id {
        txn.rollback()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        return Err(AwdError::Forbidden("Round belongs to another event".into()));
    }

    if round.status != RoundStatus::Active {
        warn!(
            "[Round] Round {} already {:?} — skip",
            round_id, round.status
        );
        txn.rollback()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        return Ok(());
    }

    // 完成轮次
    round_repo::update_round_status(&txn, round_id, RoundStatus::Completed)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let awd_event = event_repo::find_by_event_id(&txn, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    let round_count = awd_event.round_count;
    let round_number = round.round_number;
    let is_final = round_count.map(|rc| round_number >= rc).unwrap_or(false);

    txn.commit()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let completed_round_number = round_number;
    info!(
        "[Round] Round {round_id} (number {completed_round_number}) completed for event {event_id}"
    );

    // ── 步骤 2：创建 Judge batch（仅 DB，不动 HTTP）──
    let judge_result = create_judge_batch_for_round(db, event_id, round_id).await;
    if let Err(ref e) = judge_result {
        warn!(
            "[Round] Judge batch creation failed for round {round_id}: {e}"
        );
    }

    // ── 步骤 3：下一轮 ──
    // 在 Judge HTTP 推送之前启动下一轮，确保轮次时钟不受远程响应影响
    if is_final {
        info!(
            "[Round] Final round {completed_round_number} ended — no next round. Event {} remains Running/Attack.",
            event_id
        );
    } else {
        info!(
            "[Round] Starting round {} immediately for event {}",
            completed_round_number + 1,
            event_id
        );
        if let Err(e) = start_round(
            db,
            network,
            firewall,
            publisher,
            event_id,
            Some(completed_round_number + 1),
        )
        .await
        {
            warn!(
                "[Round] Failed to start round {} for event {}: {e}",
                completed_round_number + 1,
                event_id
            );
        }
    }

    // ── 步骤 4：调度 batch deadline 任务 ──
    if let Ok(batch_id) = &judge_result {
        if let Ok(deadline) = max_task_deadline(db, *batch_id).await {
            let txn = db
                .begin()
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
            if let Err(e) =
                schedule_batch_deadline_task(&txn, event_id, *batch_id, deadline).await
            {
                warn!(
                    "[Round] Failed to schedule batch deadline for batch {batch_id}: {e}"
                );
                let _ = txn.rollback().await;
            } else {
                let _ = txn.commit().await;
            }
        }
    }

    // 发布 SSE（best-effort）
    let _ = publisher
        .publish(websocket::round_completed(event_id, completed_round_number).into_realtime())
        .await;

    Ok(())
}

/// 创建 Judge batch（仅 DB 操作，不涉及 HTTP）。
/// 返回 batch_id 供后续 dispatch 使用。
async fn create_judge_batch_for_round(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
) -> AwdResult<Uuid> {
    judge_service::create_batch(db, event_id, round_id).await
}

/// P3-13：重启后恢复当前 round 的调度任务（幂等）。
///
/// 恢复规则：
/// - Active round：缺 pending `awd.round.end` 任务 → 按 scheduled_end_at 重建；
/// - Paused round：暂停中，不恢复任务（resume 时重建）。
/// - 无 Active round 且 Running/Attack：调用 `recover_round_gap` 处理崩溃间隙。
pub async fn restore_round_scheduling(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn EventPublisher,
) -> AwdResult<usize> {
    use crate::entity::sea_orm_active_enums::RoundStatus as RS;

    let maybe_round = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .filter(awd_rounds::Column::Status.is_in([RS::Active, RS::Paused]))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    match maybe_round {
        Some(round) => match round.status {
            RS::Active => {
                restore_round_end_task(db, event_id, round.id, round.scheduled_end_at).await
            }
            RS::Paused => {
                // 暂停中：任务挂起由 resume 重建
                Ok(0)
            }
            _ => Ok(0),
        },
        None => {
            // 无 Active/Paused 轮次 → 检查是否需要恢复崩溃间隙
            let awd_event = event_repo::find_by_event_id(db, event_id)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?
                .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;
            if awd_event.status == AwdEventStatus::Running
                && awd_event.phase == AwdPhase::Attack
            {
                recover_round_gap(db, event_id, network, firewall, publisher).await
            } else {
                Ok(0)
            }
        }
    }
}

/// 恢复单个 RoundEnd 任务。
async fn restore_round_end_task(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
    scheduled_end_at: chrono::DateTime<chrono::FixedOffset>,
) -> AwdResult<usize> {
    let found = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq(TaskKey::AwdRoundEnd.to_string()))
        .filter(scheduled_tasks::Column::Status.eq("pending"))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    if found.is_some() {
        return Ok(0);
    }

    let txn = db
        .begin()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    schedule_round_task(
        &txn,
        event_id,
        TaskKey::AwdRoundEnd,
        scheduled_end_at.fixed_offset(),
        Some(round_id),
        None,
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;
    txn.commit()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(1)
}

/// 崩溃间隙恢复：Running + Attack + 无活跃轮次。
///
/// 四种情况：
/// - CASE A: 无任何轮次 → Hardening 结束后崩溃，Round 1 尚未创建 → 启动 Round 1
/// - CASE B: 最高已完成轮次 N < round_count → 崩溃在 Round N 完成与 Round N+1 启动之间 → 启动 Round N+1
/// - CASE C: 最高已完成轮次 N == round_count → 最终结算条件 → 不启动
/// - CASE D: 最高轮次 > round_count → 不变量违反 → 记录错误，不操作
pub async fn recover_round_gap(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn EventPublisher,
) -> AwdResult<usize> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    let round_count = match awd_event.round_count {
        Some(rc) if rc > 0 => rc,
        _ => {
            warn!(
                "[Recovery] Event {} has no round_count configured — cannot recover round gap",
                event_id
            );
            return Ok(0);
        }
    };

    // 查找最高轮次（按 round_number 降序）
    let highest_round = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .order_by_desc(awd_rounds::Column::RoundNumber)
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    match highest_round {
        None => {
            // CASE A: 无任何轮次 → Hardening 结束后崩溃
            info!(
                "[Recovery] Event {} Attack + no rounds — starting Round 1 (hardening-end crash recovery)",
                event_id
            );
            start_round(db, network, firewall, publisher, event_id, Some(1)).await?;
            Ok(1)
        }
        Some(round) => {
            let n = round.round_number;
            if n > round_count {
                // CASE D: 不变量违反
                warn!(
                    "[Recovery] Event {} highest round {} exceeds round_count {} — invariant violation, no recovery",
                    event_id, n, round_count
                );
                Ok(0)
            } else if n >= round_count {
                // CASE C: 最终结算条件
                info!(
                    "[Recovery] Event {} final round {} completed, round_count={} — final settlement, no recovery",
                    event_id, n, round_count
                );
                Ok(0)
            } else {
                // CASE B: 崩溃间隙 → 启动下一轮
                let next = n + 1;
                info!(
                    "[Recovery] Event {} round {} completed, round_count={} — crash gap, starting round {}",
                    event_id, n, round_count, next
                );
                start_round(db, network, firewall, publisher, event_id, Some(next)).await?;
                Ok(1)
            }
        }
    }
}

/// 查询 batch 中所有 task 的最大 deadline_at。
async fn max_task_deadline(
    db: &sea_orm::DatabaseConnection,
    batch_id: Uuid,
) -> AwdResult<chrono::DateTime<chrono::FixedOffset>> {
    let task = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::BatchId.eq(batch_id))
        .order_by_desc(awd_judge_tasks::Column::DeadlineAt)
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("No tasks in batch".into()))?;
    Ok(task.deadline_at)
}

/// 在事务内创建一次性 batch deadline 任务。
async fn schedule_batch_deadline_task<C: ConnectionTrait + Send>(
    txn: &C,
    event_id: Uuid,
    batch_id: Uuid,
    execute_at: chrono::DateTime<chrono::FixedOffset>,
) -> Result<(), sea_orm::DbErr> {
    let now = chrono::Utc::now();
    scheduled_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        group_id: Set(Some(event_id)),
        task_name: Set(format!("AWD Judge batch deadline {batch_id}")),
        description: Set(Some(
            "Judge batch absolute deadline — terminalize uncompleted tasks".into(),
        )),
        task_key: Set(TaskKey::AwdJudgeBatchDeadline.to_string()),
        trigger_type: Set("once".into()),
        status: Set("pending".into()),
        execute_at: Set(Some(execute_at)),
        payload: Set(Some(
            serde_json::json!({ "event_id": event_id, "batch_id": batch_id }),
        )),
        enabled: Set(true),
        protected: Set(true),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(txn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_round_trips_with_round_id() {
        let p = RoundTaskPayload {
            event_id: Uuid::new_v4(),
            round_id: Some(Uuid::new_v4()),
            round_number: Some(3),
        };
        let v = serde_json::to_value(&p).unwrap();
        let back: RoundTaskPayload = serde_json::from_value(v).unwrap();
        assert_eq!(back.event_id, p.event_id);
        assert_eq!(back.round_id, p.round_id);
        assert_eq!(back.round_number, Some(3));
    }

    #[test]
    fn payload_defaults_round_id_none() {
        let v = serde_json::json!({ "event_id": Uuid::new_v4() });
        let back: RoundTaskPayload = serde_json::from_value(v).unwrap();
        assert!(back.round_id.is_none());
        assert!(back.round_number.is_none());
    }
}