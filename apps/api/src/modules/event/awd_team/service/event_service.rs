//! Event lifecycle service — AWD event CRUD, deployment, and phase management.

use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::{AwdEventStatus, AwdPhase, RoundStatus};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::AwdEventStatusExt,
    infrastructure::network::AwdNetworkRuntime,
    repo::{event_repo, round_repo},
    service::network_policy_service,
};

/// Start an AWD event: validate status, create first round, set hardening phase + policy.
pub async fn start_event(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
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

    // Start with hardening phase
    event_repo::update_phase(db, awd_event.id, AwdPhase::Hardening)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    event_repo::update_status(db, awd_event.id, AwdEventStatus::Running)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

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

    network_policy_service::apply_phase_policy(db, network, event_id, AwdPhase::Hardening).await?;

    Ok(())
}

/// Pause an event: save remaining round time, set network to pause phase.
pub async fn pause_event(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
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

    // Pause the active round
    if let Some(round) = round_repo::find_active_round(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
    {
        let now = chrono::Utc::now();
        let remaining = (round.scheduled_end_at.with_timezone(&chrono::Utc) - now)
            .num_seconds()
            .max(0) as i32;

        round_repo::pause_round(db, round.id, remaining)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    event_repo::update_status(db, awd_event.id, AwdEventStatus::Paused)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    event_repo::update_phase(db, awd_event.id, AwdPhase::Pause)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    network_policy_service::apply_phase_policy(db, network, event_id, AwdPhase::Pause).await?;

    Ok(())
}

/// Resume an event: restore round time, restore network phase.
pub async fn resume_event(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
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

    // Restore appropriate phase based on round phase or default to attack
    event_repo::update_phase(db, awd_event.id, AwdPhase::Attack)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    event_repo::update_status(db, awd_event.id, AwdEventStatus::Running)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    network_policy_service::apply_phase_policy(db, network, event_id, AwdPhase::Attack).await?;

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

    event_repo::update_status(db, awd_event.id, AwdEventStatus::Finished)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    Ok(())
}
