//! AWD 赛事配置/生命周期服务（Wave 2）。
//!
//! Start → Hardening (if duration > 0) → Attack → Round 1..N → Final Settlement

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    TransactionTrait,
};
use tracing::info;
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::{AwdEventStatus, AwdPhase, RoundStatus, ScoreEventType};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    domain::{AwdEventStatusExt, IdempotencyKey, timing::compute_timing},
    infrastructure::{
        firewall::FirewallRuntime,
        network::{AwdNetworkRuntime, EventNetworkIdentity},
    },
    repo::{event_repo, round_repo, score_repo},
    scheduler,
    service::{firewall_service, round_service},
};

/// 启动 AWD 赛事：校验 Precheck、配置代数、计算时间模型。
///
/// 若 hardening_duration > 0：进入 Running/Hardening，设置 hardening_ends_at，
/// 调度 AwdHardeningEnd。不创建 Round 1。
///
/// 若 hardening_duration == 0：直接进入 Running/Attack，立即启动 Round 1。
pub async fn start_event(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn crate::infrastructure::realtime::EventPublisher,
    event_id: Uuid,
) -> AwdResult<()> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    // Validate: must be verified
    if awd_event.status != AwdEventStatus::Verified {
        return Err(AwdError::InvalidState(format!(
            "Cannot start event in {:?} status. Must be verified.",
            awd_event.status
        )));
    }

    // Validate: must have verified_at set
    if awd_event.verified_at.is_none() {
        return Err(AwdError::InvalidState(
            "Cannot start event: precheck has not passed (AWD_NOT_VERIFIED)".into(),
        ));
    }

    // P2-11 Start Gate：配置代数必须匹配
    if awd_event
        .verified_generation
        .map(|g| g != awd_event.configuration_generation)
        .unwrap_or(true)
    {
        let _ = event_repo::transition_event(
            db,
            awd_event.id,
            AwdEventStatus::Verified,
            AwdEventStatus::StartBlocked,
            event_repo::TransitionPatch::config_changed(),
        )
        .await;
        return Err(AwdError::InvalidState(
            "Cannot start event: configuration changed since verification (AWD_CONFIG_CHANGED)"
                .into(),
        ));
    }

    // ── 计算时间模型 ──
    let generic_event = event_repo::find_generic_event_by_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("Generic event not found".into()))?;

    let timing = compute_timing(
        generic_event.start_time,
        generic_event.end_time,
        awd_event.round_count,
        awd_event.round_duration_secs,
    )
    .map_err(|e| AwdError::InvalidState(format!("Invalid AWD timing: {e}")))?;

    let now = chrono::Utc::now();

    // ── 种子初始化分数（§11: 幂等，每 Team 一次） ──
    seed_initial_scores(db, event_id, awd_event.initial_score).await?;

    if timing.hardening_duration_secs > 0 {
        // ── Hardening > 0：进入 Hardening 阶段 ──
        let hardening_ends_at = now + chrono::Duration::seconds(timing.hardening_duration_secs);

        event_repo::transition_event(
            db,
            awd_event.id,
            AwdEventStatus::Verified,
            AwdEventStatus::Running,
            event_repo::TransitionPatch {
                phase: Some(AwdPhase::Hardening),
                started_at: Some(now),
                hardening_ends_at: Some(Some(hardening_ends_at.into())),
                ..Default::default()
            },
        )
        .await?;

        // 调度 HardeningEnd
        let txn = db
            .begin()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        scheduler::schedule_hardening_end(&txn, event_id, hardening_ends_at.fixed_offset())
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        txn.commit()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;

        info!(
            "[Event] Event {} started in Hardening phase (hardening_ends_at={})",
            event_id, hardening_ends_at
        );

        // 应用 Hardening 网络策略
        let revision = firewall_service::next_network_revision(db).await?;
        let _ = firewall_service::reconcile_global(db, firewall, revision).await;
    } else {
        // ── Hardening = 0：直接进入 Attack，启动 Round 1 ──
        event_repo::transition_event(
            db,
            awd_event.id,
            AwdEventStatus::Verified,
            AwdEventStatus::Running,
            event_repo::TransitionPatch {
                phase: Some(AwdPhase::Attack),
                started_at: Some(now),
                ..Default::default()
            },
        )
        .await?;

        info!(
            "[Event] Event {} started directly in Attack phase (hardening=0)",
            event_id
        );

        // 应用 Attack 网络策略 + 启动 Round 1
        let revision = firewall_service::next_network_revision(db).await?;
        let _ = firewall_service::reconcile_global(db, firewall, revision).await;

        round_service::start_round(db, network, firewall, publisher, event_id, Some(1)).await?;
    }

    Ok(())
}

/// 为所有参赛队伍创建 InitialScore 账本事件（§11）。
///
/// 幂等：每个 Event × Team 恰好一条 InitialScore，通过
/// `initial-score:{event_id}:{team_id}` 幂等键确保。
/// 即使 `initial_score == 0` 也写入账本以保留审计轨迹。
async fn seed_initial_scores(
    db: &DatabaseConnection,
    event_id: Uuid,
    initial_score: i64,
) -> AwdResult<()> {
    let teams = crate::entity::event_teams::Entity::find()
        .filter(crate::entity::event_teams::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    for team in &teams {
        let key = IdempotencyKey::initial_score(&event_id.to_string(), &team.id.to_string());
        let _ = score_repo::create_score_event_if_absent(
            db,
            event_id,
            None, // not tied to a round
            team.id,
            ScoreEventType::InitialScore,
            initial_score,
            &key,
            None,
            None,
            None,
            Some("initial score"),
        )
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    info!(
        "[Event] InitialScore seeded for {} teams in event {} (delta={})",
        teams.len(),
        event_id,
        initial_score
    );
    Ok(())
}

/// 暂停赛事：保存当前阶段剩余时间，取消定时任务，网络进入 pause 阶段。
pub async fn pause_event(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    event_id: Uuid,
) -> AwdResult<()> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if awd_event.status != AwdEventStatus::Running {
        return Err(AwdError::InvalidState(
            "Can only pause a running event".into(),
        ));
    }

    let now = chrono::Utc::now();
    let remaining: i32;

    match awd_event.phase {
        AwdPhase::Hardening => {
            // 计算剩余 Hardening 时间
            remaining = awd_event
                .hardening_ends_at
                .map(|h| (h.with_timezone(&chrono::Utc) - now).num_seconds().max(0) as i32)
                .unwrap_or(0);

            // 取消 pending HardeningEnd 任务
            let _ = scheduler::cancel_pending_hardening_end(db, event_id).await;
        }
        AwdPhase::Attack => {
            // 暂停活跃轮次
            if let Some(round) = round_repo::find_active_round(db, event_id)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?
            {
                remaining = (round.scheduled_end_at.with_timezone(&chrono::Utc) - now)
                    .num_seconds()
                    .max(0) as i32;

                round_repo::pause_round(db, round.id, remaining)
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
            } else {
                remaining = 0;
            }
        }
        _ => {
            remaining = 0;
        }
    }

    // 状态 + phase + paused_phase + pause_remaining_secs 同事务原子写入
    let mut patch = event_repo::TransitionPatch::paused(awd_event.phase, remaining);
    // 清除 hardening_ends_at（暂停时不再需要）
    patch.hardening_ends_at = Some(None);

    event_repo::transition_event(
        db,
        awd_event.id,
        AwdEventStatus::Running,
        AwdEventStatus::Paused,
        patch,
    )
    .await?;

    match firewall_service::reconcile_global(
        db,
        firewall,
        firewall_service::next_network_revision(db).await?,
    )
    .await
    {
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
            Ok(())
        }
        Err(e) => {
            let _ = event_repo::transition_event(
                db,
                awd_event.id,
                AwdEventStatus::Paused,
                AwdEventStatus::NetworkError,
                Default::default(),
            )
            .await;
            Err(AwdError::Network(format!(
                "pause network reconcile failed: {e}"
            )))
        }
    }
}

/// 恢复赛事：还原阶段时间与网络阶段。
pub async fn resume_event(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    publisher: &dyn crate::infrastructure::realtime::EventPublisher,
    event_id: Uuid,
) -> AwdResult<()> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if awd_event.status != AwdEventStatus::Paused
        && awd_event.status != AwdEventStatus::NetworkError
    {
        return Err(AwdError::InvalidState(
            "Can only resume a paused or network-error event".into(),
        ));
    }

    let resume_phase = awd_event.paused_phase.unwrap_or(AwdPhase::Hardening);
    let remaining = awd_event.pause_remaining_secs.unwrap_or(0);
    let now = chrono::Utc::now();

    match resume_phase {
        AwdPhase::Hardening => {
            // 恢复 Hardening
            let hardening_ends_at = if remaining > 0 {
                let deadline = now + chrono::Duration::seconds(remaining as i64);
                Some(deadline)
            } else {
                None
            };

            event_repo::transition_event(
                db,
                awd_event.id,
                awd_event.status.clone(),
                AwdEventStatus::Running,
                event_repo::TransitionPatch {
                    phase: Some(AwdPhase::Hardening),
                    pause_remaining_secs: Some(0),
                    hardening_ends_at: Some(hardening_ends_at.map(|h| h.into())),
                    ..Default::default()
                },
            )
            .await?;

            // 调度新的 HardeningEnd
            if let Some(deadline) = hardening_ends_at {
                let txn = db
                    .begin()
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                scheduler::schedule_hardening_end(&txn, event_id, deadline.fixed_offset())
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                txn.commit()
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
            } else {
                // 剩余时间为 0 → 直接进入 Attack
                info!(
                    "[Resume] Hardening remaining=0 for event {} → transitioning to Attack",
                    event_id
                );
                // 直接调用 handle_hardening_end 等效逻辑
                let txn = db
                    .begin()
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                event_repo::update_phase(&txn, awd_event.id, AwdPhase::Attack)
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                // 清除 hardening_ends_at
                use crate::entity::awd_events;
                awd_events::ActiveModel {
                    id: Set(awd_event.id),
                    hardening_ends_at: Set(None),
                    updated_at: Set(chrono::Utc::now().into()),
                    ..Default::default()
                }
                .update(&txn)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
                txn.commit()
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
            }
        }
        AwdPhase::Attack => {
            // 恢复活跃轮次
            if let Some(round) = round_repo::find_active_round(db, event_id)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?
            {
                let round_remaining = round
                    .remaining_secs
                    .unwrap_or(awd_event.round_duration_secs);
                let new_end = now + chrono::Duration::seconds(round_remaining as i64);

                let mut active: crate::entity::awd_rounds::ActiveModel =
                    crate::entity::awd_rounds::ActiveModel {
                        id: Set(round.id),
                        status: Set(RoundStatus::Active),
                        scheduled_end_at: Set(new_end.into()),
                        remaining_secs: Set(None),
                        paused_at: Set(None),
                        ..Default::default()
                    };
                active
                    .update(db)
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
            }

            event_repo::transition_event(
                db,
                awd_event.id,
                awd_event.status.clone(),
                AwdEventStatus::Running,
                event_repo::TransitionPatch {
                    phase: Some(AwdPhase::Attack),
                    pause_remaining_secs: Some(0),
                    ..Default::default()
                },
            )
            .await?;
        }
        _ => {
            return Err(AwdError::InvalidState(format!(
                "Cannot resume from phase {:?}",
                resume_phase
            )));
        }
    }

    // 网络 reconcile
    match firewall_service::reconcile_global(
        db,
        firewall,
        firewall_service::next_network_revision(db).await?,
    )
    .await
    {
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

            // 恢复轮次调度任务
            if resume_phase == AwdPhase::Attack {
                let restored = round_service::restore_round_scheduling(
                    db, event_id, network, firewall, publisher,
                )
                .await?;
                if restored > 0 {
                    info!("[Resume] event {event_id}: rebuilt {restored} round scheduling task(s)");
                }
            }
            Ok(())
        }
        Err(e) => {
            let _ = event_repo::transition_event(
                db,
                awd_event.id,
                AwdEventStatus::Running,
                AwdEventStatus::NetworkError,
                Default::default(),
            )
            .await;
            Err(AwdError::Network(format!(
                "resume network reconcile failed: {e}"
            )))
        }
    }
}

/// 结束赛事：停止轮次与计分，保留数据。
///
/// Wave 2 临时行为：直接完成活跃轮次并进入 Finished。
/// Wave 6 将替换为 Final Settlement 自动完成。
pub async fn finish_event(db: &DatabaseConnection, event_id: Uuid) -> AwdResult<()> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if !awd_event.status.is_active() {
        return Err(AwdError::InvalidState(
            "Can only finish a running or paused event".into(),
        ));
    }

    // Complete the active round if any
    if let Some(round) = round_repo::find_active_round(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
    {
        round_repo::update_round_status(db, round.id, RoundStatus::Completed)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    // 清除 hardening_ends_at（如果仍在 Hardening）
    use crate::entity::awd_events;
    awd_events::ActiveModel {
        id: Set(awd_event.id),
        hardening_ends_at: Set(None),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .update(db)
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;

    event_repo::transition_event(
        db,
        awd_event.id,
        awd_event.status.clone(),
        AwdEventStatus::Finished,
        event_repo::TransitionPatch::finished(),
    )
    .await?;

    Ok(())
}
