use crate::entity::event_users;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EventUsersDto {
    pub event_id: Uuid,
    pub user_id: Uuid,
    pub points: f64,
    pub banned: bool,
    pub joined_at: DateTimeWithTimeZone,
}

impl From<event_users::Model> for EventUsersDto {
    fn from(m: event_users::Model) -> Self {
        Self {
            event_id: m.event_id,
            user_id: m.user_id,
            points: m.points,
            banned: m.banned,
            joined_at: m.joined_at,
        }
    }
}
