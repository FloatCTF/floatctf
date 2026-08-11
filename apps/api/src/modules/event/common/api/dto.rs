use crate::entity::{
    events,
    sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
};
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EventsDto {
    pub id: Uuid,
    pub family: EventFamily,
    pub purpose: EventPurpose,
    pub participant_mode: ParticipantMode,
    pub system_key: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub hidden: bool,
    pub start_time: DateTimeWithTimeZone,
    pub rules: String,
    pub allow_join: bool,
    pub flag_prefix: Option<String>,
    pub end_time: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl From<events::Model> for EventsDto {
    fn from(m: events::Model) -> Self {
        Self {
            id: m.id,
            family: m.family,
            purpose: m.purpose,
            participant_mode: m.participant_mode,
            system_key: m.system_key,
            title: m.title,
            description: m.description,
            hidden: m.hidden,
            start_time: m.start_time,
            rules: m.rules,
            allow_join: m.allow_join,
            flag_prefix: m.flag_prefix,
            end_time: m.end_time,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
