//! Event repository — centralized database queries for event operations.
//!
//! Extracted from scattered handler-level queries to provide reusable
//! database access patterns. Handlers should call these functions
//! instead of writing their own SeaORM queries.

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{event_team_members, event_teams};

/// Team membership information for a user in an event.
#[derive(Debug, Clone)]
pub struct TeamMembership {
    pub team_id: Uuid,
    pub role: String,
}

/// Find user's team membership for a specific event.
///
/// Returns `Some(TeamMembership)` if the user is a member of a team
/// in the given event, `None` otherwise.
pub async fn find_user_team_membership(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> Result<Option<TeamMembership>, sea_orm::DbErr> {
    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await?;

    Ok(membership.map(|m| TeamMembership {
        team_id: m.team_id,
        role: format!("{:?}", m.role),
    }))
}

/// Find team by event and name.
pub async fn find_team_by_name(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_name: &str,
) -> Result<Option<event_teams::Model>, sea_orm::DbErr> {
    event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .filter(event_teams::Column::Name.eq(team_name))
        .one(db)
        .await
}

/// Check if a team name is already taken in an event.
pub async fn is_team_name_taken(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_name: &str,
) -> Result<bool, sea_orm::DbErr> {
    let exists = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .filter(event_teams::Column::Name.eq(team_name))
        .one(db)
        .await?;
    Ok(exists.is_some())
}

/// Count members in a team for an event.
pub async fn count_team_members(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<i64, sea_orm::DbErr> {
    use sea_orm::{PaginatorTrait, QuerySelect};
    let count = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::TeamId.eq(team_id))
        .count(db)
        .await?;
    Ok(count as i64)
}
