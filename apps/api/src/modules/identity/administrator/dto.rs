use crate::entity::super_admin;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SuperAdminDto {
    pub id: Uuid,
    pub username: String,
    pub password: String,
    pub email: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl From<super_admin::Model> for SuperAdminDto {
    fn from(m: super_admin::Model) -> Self {
        Self {
            id: m.id,
            username: m.username,
            password: m.password,
            email: m.email,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
