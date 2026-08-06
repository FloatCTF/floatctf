use crate::entity::event_challenges;
use sea_orm::entity::prelude::Uuid;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EventChallengesDto {
    pub event_id: Uuid,
    pub challenge_id: Uuid,
    pub points: f64,
    pub hidden: bool,
}

impl From<event_challenges::Model> for EventChallengesDto {
    fn from(m: event_challenges::Model) -> Self {
        Self {
            event_id: m.event_id,
            challenge_id: m.challenge_id,
            points: m.points,
            hidden: m.hidden,
        }
    }
}
