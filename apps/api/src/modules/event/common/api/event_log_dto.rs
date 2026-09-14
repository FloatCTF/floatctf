use crate::entity::{
    event_logs,
    sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
};
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;
use serde_json::Value as Json;

#[derive(Debug, Serialize)]
pub struct EventLogsDto {
    pub id: Uuid,
    pub event_id: Uuid,
    pub user_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub family: EventFamily,
    pub purpose: EventPurpose,
    pub participant_mode: ParticipantMode,
    pub level: String,
    pub action: String,
    pub details: Json,
    pub created_at: DateTimeWithTimeZone,
    pub ip_address: Option<String>,
}

impl From<event_logs::Model> for EventLogsDto {
    fn from(m: event_logs::Model) -> Self {
        Self {
            id: m.id,
            event_id: m.event_id,
            user_id: m.user_id,
            team_id: m.team_id,
            family: m.family,
            purpose: m.purpose,
            participant_mode: m.participant_mode,
            level: m.level,
            action: m.action,
            details: m.details,
            created_at: m.created_at,
            ip_address: m.ip_address,
        }
    }
}
