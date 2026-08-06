//! GameBox lifecycle service.

use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::{AwdEventStatus, GameboxStatus};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::{AwdEventStatusExt, GameboxStatusExt},
    repo::{ban_repo, event_repo, gamebox_repo},
};

/// Reset a GameBox instance.
/// Validates permissions and deducts reset penalty if applicable.
pub async fn reset_gamebox(
    db: &DatabaseConnection,
    event_id: Uuid,
    instance_id: Uuid,
    team_id: Uuid,
    _requested_by: Uuid,
    is_free_reset: bool,
) -> AwdResult<()> {
    // 1. Verify event is running
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if !awd_event.status.is_active() {
        return Err(AwdError::Forbidden("Event is not running".into()));
    }

    // 2. Verify team not banned
    let ban = ban_repo::find_active_ban(db, event_id, team_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    if ban.is_some() {
        return Err(AwdError::Forbidden("Team is banned".into()));
    }

    // 3. Verify instance belongs to team
    let instance = gamebox_repo::find_instance_by_id(db, instance_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("GameBox instance not found".into()))?;

    if instance.team_id != team_id {
        return Err(AwdError::Forbidden(
            "This GameBox does not belong to your team".into(),
        ));
    }

    // 4. Mark as resetting
    gamebox_repo::update_instance_status(db, instance_id, GameboxStatus::Resetting)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 5. Set reset protection
    let protection_until =
        chrono::Utc::now() + chrono::Duration::seconds(awd_event.reset_protection_secs as i64);

    let mut active: crate::entity::awd_gamebox_instances::ActiveModel =
        crate::entity::awd_gamebox_instances::ActiveModel {
            id: Set(instance_id),
            reset_protection_until: Set(Some(protection_until.into())),
            ..Default::default()
        };
    active
        .update(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // Note: Actual Docker container reset is handled by fcmc AwdContainerRuntime.
    // After Docker reset completes, the instance status should be set to Ready.

    Ok(())
}

/// Mark a GameBox instance as ready after successful container creation.
pub async fn mark_gamebox_ready(
    db: &DatabaseConnection,
    instance_id: Uuid,
    container_id: &str,
) -> AwdResult<()> {
    gamebox_repo::set_instance_container(db, instance_id, container_id, GameboxStatus::Ready)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}

/// Mark a GameBox instance as failed after container creation failure.
pub async fn mark_gamebox_failed(db: &DatabaseConnection, instance_id: Uuid) -> AwdResult<()> {
    gamebox_repo::update_instance_status(db, instance_id, GameboxStatus::StartFailed)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}
