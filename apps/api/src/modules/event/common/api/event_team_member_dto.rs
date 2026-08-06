use crate::entity::{event_team_members, sea_orm_active_enums::EventTeamMemberRole};
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EventTeamMembersDto {
    pub event_id: Uuid,
    pub team_id: Uuid,
    pub user_id: Uuid,
    pub role: EventTeamMemberRole,
    pub joined_at: DateTimeWithTimeZone,
}

impl From<event_team_members::Model> for EventTeamMembersDto {
    fn from(m: event_team_members::Model) -> Self {
        Self {
            event_id: m.event_id,
            team_id: m.team_id,
            user_id: m.user_id,
            role: m.role,
            joined_at: m.joined_at,
        }
    }
}
