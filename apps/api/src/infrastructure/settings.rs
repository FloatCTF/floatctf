//! Dynamic DB-backed settings (admin-editable).
//!
//! Process-static env config lives in `crate::core::config::AppConfig`.

use crate::entity::sea_orm_active_enums::SettingValueType;
use crate::entity::settings;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, DbConn, EntityTrait, QueryFilter, sea_query::OnConflict,
};
use std::env;

/// Upsert default rows into the `settings` table (do nothing on conflict).
///
/// Renamed from `init_settings` to make the seed/upsert behaviour explicit.
pub async fn seed_default_settings(db: &DbConn) {
    let defaults = vec![
        (
            "INSTANCE_DESTROY_DELAY",
            env::var("INSTANCE_DESTROY_DELAY").unwrap_or("60".to_string()),
            SettingValueType::Integer,
            "实例销毁延迟时间 (分钟)",
        ),
        (
            "EVENT_SCORE_DECAY",
            env::var("EVENT_SCORE_DECAY").unwrap_or("15".to_string()),
            SettingValueType::Integer,
            "比赛题目分数衰减系数",
        ),
        (
            "EVENT_SCORE_MIN_PERCENT",
            env::var("EVENT_SCORE_MIN_PERCENT").unwrap_or("0.45".to_string()),
            SettingValueType::Float,
            "比赛题目最低分数为题目的百分比",
        ),
        (
            "CHALLENGES_DIR",
            env::var("CHALLENGES_DIR").unwrap_or("./challenges".to_string()),
            SettingValueType::String,
            "题目位置",
        ),
        (
            "HTTP_PREFIX",
            env::var("HTTP_PREFIX").unwrap_or("http://".to_string()),
            SettingValueType::String,
            "HTTP前缀",
        ),
        (
            "NODE_IP",
            env::var("NODE_IP").unwrap_or("127.0.0.1".to_string()),
            SettingValueType::String,
            "节点IP",
        ),
        (
            "UPLOAD_DIR",
            env::var("UPLOAD_DIR").unwrap_or("./uploads".to_string()),
            SettingValueType::String,
            "上传目录位置",
        ),
        (
            "FLAG_PREFIX",
            "flag".to_string(),
            SettingValueType::String,
            "全局flag前缀",
        ),
        (
            "WEAPONS_DIR",
            "./weapons".to_string(),
            SettingValueType::String,
            "工具目录",
        ),
        (
            "IMAGE_DIR",
            "./images".to_string(),
            SettingValueType::String,
            "图片目录",
        ),
        (
            "MAIN_URL",
            "http://localhost:8080".to_string(),
            SettingValueType::String,
            "主站地址前缀baseURL",
        ),
        (
            "SMTP_URI",
            "smtp.example.com:user@example.com:SMTP_PASS".to_string(),
            SettingValueType::String,
            "SMTP服务器地址与凭证",
        ),
    ];
    for (key, value, value_type, description) in defaults {
        let e = settings::Entity::insert(settings::ActiveModel {
            key: Set(key.to_string()),
            value: Set(value.to_string()),
            r#type: Set(value_type),
            description: Set(description.to_string()),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::column(settings::Column::Key)
                .do_nothing()
                .to_owned(),
        )
        .exec(db)
        .await;
        if let Err(err) = e {
            match err {
                sea_orm::DbErr::RecordNotInserted => {
                    tracing::debug!("Setting `{}` already exists, skipped", key);
                }
                _ => {
                    tracing::error!("Failed to insert setting `{}`: {}", key, err);
                }
            }
        }
    }
}

/// Backward-compatible alias for [`seed_default_settings`].
#[deprecated(note = "use seed_default_settings")]
pub async fn init_settings(db: &DbConn) {
    seed_default_settings(db).await;
}

pub async fn get_setting(db: &DbConn, key: &str) -> Result<String, anyhow::Error> {
    settings::Entity::find()
        .filter(settings::Column::Key.eq(key))
        .one(db)
        .await
        .ok()
        .flatten()
        .map(|s| s.value)
        .ok_or(anyhow::anyhow!("Setting not found:{}", key))
}
