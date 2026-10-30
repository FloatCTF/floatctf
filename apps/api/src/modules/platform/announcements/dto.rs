use crate::entity::announcements;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AnnouncementsDto {
    pub id: Uuid,
    pub title: String,
    pub content: Option<String>,
    /// 发布者账号 ID；`None` = 发布者账号已注销（展示回落到 `publisher` 快照）。
    pub publisher_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub publisher: String,
}

impl From<announcements::Model> for AnnouncementsDto {
    fn from(m: announcements::Model) -> Self {
        Self {
            id: m.id,
            title: m.title,
            content: m.content,
            publisher_id: m.publisher_id,
            created_at: m.created_at,
            updated_at: m.updated_at,
            publisher: m.publisher,
        }
    }
}
