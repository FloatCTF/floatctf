//! Round lifecycle service (Phase 3 P3-1..P3-6).
//!
//! Round 调度闭环：
//!
//! ```text
//! RoundStart(N) → 事务(lock event → 检查 Running → find-or-create round(N)
//!                 → 更新 event phase → 插入 RoundEnd(N) 任务) → COMMIT
//!                 → firewall reconcile（DB desired phase → 全局 nftables）
//!                 → conntrack flush → dispatch judge → publish
//! RoundEnd(N)   → 事务(lock round → Grace + grace_ends_at → 插入 GraceEnd(N)) → COMMIT
//! GraceEnd(N)   → 事务(lock round → Completed + completed_at)
//!                 → 若 event 仍 Running → 插入 RoundStart(N+1)
//! ```
//!
//! 设计约束（chore/plans/awd/03-phase3-core-loop.md §5.1/§5.4）：
//! - 外部副作用（nft reconcile / conntrack / judge dispatch）**不进长 DB 事务**；
//! - round 任务幂等：`find-or-create` + 状态机守卫，scheduler retry 不产生重复 round；
//! - phase 切换唯一路径 = DB desired phase → 全局 DesiredFirewallState → reconcile。

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
        AwdEventStatus, AwdPhase, JudgeTaskStatus, RoundStatus, ScoreEventType,
    },
};
use crate::infrastructure::realtime::EventPublisher;
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::AwdEventStatusExt,
    infrastructure::{firewall::FirewallRuntime, network::AwdNetworkRuntime},
    repo::{event_repo, judge_repo, round_repo, score_repo},
    service::{firewall_service, judge_service},
    websocket,
};
use crate::scheduler::TaskKey;

/// Round 任务 payload：end/grace_end 任务带 round_id（幂等键维度）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundTaskPayload {
    pub event_id: Uuid,
    pub round_id: Option<Uuid>,
    /// RoundStart 任务携带期望 round_number（幂等键维度，P3-3 防 retry 双 round）。
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

/// 在事务内创建一次性 round 任务（end/grace_end/start）。
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

/// 开始一轮（P3-1/P3-2/P3-3/P3-6）。
///
/// 幂等：同 (event_id, round_number) 已存在 → 返回既有 round（retry 安全）。
pub async fn start_round(
    db: &sea_orm::DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn EventPublisher,
    event_id: Uuid,
    expected_round_number: Option<i32>,
) -> AwdResult<RoundStarted> {
    // ── 事务内：DB 状态变更 ──
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

    // 上一轮残留清理（崩溃恢复场景：active 轮次未 Completed）
    if let Some(prev) = round_repo::find_active_round(&txn, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
    {
        info!(
            "[Round] Completing leftover active round {} for event {}",
            prev.round_number, event_id
        );
        let timed_out = judge_repo::timeout_pending_tasks(&txn, prev.id)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        if timed_out > 0 {
            warn!("[Round] {timed_out} pending judge tasks timed out");
            // P3-5：deadline 超时计分（在事务外，读已提交的 JudgeTimeout 状态）
            score_judge_timeouts(db, prev.id).await?;
        }
        round_repo::update_round_status(&txn, prev.id, RoundStatus::Completed)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    // round number：任务携带期望值则用之（幂等键维度）；否则 latest + 1（或 1）
    let round_number = match expected_round_number {
        Some(n) if n >= 1 => n,
        _ => {
            let latest = round_repo::find_latest_round(&txn, event_id)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
            latest.map(|r| r.round_number + 1).unwrap_or(1)
        }
    };

    // 幂等：同 round_number 已存在 → 直接返回（retry 不重复创建）
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

    // phase：round 1 = Hardening，后续 = Attack
    let phase = if round_number == 1 {
        AwdPhase::Hardening
    } else {
        AwdPhase::Attack
    };

    // 更新 event phase（守卫 repo 方法：Hardening↔Attack 合法）
    event_repo::update_phase(&txn, awd_event.id, phase.clone())
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

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

    // ── COMMIT 后：网络副作用（横切规则 3）──
    // phase 切换 = DB desired → 全局 DesiredFirewallState → nftables reconcile（P3-6）
    let revision = firewall_service::next_network_revision(db).await?;
    match firewall_service::reconcile_global(db, firewall, revision).await {
        Ok(_) => {
            firewall_service::flush_event_connections(network, event_id, &awd_event.gamebox_cidr)
                .await;
            // P3-7：DB commit 后发布；publish 失败不回滚业务（best-effort）
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
            // P3-6 Fail Closed：reconcile 失败 → network.policy.failed + 返回错误（调用方置 NetworkError）
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

    // judge batch 分发（P3-2：非致命——round 已提交，judge 分发可重试，
    // 失败只记录，不让轮次启动因网络抖动回滚）
    if let Err(e) = dispatch_judge_for_round(db, &awd_event, round.id, event_id).await {
        warn!(
            "[Round] judge dispatch failed for round {} event {}: {}",
            round.id, event_id, e
        );
    }

    Ok(RoundStarted {
        round_id: round.id,
        round_number: round.round_number,
        phase,
        created: true,
    })
}

/// RoundEnd：进入 Grace（P3-4）。
///
/// 幂等：非 Active 状态（已 Grace/Completed）→ 直接成功跳过。
pub async fn end_round(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
) -> AwdResult<()> {
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
            "[Round] Round {} already {:?} — skip grace",
            round_id, round.status
        );
        txn.rollback()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        return Ok(());
    }

    // 超时未回 judge 任务（deadline 强制，P3-5）
    let timed_out = judge_repo::timeout_pending_tasks(&txn, round_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    if timed_out > 0 {
        warn!("[Round] Round {round_id} end: {timed_out} pending judge tasks timed out");
    }

    // grace 周期
    let awd_event = event_repo::find_by_event_id(&txn, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;
    let grace_ends_at =
        chrono::Utc::now() + chrono::Duration::seconds(awd_event.judge_grace_period_secs as i64);

    let mut active: awd_rounds::ActiveModel = awd_rounds::ActiveModel {
        id: Set(round_id),
        status: Set(RoundStatus::Grace),
        grace_ends_at: Set(Some(grace_ends_at.into())),
        ..Default::default()
    };
    active
        .update(&txn)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 插入 GraceEnd 任务
    schedule_round_task(
        &txn,
        event_id,
        TaskKey::AwdRoundGraceEnd,
        grace_ends_at.fixed_offset(),
        Some(round_id),
        None,
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;

    txn.commit()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    info!("[Round] Round {round_id} entered grace until {grace_ends_at}");
    Ok(())
}

/// GraceEnd：Complete 当前轮，若赛事仍 Running 则调度下一轮（N+1，P3-1）。
///
/// 幂等：非 Grace 状态 → 跳过。
pub async fn grace_end_round(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
    publisher: &dyn EventPublisher,
) -> AwdResult<()> {
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

    if round.status != RoundStatus::Grace {
        warn!(
            "[Round] Round {round_id} not in Grace ({:?}) — skip",
            round.status
        );
        txn.rollback()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        return Ok(());
    }

    round_repo::update_round_status(&txn, round_id, RoundStatus::Completed)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 若赛事仍 Running → RoundStart(N+1)
    let awd_event = event_repo::find_by_event_id(&txn, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;
    if awd_event.status == AwdEventStatus::Running {
        schedule_round_task(
            &txn,
            event_id,
            TaskKey::AwdRoundStart,
            chrono::Utc::now().fixed_offset(),
            None,
            Some(round.round_number + 1),
        )
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    txn.commit()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let completed_round_number = round.round_number;
    info!("[Round] Round {round_id} completed for event {event_id}");
    // P3-7：DB commit 后发布（best-effort）
    let _ = publisher
        .publish(websocket::round_completed(event_id, completed_round_number).into_realtime())
        .await;
    Ok(())
}

/// 分发 judge batch（沿用原 handler 逻辑）。
async fn dispatch_judge_for_round(
    db: &sea_orm::DatabaseConnection,
    awd_event: &awd_events::Model,
    round_id: Uuid,
    event_id: Uuid,
) -> AwdResult<()> {
    let token_ciphertext = awd_event
        .judgeserver_token_ciphertext
        .clone()
        .ok_or_else(|| AwdError::NotFound("JudgeServer token not configured".into()))?;
    let token_nonce = awd_event
        .judgeserver_token_nonce
        .clone()
        .ok_or_else(|| AwdError::NotFound("JudgeServer token nonce not configured".into()))?;
    let crypto = crate::modules::event::awd_team::crypto::AwdCrypto::from_config_secret()
        .map_err(|e| AwdError::Crypto(e.to_string()))?;
    let token = crypto
        .decrypt(
            &crate::modules::event::awd_team::crypto::EncryptedBlob {
                ciphertext: token_ciphertext,
                nonce: token_nonce,
                key_version: awd_event.key_version,
            },
            &crate::modules::event::awd_team::crypto::AwdCrypto::build_aad(
                event_id,
                "internal_token",
            ),
        )
        .map_err(|e| AwdError::Crypto(e.to_string()))?;
    let token = String::from_utf8(token).map_err(|_| AwdError::Crypto("token not utf8".into()))?;

    let batch_id = judge_service::create_batch(db, event_id, round_id).await?;
    let judgeserver_url = format!("http://{}:8082", awd_event.judgeserver_ip);
    judge_service::dispatch_batch(db, batch_id, &judgeserver_url, &token).await?;
    Ok(())
}

/// P3-5 deadline 强制：给指定 round 中超时（deadline 未回）的 judge 任务计分。
///
/// 语义：超时 = 防御方未响应 = 视为 down（-down_points）。
/// score_event_type 枚举无 JudgeTimeout 变体（避免 enum 迁移），
/// 用 JudgeDown + reason="judge deadline timeout" 表达，审计可区分。
/// 幂等键 `judge-timeout:{task_id}`（scheduler retry 不重复计分）。
/// 返回计分的任务数。
/// P3-13：重启后恢复当前 round 的调度任务（幂等）。
///
/// 场景：历史版本创建的 round 没有 RoundEnd 任务，或崩溃发生在任务窗口内。
/// 恢复规则：
/// - Active round：缺 pending `awd.round.end` 任务 → 按 scheduled_end_at 重建；
/// - Grace round：缺 pending `awd.round.grace_end` 任务 → 按 grace_ends_at 重建；
/// - Paused round：暂停中，不恢复 end/grace 任务（resume 时重建）。
pub async fn restore_round_scheduling(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
) -> AwdResult<usize> {
    use crate::entity::sea_orm_active_enums::RoundStatus as RS;

    let Some(round) = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .filter(awd_rounds::Column::Status.is_in([RS::Active, RS::Grace, RS::Paused]))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
    else {
        return Ok(0);
    };

    let mut restored = 0usize;
    let task_exists = |key: TaskKey| -> std::pin::Pin<
        Box<dyn std::future::Future<Output = AwdResult<bool>> + Send + '_>,
    > {
        Box::pin(async move {
            let found = scheduled_tasks::Entity::find()
                .filter(scheduled_tasks::Column::GroupId.eq(event_id))
                .filter(scheduled_tasks::Column::TaskKey.eq(key.to_string()))
                .filter(scheduled_tasks::Column::Status.eq("pending"))
                .one(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
            Ok(found.is_some())
        })
    };

    match round.status {
        RS::Active => {
            if !task_exists(TaskKey::AwdRoundEnd).await? {
                let txn = db
                    .begin()
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                schedule_round_task(
                    &txn,
                    event_id,
                    TaskKey::AwdRoundEnd,
                    round.scheduled_end_at.fixed_offset(),
                    Some(round.id),
                    None,
                )
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
                txn.commit()
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                restored += 1;
            }
        }
        RS::Grace => {
            if let Some(grace_end) = round.grace_ends_at {
                if !task_exists(TaskKey::AwdRoundGraceEnd).await? {
                    let txn = db
                        .begin()
                        .await
                        .map_err(|e| AwdError::Database(e.to_string()))?;
                    schedule_round_task(
                        &txn,
                        event_id,
                        TaskKey::AwdRoundGraceEnd,
                        grace_end.fixed_offset(),
                        Some(round.id),
                        None,
                    )
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                    txn.commit()
                        .await
                        .map_err(|e| AwdError::Database(e.to_string()))?;
                    restored += 1;
                }
            }
        }
        RS::Paused => {
            // 暂停中：任务挂起由 resume 重建
        }
        _ => {}
    }
    Ok(restored)
}

pub async fn score_judge_timeouts(
    db: &sea_orm::DatabaseConnection,
    round_id: Uuid,
) -> AwdResult<u64> {
    let timed_out = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::RoundId.eq(round_id))
        .filter(awd_judge_tasks::Column::Status.eq(JudgeTaskStatus::JudgeTimeout))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 收集涉及到的 template 分值
    use crate::entity::awd_gamebox_templates;
    let mut template_points: std::collections::HashMap<Uuid, i64> =
        std::collections::HashMap::new();
    for t in &timed_out {
        if !template_points.contains_key(&t.template_id) {
            if let Some(tpl) = awd_gamebox_templates::Entity::find_by_id(t.template_id)
                .one(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?
            {
                template_points.insert(t.template_id, tpl.down_points);
            }
        }
    }

    let mut scored = 0u64;
    for t in &timed_out {
        let delta = -template_points.get(&t.template_id).copied().unwrap_or(0);
        let key = format!("judge-timeout:{}", t.id);
        match score_repo::create_score_event(
            db,
            t.event_id,
            Some(round_id),
            t.team_id,
            ScoreEventType::JudgeDown,
            delta,
            &key,
            None,
            Some(t.gamebox_instance_id),
            Some(t.template_id),
            Some("judge deadline timeout"),
        )
        .await
        {
            Ok(_) => scored += 1,
            Err(e) => {
                // 幂等重复（scheduler retry）不视为错误
                let msg = e.to_string().to_lowercase();
                if !(msg.contains("23505") || msg.contains("duplicate")) {
                    return Err(AwdError::Database(e.to_string()));
                }
            }
        }
    }
    Ok(scored)
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
