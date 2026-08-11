//! GameBox library repository + 共享身份辅助。
//!
//! 全局 GameBox 身份（identity only）CRUD / list / hide，
//! 以及运行时镜像钉扎、safe_name 去重等与赛制无关的纯逻辑。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::entity::gameboxes;
use crate::modules::gamebox::{
    GameboxError, GameboxResult,
    identity::{slugify, validate_safe_name},
};

pub async fn find_gamebox_by_id<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
) -> Result<Option<gameboxes::Model>, sea_orm::DbErr> {
    gameboxes::Entity::find_by_id(id).one(db).await
}

pub async fn find_gamebox_by_safe_name<C: ConnectionTrait>(
    db: &C,
    safe_name: &str,
) -> Result<Option<gameboxes::Model>, sea_orm::DbErr> {
    gameboxes::Entity::find()
        .filter(gameboxes::Column::SafeName.eq(safe_name))
        .one(db)
        .await
}

pub async fn list_gameboxes(
    db: &DatabaseConnection,
    include_hidden: bool,
) -> Result<Vec<gameboxes::Model>, sea_orm::DbErr> {
    let mut q = gameboxes::Entity::find().order_by_asc(gameboxes::Column::Name);
    if !include_hidden {
        q = q.filter(gameboxes::Column::Hidden.eq(false));
    }
    q.all(db).await
}

/// 创建 GameBox 身份（name/safe_name/category/description/hidden）。
pub async fn create_gamebox_identity<C: ConnectionTrait>(
    db: &C,
    name: String,
    safe_name: String,
    category: String,
    description: String,
    hidden: bool,
) -> Result<gameboxes::Model, sea_orm::DbErr> {
    let now = chrono::Utc::now().into();
    gameboxes::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(name),
        safe_name: Set(safe_name),
        category: Set(category),
        description: Set(description),
        hidden: Set(hidden),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
}

/// 更新身份 + 可编辑运行参数（name/category/description/hidden + 资源/healthcheck/judge）。
/// 不含 safe_name、digest、镜像 pin、build 状态。
pub struct GameBoxIdentityPatch {
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub hidden: Option<bool>,
    pub username: Option<String>,
    pub recommended_cpu_millis: Option<i64>,
    pub recommended_memory_bytes: Option<i64>,
    pub recommended_pids_limit: Option<i64>,
    /// Some(None) 清空。
    pub healthchecks_json: Option<Option<serde_json::Value>>,
    pub judge_script_name: Option<String>,
    pub judge_script_content: Option<String>,
    /// Some(None) 清空。
    pub judge_args_json: Option<Option<serde_json::Value>>,
    /// Some(None) 清空。
    pub judge_timeout_secs: Option<Option<i32>>,
    /// Some(None) 清空。
    pub judge_retry_interval_secs: Option<Option<i32>>,
}

pub async fn update_gamebox_identity(
    db: &DatabaseConnection,
    id: Uuid,
    patch: GameBoxIdentityPatch,
) -> Result<Option<gameboxes::Model>, sea_orm::DbErr> {
    if find_gamebox_by_id(db, id).await?.is_none() {
        return Ok(None);
    }
    let mut active = gameboxes::ActiveModel {
        id: Set(id),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    };
    if let Some(v) = patch.name {
        active.name = Set(v);
    }
    if let Some(v) = patch.category {
        active.category = Set(v);
    }
    if let Some(v) = patch.description {
        active.description = Set(v);
    }
    if let Some(v) = patch.hidden {
        active.hidden = Set(v);
    }
    if let Some(v) = patch.username {
        let v = v.trim().to_string();
        if v.is_empty() {
            active.username = Set(None);
        } else {
            active.username = Set(Some(v));
        }
    }
    if let Some(v) = patch.recommended_cpu_millis {
        active.recommended_cpu_millis = Set(v);
    }
    if let Some(v) = patch.recommended_memory_bytes {
        active.recommended_memory_bytes = Set(v);
    }
    if let Some(v) = patch.recommended_pids_limit {
        active.recommended_pids_limit = Set(v);
    }
    if let Some(v) = patch.healthchecks_json {
        active.healthchecks_json = Set(v);
    }
    if let Some(v) = patch.judge_script_name {
        let v = v.trim().to_string();
        if v.is_empty() {
            active.judge_script_name = Set(None);
        } else {
            active.judge_script_name = Set(Some(v));
        }
    }
    if let Some(v) = patch.judge_script_content {
        active.judge_script_content = Set(if v.trim().is_empty() { None } else { Some(v) });
    }
    if let Some(v) = patch.judge_args_json {
        active.judge_args_json = Set(v);
    }
    if let Some(v) = patch.judge_timeout_secs {
        active.judge_timeout_secs = Set(v);
    }
    if let Some(v) = patch.judge_retry_interval_secs {
        active.judge_retry_interval_secs = Set(v);
    }
    Ok(Some(active.update(db).await?))
}

/// 归档（hide）GameBox：被赛事引用时禁止 hard delete，一律 hide/archive。
pub async fn set_gamebox_hidden(
    db: &DatabaseConnection,
    id: Uuid,
    hidden: bool,
) -> Result<Option<()>, sea_orm::DbErr> {
    if find_gamebox_by_id(db, id).await?.is_none() {
        return Ok(None);
    }
    gameboxes::ActiveModel {
        id: Set(id),
        hidden: Set(hidden),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(Some(()))
}

// ---------------------------------------------------------------------------
// 共享身份辅助（原 awd::service::gamebox_service 迁出）
// ---------------------------------------------------------------------------

/// 更新 GameBox 身份 + 可编辑运行参数（含参数范围校验）。
pub async fn update_gamebox_identity_checked(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
    patch: GameBoxIdentityPatch,
) -> GameboxResult<gameboxes::Model> {
    if let Some(v) = patch.recommended_cpu_millis {
        if v <= 0 {
            return Err(GameboxError::Validation(
                "recommended_cpu_millis must be > 0".into(),
            ));
        }
    }
    if let Some(v) = patch.recommended_memory_bytes {
        if v <= 0 {
            return Err(GameboxError::Validation(
                "recommended_memory_bytes must be > 0".into(),
            ));
        }
    }
    if let Some(v) = patch.recommended_pids_limit {
        if v <= 0 {
            return Err(GameboxError::Validation(
                "recommended_pids_limit must be > 0".into(),
            ));
        }
    }
    if let Some(Some(v)) = patch.judge_timeout_secs {
        if v < 0 {
            return Err(GameboxError::Validation(
                "judge_timeout_secs must be >= 0".into(),
            ));
        }
    }
    if let Some(Some(v)) = patch.judge_retry_interval_secs {
        if v < 0 {
            return Err(GameboxError::Validation(
                "judge_retry_interval_secs must be >= 0".into(),
            ));
        }
    }
    update_gamebox_identity(db, gamebox_id, patch)
        .await
        .map_err(|e| GameboxError::Database(e.to_string()))?
        .ok_or_else(|| GameboxError::NotFound("GameBox not found".into()))
}

/// safe_name 生成 + 去重（仅用于 admin 手动创建身份场景；import 不走 -2 后缀）。
pub async fn unique_safe_name(
    db: &DatabaseConnection,
    display_name: &str,
) -> GameboxResult<String> {
    let base = slugify(display_name);
    let base = if base.is_empty() {
        "gamebox".to_string()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut i = 1;
    while find_gamebox_by_safe_name(db, &candidate)
        .await
        .map_err(|e| GameboxError::Database(e.to_string()))?
        .is_some()
    {
        candidate = format!("{base}-{i}");
        i += 1;
        if i > 1000 {
            return Err(GameboxError::Internal("safe_name 去重溢出".into()));
        }
    }
    Ok(candidate)
}

/// 校验显式 safe_name（不加自动后缀）。
pub fn validate_identity_safe_name(safe_name: &str) -> GameboxResult<()> {
    validate_safe_name(safe_name).map_err(GameboxError::Validation)
}

/// 运行时镜像钉扎：
/// `image_repo_digest`（完整 `repo@sha256:…`）> `image_id`（仅本地 `sha256:…`）> `image_ref` tag。
pub fn effective_image_ref_from_gamebox(gamebox: &gameboxes::Model) -> GameboxResult<String> {
    if let Some(ref d) = gamebox.image_repo_digest {
        if !d.is_empty() {
            return Ok(d.clone());
        }
    }
    if let Some(ref id) = gamebox.image_id {
        if !id.is_empty() {
            return Ok(id.clone());
        }
    }
    if let Some(ref r) = gamebox.image_ref {
        if !r.is_empty() {
            // Tag-only is a last resort; ready gameboxes should have id or digest.
            if gamebox.build_status.as_deref() == Some(crate::modules::gamebox::BUILD_STATUS_READY)
            {
                return Err(GameboxError::Validation(format!(
                    "ready gamebox {} has no image pin (image_repo_digest/image_id)",
                    gamebox.id
                )));
            }
            return Ok(r.clone());
        }
    }
    Err(GameboxError::Validation(format!(
        "gamebox {} has no usable image reference",
        gamebox.id
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::prelude::DateTimeWithTimeZone;

    fn dummy_gamebox(
        repo_digest: Option<&str>,
        image_id: Option<&str>,
        image_ref: Option<&str>,
        status: &str,
    ) -> gameboxes::Model {
        gameboxes::Model {
            id: Uuid::nil(),
            name: "ttt1".into(),
            safe_name: "ttt1".into(),
            category: "other".into(),
            description: String::new(),
            hidden: false,
            created_at: DateTimeWithTimeZone::from(chrono::Utc::now()),
            updated_at: DateTimeWithTimeZone::from(chrono::Utc::now()),
            version: Some("1.0.0".into()),
            source_toml: None,
            spec_json: None,
            spec_digest: None,
            package_digest: Some("b".into()),
            image_ref: image_ref.map(str::to_string),
            image_id: image_id.map(str::to_string),
            image_repo_digest: repo_digest.map(str::to_string),
            username: Some("ctf".into()),
            recommended_cpu_millis: 1000,
            recommended_memory_bytes: 512 * 1024 * 1024,
            recommended_pids_limit: 100,
            healthchecks_json: None,
            judge_script_name: None,
            judge_script_content: None,
            judge_args_json: None,
            judge_timeout_secs: None,
            judge_retry_interval_secs: None,
            build_status: Some(status.into()),
            build_error: None,
        }
    }

    #[test]
    fn pinned_image_prefers_repo_digest() {
        let g = dummy_gamebox(
            Some("floatctf/gameboxes/ttt1@sha256:abc"),
            Some("sha256:local"),
            Some("floatctf/gameboxes/ttt1:1.0.0"),
            crate::modules::gamebox::BUILD_STATUS_READY,
        );
        assert_eq!(
            effective_image_ref_from_gamebox(&g).unwrap(),
            "floatctf/gameboxes/ttt1@sha256:abc"
        );
    }

    #[test]
    fn pinned_image_falls_back_to_image_id_local_only() {
        let g = dummy_gamebox(
            None,
            Some("sha256:localid"),
            Some("floatctf/gameboxes/ttt1:1.0.0"),
            crate::modules::gamebox::BUILD_STATUS_READY,
        );
        assert_eq!(
            effective_image_ref_from_gamebox(&g).unwrap(),
            "sha256:localid"
        );
    }

    #[test]
    fn ready_without_pin_errors() {
        let g = dummy_gamebox(
            None,
            None,
            Some("floatctf/gameboxes/ttt1:1.0.0"),
            crate::modules::gamebox::BUILD_STATUS_READY,
        );
        assert!(effective_image_ref_from_gamebox(&g).is_err());
    }
}
