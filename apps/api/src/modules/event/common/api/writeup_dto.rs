use crate::entity::event_writeup;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EventWriteupDto {
    pub event_id: Uuid,
    pub user_id: Uuid,
    pub team_id: Option<Uuid>,
    pub file_url: String,
    pub created_at: DateTimeWithTimeZone,
}

impl From<event_writeup::Model> for EventWriteupDto {
    fn from(m: event_writeup::Model) -> Self {
        Self {
            event_id: m.event_id,
            user_id: m.user_id,
            team_id: m.team_id,
            file_url: m.file_url,
            created_at: m.created_at,
        }
    }
}
