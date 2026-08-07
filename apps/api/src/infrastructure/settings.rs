//! Dynamic DB-backed settings (admin-editable).
//!
//! Process-static env config lives in `crate::core::config::AppConfig`.

use crate::entity::settings;
use crate::{core::AppConfig, entity::sea_orm_active_enums::SettingValueType};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, DbConn, EntityTrait, QueryFilter, sea_query::OnConflict,
};
/// Upsert default rows into the `settings` table (do nothing on conflict).
///
/// Values come from the process TOML configuration; settings remain editable
/// through the database after they have been seeded.
pub async fn seed_default_settings(db: &DbConn, config: &AppConfig) {
    let defaults = vec![
        (
            "INSTANCE_DESTROY_DELAY",
            config.challenge.instance_destroy_delay.clone(),
            SettingValueType::Integer,
            "实例销毁延迟时间 (分钟)",
        ),
        (
            "EVENT_SCORE_DECAY",
            config.challenge.event_score_decay.clone(),
            SettingValueType::Integer,
            "比赛题目分数衰减系数",
        ),
        (
            "EVENT_SCORE_MIN_PERCENT",
            config.challenge.event_score_min_percent.clone(),
            SettingValueType::Float,
            "比赛题目最低分数为题目的百分比",
        ),
        (
            "CHALLENGES_DIR",
            config.challenge.challenges_dir.clone(),
            SettingValueType::String,
            "题目位置",
        ),
        (
            "HTTP_PREFIX",
            config.challenge.http_prefix.clone(),
            SettingValueType::String,
            "HTTP前缀",
        ),
        (
            "NODE_IP",
            config.challenge.node_ip.clone(),
            SettingValueType::String,
            "节点IP",
        ),
        (
            "FLAG_PREFIX",
            "flag".to_string(),
            SettingValueType::String,
            "全局flag前缀",
        ),
        (
            "MAIN_URL",
            config.challenge.main_url.clone(),
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
pub async fn init_settings(db: &DbConn, config: &AppConfig) {
    seed_default_settings(db, config).await;
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
