use crate::entity::{sea_orm_active_enums::SettingValueType, settings};
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SettingsDto {
    pub id: Uuid,
    pub key: String,
    pub value: String,
    pub r#type: SettingValueType,
    pub description: String,
    pub protected: bool,
    pub updated_at: DateTimeWithTimeZone,
}

impl From<settings::Model> for SettingsDto {
    fn from(m: settings::Model) -> Self {
        Self {
            id: m.id,
            key: m.key,
            value: m.value,
            r#type: m.r#type,
            description: m.description,
            protected: m.protected,
            updated_at: m.updated_at,
        }
    }
}
