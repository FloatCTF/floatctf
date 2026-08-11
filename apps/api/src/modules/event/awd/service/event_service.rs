//! AWD 赛事配置/生命周期服务。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use tracing::info;
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::{AwdEventStatus, AwdPhase, RoundStatus};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    domain::AwdEventStatusExt,
    infrastructure::{
        firewall::FirewallRuntime,
        network::{AwdNetworkRuntime, EventNetworkIdentity},
    },
    repo::{event_repo, round_repo},
    service::{firewall_service, round_service},
};

/// 启动an AWD event: validate status, create first round, set hardening phase + policy。
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

    // P2-11 Start Gate：配置代数必须匹配（配置在验证后变更 → StartBlocked）
    if awd_event
        .verified_generation
        .map(|g| g != awd_event.configuration_generation)
        .unwrap_or(true)
    {
        // 进入 StartBlocked（状态机合法路径）
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

    // Start with hardening phase + started_at，经状态机唯一入口（Phase 0）
    event_repo::transition_event(
        db,
        awd_event.id,
        AwdEventStatus::Verified,
        AwdEventStatus::Running,
        event_repo::TransitionPatch {
            phase: Some(AwdPhase::Hardening),
            started_at: Some(chrono::Utc::now()),
            ..Default::default()
        },
    )
    .await?;

    // P3-1：第一轮经 round_service 创建（幂等 find-or-create + 插入 RoundEnd(1) 任务
    // + COMMIT 后 reconcile + conntrack + judge dispatch）。
    round_service::start_round(db, network, firewall, publisher, event_id, Some(1)).await?;

    Ok(())
}

/// 暂停赛事：保存轮次剩余时间，网络进入 pause 阶段。
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
    let remaining = if let Some(round) = round_repo::find_active_round(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
    {
        let remaining = (round.scheduled_end_at.with_timezone(&chrono::Utc) - now)
            .num_seconds()
            .max(0) as i32;

        round_repo::pause_round(db, round.id, remaining)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        remaining
    } else {
        0
    };

    // 状态 + phase + paused_phase + pause_remaining_secs 同事务原子写入（Phase 0）
    event_repo::transition_event(
        db,
        awd_event.id,
        AwdEventStatus::Running,
        AwdEventStatus::Paused,
        event_repo::TransitionPatch::paused(awd_event.phase, remaining),
    )
    .await?;

    // P4-10：pause 网络应用失败 → Fail Closed（NetworkError），不留"Paused 但网络没生效"
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

/// 恢复赛事：还原轮次时间与网络阶段。
pub async fn resume_event(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    event_id: Uuid,
) -> AwdResult<()> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if awd_event.status != AwdEventStatus::Paused {
        return Err(AwdError::InvalidState(
            "Can only resume a paused event".into(),
        ));
    }

    // Resume the paused round
    if let Some(round) = round_repo::find_active_round(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
    {
        let remaining = round
            .remaining_secs
            .unwrap_or(awd_event.round_duration_secs);
        let new_end = chrono::Utc::now() + chrono::Duration::seconds(remaining as i64);

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

    // Restore the pre-pause phase（paused_phase，Phase 0 P0-1b 迁移），缺省回退 Hardening 而非 Attack
    let resume_phase = awd_event.paused_phase.unwrap_or(AwdPhase::Hardening);

    // 状态 + phase 同事务原子写入（Phase 0）
    event_repo::transition_event(
        db,
        awd_event.id,
        AwdEventStatus::Paused,
        AwdEventStatus::Running,
        event_repo::TransitionPatch {
            phase: Some(resume_phase.clone()),
            pause_remaining_secs: Some(0),
            ..Default::default()
        },
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

            // P4-9 修复：暂停期间原 round.end/grace_end 任务已被消费（触发时 round 非
            // Active/Grace 而幂等跳过）；resume 后必须按新 deadline 重建，否则比赛卡死
            let restored = round_service::restore_round_scheduling(db, event_id).await?;
            if restored > 0 {
                info!("[Resume] event {event_id}: rebuilt {restored} round scheduling task(s)");
            }
            Ok(())
        }
        Err(e) => {
            // P4-10：resume 网络应用失败 → NetworkError（Running→NetworkError 合法）
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

    // Complete the active round
    if let Some(round) = round_repo::find_active_round(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
    {
        round_repo::update_round_status(db, round.id, RoundStatus::Completed)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    // 状态 + finished_at 同事务原子写入（Phase 0）
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
