use crate::entity::logs;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;
use serde_json::Value as Json;

#[derive(Debug, Serialize)]
pub struct LogsDto {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub superadmin_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub category: String,
    pub action: String,
    pub level: String,
    pub message: String,
    pub details: Json,
    pub created_at: DateTimeWithTimeZone,
}

impl From<logs::Model> for LogsDto {
    fn from(m: logs::Model) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            superadmin_id: m.superadmin_id,
            ip_address: m.ip_address,
            category: m.category,
            action: m.action,
            level: m.level,
            message: m.message,
            details: m.details,
            created_at: m.created_at,
        }
    }
}
