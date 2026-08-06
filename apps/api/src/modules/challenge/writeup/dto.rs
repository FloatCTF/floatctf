use crate::entity::challenge_writeup;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ChallengeWriteupDto {
    pub id: Uuid,
    pub challenge_id: Uuid,
    pub user_id: Uuid,
    pub content: String,
    pub created_at: DateTimeWithTimeZone,
}

impl From<challenge_writeup::Model> for ChallengeWriteupDto {
    fn from(m: challenge_writeup::Model) -> Self {
        Self {
            id: m.id,
            challenge_id: m.challenge_id,
            user_id: m.user_id,
            content: m.content,
            created_at: m.created_at,
        }
    }
}
