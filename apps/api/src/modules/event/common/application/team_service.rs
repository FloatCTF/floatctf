//! Team membership service — centralized team operations.
//!
//! Extracted from scattered handler queries to provide consistent
//! team membership checks across admin and player handlers.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::entity::event_team_members;
use crate::modules::event::awd::{AwdError, AwdResult};

/// Find user's team for an event.
pub async fn find_user_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> AwdResult<Option<Uuid>> {
    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    Ok(membership.map(|m| m.team_id))
}

/// Require user to be a member of a team in the event.
/// Returns team_id or error.
pub async fn require_member(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> AwdResult<Uuid> {
    find_user_team(db, event_id, user_id)
        .await?
        .ok_or_else(|| AwdError::NotFound("You are not in a team for this event".into()))
}

/// Check if user is a captain of their team.
pub async fn is_captain(db: &DatabaseConnection, event_id: Uuid, user_id: Uuid) -> AwdResult<bool> {
    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    Ok(membership
        .map(|m| format!("{:?}", m.role) == "Captain")
        .unwrap_or(false))
}

/// Join a team (add user to team).
pub async fn join_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
) -> AwdResult<()> {
    // Check if already in a team
    let existing = find_user_team(db, event_id, user_id).await?;
    if existing.is_some() {
        return Err(AwdError::Conflict(
            "Already in a team for this event".into(),
        ));
    }

    let model = event_team_members::ActiveModel {
        event_id: Set(event_id),
        team_id: Set(team_id),
        user_id: Set(user_id),
        role: Set(crate::entity::sea_orm_active_enums::EventTeamMemberRole::Member),
        ..Default::default()
    };

    model
        .insert(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}

/// Leave a team (remove user from team).
pub async fn leave_team(db: &DatabaseConnection, event_id: Uuid, user_id: Uuid) -> AwdResult<()> {
    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    match membership {
        Some(membership) => {
            event_team_members::Entity::delete_by_id((
                membership.event_id,
                membership.team_id,
                membership.user_id,
            ))
            .exec(db)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
            Ok(())
        }
        None => Err(AwdError::NotFound("Not in a team for this event".into())),
    }
}
