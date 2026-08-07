use crate::entity::{sea_orm_active_enums::SettingValueType, settings};
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SettingsDto {
    pub id: Uuid,
    pub key: String,
    /// 数据库中的原始值（保留 {{WORK_DIR}} 等模板，供管理端编辑）
    pub value: String,
    /// 解析 {{VAR}} 引用后的值（供展示/消费方核对）
    pub resolved_value: String,
    pub r#type: SettingValueType,
    pub description: String,
    pub protected: bool,
    pub updated_at: DateTimeWithTimeZone,
}

impl SettingsDto {
    pub fn from_model(m: settings::Model, resolved_value: String) -> Self {
        Self {
            id: m.id,
            key: m.key,
            value: m.value,
            resolved_value,
            r#type: m.r#type,
            description: m.description,
            protected: m.protected,
            updated_at: m.updated_at,
        }
    }
}
