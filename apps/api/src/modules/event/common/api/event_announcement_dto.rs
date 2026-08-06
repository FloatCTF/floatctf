use crate::entity::event_announcements;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EventAnnouncementsDto {
    pub id: Uuid,
    pub event_id: Uuid,
    pub title: String,
    pub content: String,
    pub created_at: DateTimeWithTimeZone,
}

impl From<event_announcements::Model> for EventAnnouncementsDto {
    fn from(m: event_announcements::Model) -> Self {
        Self {
            id: m.id,
            event_id: m.event_id,
            title: m.title,
            content: m.content,
            created_at: m.created_at,
        }
    }
}
