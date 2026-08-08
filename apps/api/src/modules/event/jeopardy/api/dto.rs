use crate::entity::{instances, sea_orm_active_enums::InstanceStatus};
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct InstancesDto {
    pub id: Uuid,
    pub status: InstanceStatus,
    pub r#ref: String,
    pub flag: String,
    pub content: Option<String>,
    pub challenge_id: Option<Uuid>,
    pub user_id: Uuid,
    pub identifier: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub destroy_at: DateTimeWithTimeZone,
}

impl From<instances::Model> for InstancesDto {
    fn from(m: instances::Model) -> Self {
        Self {
            id: m.id,
            status: m.status,
            r#ref: m.r#ref,
            flag: m.flag,
            content: m.content,
            challenge_id: m.challenge_id,
            user_id: m.user_id,
            identifier: m.identifier,
            created_at: m.created_at,
            updated_at: m.updated_at,
            destroy_at: m.destroy_at,
        }
    }
}
