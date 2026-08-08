//! GameBox library repository — 全局 GameBox 长期身份 + 不可变 Revision。
//!
//! GameBox = AWD 题目的长期身份；GameBoxRevision = 不可变部署版本（§6）。
//! 编辑 GameBox = 创建 Revision N+1，绝不修改已有 Revision。
//! spec_digest 相同（canonical spec 未变）时不创建新 Revision（§36）。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::{gamebox_revisions, gameboxes};

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

pub async fn create_gamebox(
    db: &DatabaseConnection,
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

// ── Revisions ──

pub async fn find_revisions_by_gamebox(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
) -> Result<Vec<gamebox_revisions::Model>, sea_orm::DbErr> {
    gamebox_revisions::Entity::find()
        .filter(gamebox_revisions::Column::GameboxId.eq(gamebox_id))
        .order_by_desc(gamebox_revisions::Column::RevisionNumber)
        .all(db)
        .await
}

pub async fn find_revision_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<gamebox_revisions::Model>, sea_orm::DbErr> {
    gamebox_revisions::Entity::find_by_id(id).one(db).await
}

/// 创建 Revision N+1（immutable：创建后永不变更）。
/// 若 canonical spec_digest 与 latest 相同 → 返回 None（不重复建 revision，§36）。
pub struct NewRevision {
    pub source_toml: String,
    pub spec_json: serde_json::Value,
    pub spec_digest: String,
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

pub async fn create_revision(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
    rev: NewRevision,
) -> Result<Option<gamebox_revisions::Model>, sea_orm::DbErr> {
    let latest = find_revisions_by_gamebox(db, gamebox_id).await?;
    if let Some(l) = latest.first() {
        if l.spec_digest == rev.spec_digest {
            // canonical spec 未变化 → 不创建新 revision（§36）
            return Ok(None);
        }
    }
    let next_number = latest.first().map(|l| l.revision_number + 1).unwrap_or(1);
    gamebox_revisions::ActiveModel {
        id: Set(Uuid::new_v4()),
        gamebox_id: Set(gamebox_id),
        revision_number: Set(next_number),
        source_toml: Set(rev.source_toml),
        spec_schema_version: Set(1),
        spec_json: Set(rev.spec_json),
        spec_digest: Set(rev.spec_digest),
        image_ref: Set(rev.image_ref),
        image_digest: Set(rev.image_digest),
        username: Set(rev.username),
        default_cpu_millis: Set(rev.default_cpu_millis),
        default_memory_bytes: Set(rev.default_memory_bytes),
        default_pids_limit: Set(rev.default_pids_limit),
        healthcheck_json: Set(rev.healthcheck_json),
        judge_script_name: Set(rev.judge_script_name),
        judge_script_content: Set(rev.judge_script_content),
        judge_args_json: Set(rev.judge_args_json),
        default_judge_timeout_secs: Set(rev.default_judge_timeout_secs),
        default_judge_retry_interval_secs: Set(rev.default_judge_retry_interval_secs),
        created_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map(Some)
}

/// GameBox + latest revision 视图（列表 API 用）。
pub struct GameBoxWithRevision {
    pub gamebox: gameboxes::Model,
    pub latest_revision: Option<gamebox_revisions::Model>,
}

pub async fn list_gameboxes_with_revisions(
    db: &DatabaseConnection,
    include_hidden: bool,
) -> Result<Vec<GameBoxWithRevision>, sea_orm::DbErr> {
    let mut out = Vec::new();
    for gb in list_gameboxes(db, include_hidden).await? {
        let latest = find_revisions_by_gamebox(db, gb.id)
            .await?
            .into_iter()
            .next();
        out.push(GameBoxWithRevision {
            gamebox: gb,
            latest_revision: latest,
        });
    }
    Ok(out)
}
