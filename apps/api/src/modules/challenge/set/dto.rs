use crate::entity::challenge_sets;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ChallengeSetsDto {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

impl From<challenge_sets::Model> for ChallengeSetsDto {
    fn from(m: challenge_sets::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            description: m.description,
            created_at: m.created_at,
        }
    }
}
