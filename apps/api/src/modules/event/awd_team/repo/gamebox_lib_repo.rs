//! GameBox library repository — 全局 GameBox 单版本身份（同 challenges 语义）。
//!
//! GameBox = AWD 题目的长期身份，运行时配置直接挂在 gameboxes 表上（单版本）。
//! 编辑 = 原地覆盖当前配置（无版本历史，无不可变 Revision 设计）。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::gameboxes;

/// 创建/更新 GameBox 时携带的完整运行时配置（= gameboxes 配置列）。
pub struct GameBoxConfigFields {
    pub source_toml: String,
    pub image_ref: String,
    pub image_digest: Option<String>,
    pub username: String,
    pub default_cpu_millis: i64,
    pub default_memory_bytes: i64,
    pub default_pids_limit: i64,
    pub healthcheck_json: Option<serde_json::Value>,
    pub judge_script_name: Option<String>,
    pub judge_script_content: Option<String>,
    pub judge_args_json: Option<serde_json::Value>,
    pub default_judge_timeout_secs: Option<i32>,
    pub default_judge_retry_interval_secs: Option<i32>,
}

pub async fn find_gamebox_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<gameboxes::Model>, sea_orm::DbErr> {
    gameboxes::Entity::find_by_id(id).one(db).await
}

pub async fn find_gamebox_by_safe_name(
    db: &DatabaseConnection,
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

/// 创建 GameBox（单版本：身份 + 完整运行时配置一次写入，同 challenges 创建语义）。
pub async fn create_gamebox(
    db: &DatabaseConnection,
    name: String,
    safe_name: String,
    category: String,
    description: String,
    hidden: bool,
    cfg: GameBoxConfigFields,
) -> Result<gameboxes::Model, sea_orm::DbErr> {
    let now = chrono::Utc::now().into();
    gameboxes::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(name),
        safe_name: Set(safe_name),
        category: Set(category),
        description: Set(description),
        hidden: Set(hidden),
        source_toml: Set(Some(cfg.source_toml)),
        image_ref: Set(Some(cfg.image_ref)),
        image_digest: Set(cfg.image_digest),
        username: Set(Some(cfg.username)),
        default_cpu_millis: Set(Some(cfg.default_cpu_millis)),
        default_memory_bytes: Set(Some(cfg.default_memory_bytes)),
        default_pids_limit: Set(Some(cfg.default_pids_limit)),
        healthcheck_json: Set(cfg.healthcheck_json),
        judge_script_name: Set(cfg.judge_script_name),
        judge_script_content: Set(cfg.judge_script_content),
        judge_args_json: Set(cfg.judge_args_json),
        default_judge_timeout_secs: Set(cfg.default_judge_timeout_secs),
        default_judge_retry_interval_secs: Set(cfg.default_judge_retry_interval_secs),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
}

/// 编辑 GameBox：原地覆盖当前配置（单版本，同 challenges 编辑语义）。
/// 返回更新后的 Model；GameBox 不存在返回 None。
pub async fn update_gamebox_config(
    db: &DatabaseConnection,
    id: Uuid,
    cfg: GameBoxConfigFields,
) -> Result<Option<gameboxes::Model>, sea_orm::DbErr> {
    if find_gamebox_by_id(db, id).await?.is_none() {
        return Ok(None);
    }
    let now = chrono::Utc::now().into();
    let updated = gameboxes::ActiveModel {
        id: Set(id),
        source_toml: Set(Some(cfg.source_toml)),
        image_ref: Set(Some(cfg.image_ref)),
        image_digest: Set(cfg.image_digest),
        username: Set(Some(cfg.username)),
        default_cpu_millis: Set(Some(cfg.default_cpu_millis)),
        default_memory_bytes: Set(Some(cfg.default_memory_bytes)),
        default_pids_limit: Set(Some(cfg.default_pids_limit)),
        healthcheck_json: Set(cfg.healthcheck_json),
        judge_script_name: Set(cfg.judge_script_name),
        judge_script_content: Set(cfg.judge_script_content),
        judge_args_json: Set(cfg.judge_args_json),
        default_judge_timeout_secs: Set(cfg.default_judge_timeout_secs),
        default_judge_retry_interval_secs: Set(cfg.default_judge_retry_interval_secs),
        updated_at: Set(now),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(Some(updated))
}

/// 归档（hide）GameBox：被赛事引用时禁止 hard delete（§55），一律 hide/archive。
pub async fn set_gamebox_hidden(
    db: &DatabaseConnection,
    id: Uuid,
    hidden: bool,
) -> Result<Option<()>, sea_orm::DbErr> {
    let cur = match find_gamebox_by_id(db, id).await? {
        Some(m) => m,
        None => return Ok(None),
    };
    gameboxes::ActiveModel {
        id: Set(id),
        hidden: Set(hidden),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .update(db)
    .await?;
    let _ = cur;
    Ok(Some(()))
}
