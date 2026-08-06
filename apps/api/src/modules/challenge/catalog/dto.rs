use crate::entity::challenges;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ChallengesDto {
    pub id: Uuid,
    pub name: String,
    pub safe_name: String,
    pub category: String,
    pub description: String,
    pub attachment: Option<String>,
    pub hidden: bool,
    pub toml_str: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl From<challenges::Model> for ChallengesDto {
    fn from(m: challenges::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            safe_name: m.safe_name,
            category: m.category,
            description: m.description,
            attachment: m.attachment,
            hidden: m.hidden,
            toml_str: m.toml_str,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
