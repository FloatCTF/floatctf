//! Event lifecycle service — AWD event CRUD, deployment, and phase management.

use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::{AwdEventStatus, AwdPhase, RoundStatus};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::AwdEventStatusExt,
    infrastructure::{
        firewall::FirewallRuntime,
        network::{AwdNetworkRuntime, EventNetworkIdentity},
    },
    repo::{event_repo, round_repo},
    service::firewall_service,
};

/// Start an AWD event: validate status, create first round, set hardening phase + policy.
pub async fn start_event(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
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
            "Cannot start event: precheck has not passed (no verified_at)".into(),
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

    // Create the first round
    let now = chrono::Utc::now();
    let round_end = now + chrono::Duration::seconds(awd_event.round_duration_secs as i64);

    round_repo::create_round(
        db,
        event_id,
        1, // round 1
        AwdPhase::Hardening,
        round_end,
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;

    // 全局 desired-state reconcile（nftables）+ conntrack 清理（Phase 1 P1-10）
    firewall_service::reconcile_global(db, firewall, firewall_service::next_network_revision(db).await?)
        .await?;
    firewall_service::flush_event_connections(network, event_id, &awd_event.gamebox_cidr).await;

    Ok(())
}

/// Pause an event: save remaining round time, set network to pause phase.
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

    firewall_service::reconcile_global(db, firewall, firewall_service::next_network_revision(db).await?)
        .await?;
    firewall_service::flush_event_connections(network, event_id, &awd_event.gamebox_cidr).await;

    Ok(())
}

/// Resume an event: restore round time, restore network phase.
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

    firewall_service::reconcile_global(db, firewall, firewall_service::next_network_revision(db).await?)
        .await?;
    firewall_service::flush_event_connections(network, event_id, &awd_event.gamebox_cidr).await;

    Ok(())
}

/// Finish an event: stop rounds, stop scoring, preserve data.
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
