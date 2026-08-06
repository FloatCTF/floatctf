use crate::entity::event_teams;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EventTeamsDto {
    pub id: Uuid,
    pub event_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub points: f64,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub banned: bool,
}

impl From<event_teams::Model> for EventTeamsDto {
    fn from(m: event_teams::Model) -> Self {
        Self {
            id: m.id,
            event_id: m.event_id,
            name: m.name,
            description: m.description,
            points: m.points,
            created_at: m.created_at,
            updated_at: m.updated_at,
            banned: m.banned,
        }
    }
}
