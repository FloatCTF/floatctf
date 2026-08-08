//! Dynamic DB-backed settings (admin-editable).
//!
//! Process-static env config lives in `crate::core::config::AppConfig`.

use std::collections::{HashMap, HashSet};

use crate::entity::settings;
use crate::{core::AppConfig, entity::sea_orm_active_enums::SettingValueType};
use sea_orm::{ActiveValue::Set, DbConn, EntityTrait, sea_query::OnConflict};
/// Upsert default rows into the `settings` table (do nothing on conflict).
///
/// Values come from the process TOML configuration; settings remain editable
/// through the database after they have been seeded.
pub async fn seed_default_settings(db: &DbConn, config: &AppConfig) {
    let defaults = vec![
        (
            "INSTANCE_DESTROY_DELAY",
            "60".to_string(),
            SettingValueType::Integer,
            "实例销毁延迟时间 (分钟)",
        ),
        (
            "EVENT_SCORE_DECAY",
            "500".to_string(),
            SettingValueType::Integer,
            "比赛题目分数衰减系数",
        ),
        (
            "EVENT_SCORE_MIN_PERCENT",
            "0.45".to_string(),
            SettingValueType::Float,
            "比赛题目最低分数为题目的百分比",
        ),
        (
            "WORK_DIR",
            config.server.work_dir.clone(),
            SettingValueType::String,
            "工作目录（其他设置可用 {{WORK_DIR}} 引用）",
        ),
        (
            "CHALLENGES_DIR",
            "{{WORK_DIR}}/challenges".to_string(),
            SettingValueType::String,
            "题目位置（支持 {{WORK_DIR}} 等变量引用）",
        ),
        (
            "GAMEBOXES_DIR",
            "{{WORK_DIR}}/gameboxes".to_string(),
            SettingValueType::String,
            "GameBox 位置（支持 {{WORK_DIR}} 等变量引用）",
        ),
        (
            "HTTP_PREFIX",
            "http://".to_string(),
            SettingValueType::String,
            "HTTP前缀",
        ),
        (
            "NODE_IP",
            "127.0.0.1".to_string(),
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
            config.main_url.clone(),
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

/// Load all settings into a `key -> raw value` map.
async fn load_settings_map(db: &DbConn) -> HashMap<String, String> {
    let rows = settings::Entity::find().all(db).await.unwrap_or_default();
    rows.into_iter().map(|s| (s.key, s.value)).collect()
}

/// Resolve `{{KEY}}` references in `value` against the DB settings.
///
/// 解析后的值仅用于 API 响应与消费方；数据库里始终保存原始模板，
/// 便于管理端继续编辑 `{{WORK_DIR}}/xxx` 形式。
/// 未定义变量 / 循环引用 / 非法 token 保持原样，永不失败、永不死循环。
pub async fn resolve_setting_value(db: &DbConn, value: &str) -> String {
    let map = load_settings_map(db).await;
    resolve_value_with_map(value, &map)
}

/// 纯函数解析器：`{{KEY}}` 逐层替换为 map 中的原始值，环检测防死循环。
///
/// 规则：
/// - 引用深度不限（`A -> {{B}} -> {{C}}`）
/// - 引用链出现环（`{{A}} -> {{B}} -> {{A}}`）时，重复出现的引用保留字面量
/// - map 中不存在的 key 保留 `{{KEY}}` 字面量
/// - 非法/空 token（如 `{{ }}`）不替换
pub fn resolve_value_with_map(value: &str, map: &HashMap<String, String>) -> String {
    let mut seen: HashSet<String> = HashSet::new();
    resolve_impl(value, map, &mut seen)
}

fn resolve_impl(value: &str, map: &HashMap<String, String>, seen: &mut HashSet<String>) -> String {
    let chars: Vec<(usize, char)> = value.char_indices().collect();
    let n = chars.len();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < n {
        let (_, c) = chars[i];
        if c == '{' && i + 1 < n && chars[i + 1].1 == '{' {
            // 找闭合的 "}}"
            let mut j = i + 2;
            let mut end = None;
            while j + 1 < n {
                if chars[j].1 == '}' && chars[j + 1].1 == '}' {
                    end = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(j) = end {
                let key: String = chars[i + 2..j].iter().map(|(_, c)| *c).collect();
                let valid = !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
                if valid {
                    if seen.contains(&key) {
                        // 环：保留字面量并跳过，避免死循环
                        out.push_str(&chars[i..j + 2].iter().map(|(_, c)| *c).collect::<String>());
                        i = j + 2;
                        continue;
                    }
                    if let Some(v) = map.get(&key) {
                        seen.insert(key.clone());
                        let resolved = resolve_impl(v, map, seen);
                        seen.remove(&key);
                        out.push_str(&resolved);
                        i = j + 2;
                        continue;
                    }
                    // 未定义 key：保留字面量
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// 读取单个设置，返回解析后的值（供 CHALLENGES_DIR/NODE_IP 等服务端消费方使用）。
pub async fn get_setting(db: &DbConn, key: &str) -> Result<String, anyhow::Error> {
    let map = load_settings_map(db).await;
    let raw = map
        .get(key)
        .cloned()
        .ok_or(anyhow::anyhow!("Setting not found:{}", key))?;
    Ok(resolve_value_with_map(&raw, &map))
}

/// Upsert 一个设置值（不存在则插入，存在则更新 value）。
///
/// 动态设置统一走这里（AGENTS.md 铁律 1：配置只从 TOML / settings 表读取）。
/// 内部系统参数（如 AWD 网络策略 revision）由系统自动维护。
pub async fn upsert_setting(db: &DbConn, key: &str, value: &str) -> Result<(), anyhow::Error> {
    settings::Entity::insert(settings::ActiveModel {
        key: Set(key.to_string()),
        value: Set(value.to_string()),
        r#type: Set(SettingValueType::String),
        description: Set("内部系统参数（自动维护）".to_string()),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::column(settings::Column::Key)
            .update_column(settings::Column::Value)
            .to_owned(),
    )
    .exec(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn no_reference_stays_unchanged() {
        let m = map(&[("WORK_DIR", "./")]);
        assert_eq!(
            resolve_value_with_map("/var/lib/floatctf", &m),
            "/var/lib/floatctf"
        );
        assert_eq!(resolve_value_with_map("flag", &m), "flag");
    }

    #[test]
    fn direct_reference() {
        let m = map(&[
            ("WORK_DIR", "./"),
            ("CHALLENGES_DIR", "{{WORK_DIR}}/challenges"),
        ]);
        // 纯字符串拼接："./" + "/challenges" = ".//challenges"（Path 使用时自动归一化）
        assert_eq!(
            resolve_value_with_map("{{WORK_DIR}}/challenges", &m),
            ".//challenges"
        );
        assert_eq!(
            std::path::Path::new(".//challenges"),
            std::path::Path::new("./challenges")
        );
    }

    #[test]
    fn recursive_reference() {
        let m = map(&[
            ("WORK_DIR", "/data"),
            ("CHALLENGES_DIR", "{{WORK_DIR}}/challenges"),
            ("UPLOAD_DIR", "{{CHALLENGES_DIR}}/uploads"),
        ]);
        assert_eq!(
            resolve_value_with_map("{{UPLOAD_DIR}}/x", &m),
            "/data/challenges/uploads/x"
        );
    }

    #[test]
    fn multiple_references_in_one_value() {
        let m = map(&[("A", "1"), ("B", "2")]);
        assert_eq!(resolve_value_with_map("{{A}}-{{B}}", &m), "1-2");
    }

    #[test]
    fn cycle_terminates_and_keeps_literal() {
        let m = map(&[("A", "{{B}}"), ("B", "{{A}}")]);
        // A -> {{B}} -> {{A}}: A 已在 seen 中，保留 {{A}} 字面量
        assert_eq!(resolve_value_with_map("{{A}}", &m), "{{A}}");
    }

    #[test]
    fn self_reference_terminates() {
        let m = map(&[("A", "{{A}}/x")]);
        assert_eq!(resolve_value_with_map("{{A}}", &m), "{{A}}/x");
    }

    #[test]
    fn undefined_key_keeps_literal() {
        let m = map(&[("WORK_DIR", "./")]);
        assert_eq!(
            resolve_value_with_map("{{UNDEFINED}}/path", &m),
            "{{UNDEFINED}}/path"
        );
    }

    #[test]
    fn malformed_tokens_kept() {
        let m = map(&[("A", "1")]);
        assert_eq!(resolve_value_with_map("{{ }}", &m), "{{ }}");
        assert_eq!(
            resolve_value_with_map("open {A} close", &m),
            "open {A} close"
        );
        assert_eq!(resolve_value_with_map("{{A", &m), "{{A");
    }

    #[test]
    fn non_ascii_content_is_preserved() {
        let m = map(&[("WORK_DIR", "/数据")]);
        assert_eq!(
            resolve_value_with_map("路径 {{WORK_DIR}}/子目录", &m),
            "路径 /数据/子目录"
        );
    }
}
