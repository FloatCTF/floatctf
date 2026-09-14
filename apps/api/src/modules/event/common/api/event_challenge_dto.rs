use crate::entity::jeopardy_event_challenges;
use sea_orm::entity::prelude::Uuid;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EventChallengesDto {
    pub event_id: Uuid,
    pub challenge_id: Uuid,
    pub points: f64,
    pub hidden: bool,
}

impl From<jeopardy_event_challenges::Model> for EventChallengesDto {
    fn from(m: jeopardy_event_challenges::Model) -> Self {
        Self {
            event_id: m.event_id,
            challenge_id: m.challenge_id,
            points: m.points,
            hidden: m.hidden,
        }
    }
}
