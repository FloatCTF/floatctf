//! GameBox library repository — 全局 GameBox 身份（identity only）。
//!
//! 运行时配置（镜像 / judge / healthcheck / resources）在 `gamebox_revisions`。
//! 本 repo 只负责 identity CRUD + list / hide。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::entity::gameboxes;

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
