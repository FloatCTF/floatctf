//! GameBoxRevision repository — 不可变版本行 + build 状态更新。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use uuid::Uuid;

use crate::entity::gamebox_revisions;

pub const BUILD_STATUS_BUILDING: &str = "building";
pub const BUILD_STATUS_READY: &str = "ready";
pub const BUILD_STATUS_FAILED: &str = "failed";

/// 新建 revision 时的完整字段（build_status 初始为 building）。
pub struct NewRevision {
    pub gamebox_id: Uuid,
    pub version: String,
    pub revision_number: i32,
    pub source_toml: String,
    pub spec_json: serde_json::Value,
    pub spec_digest: String,
    pub package_digest: String,
    pub image_ref: Option<String>,
    pub username: String,
    pub recommended_cpu_millis: i64,
    pub recommended_memory_bytes: i64,
    pub recommended_pids_limit: i64,
    pub healthchecks_json: serde_json::Value,
    pub judge_script_name: Option<String>,
    pub judge_script_content: Option<String>,
    pub judge_args_json: Option<serde_json::Value>,
    pub judge_timeout_secs: Option<i32>,
    pub judge_retry_interval_secs: Option<i32>,
}

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
) -> Result<Option<gamebox_revisions::Model>, sea_orm::DbErr> {
    gamebox_revisions::Entity::find_by_id(id).one(db).await
}

pub async fn find_by_gamebox_and_version<C: ConnectionTrait>(
    db: &C,
    gamebox_id: Uuid,
    version: &str,
) -> Result<Option<gamebox_revisions::Model>, sea_orm::DbErr> {
    gamebox_revisions::Entity::find()
        .filter(gamebox_revisions::Column::GameboxId.eq(gamebox_id))
        .filter(gamebox_revisions::Column::Version.eq(version))
        .one(db)
        .await
}

/// 同 gamebox 下最新 ready revision（按 revision_number desc）。
pub async fn find_latest_ready_by_gamebox<C: ConnectionTrait>(
    db: &C,
    gamebox_id: Uuid,
) -> Result<Option<gamebox_revisions::Model>, sea_orm::DbErr> {
    gamebox_revisions::Entity::find()
        .filter(gamebox_revisions::Column::GameboxId.eq(gamebox_id))
        .filter(gamebox_revisions::Column::BuildStatus.eq(BUILD_STATUS_READY))
        .order_by_desc(gamebox_revisions::Column::RevisionNumber)
        .one(db)
        .await
}

/// 同 gamebox 下最新 revision（任意状态）。
pub async fn find_latest_by_gamebox<C: ConnectionTrait>(
    db: &C,
    gamebox_id: Uuid,
) -> Result<Option<gamebox_revisions::Model>, sea_orm::DbErr> {
    gamebox_revisions::Entity::find()
        .filter(gamebox_revisions::Column::GameboxId.eq(gamebox_id))
        .order_by_desc(gamebox_revisions::Column::RevisionNumber)
        .one(db)
        .await
}

pub async fn list_by_gamebox(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
) -> Result<Vec<gamebox_revisions::Model>, sea_orm::DbErr> {
    gamebox_revisions::Entity::find()
        .filter(gamebox_revisions::Column::GameboxId.eq(gamebox_id))
        .order_by_desc(gamebox_revisions::Column::RevisionNumber)
        .all(db)
        .await
}

/// 下一 revision_number = max+1（无记录时为 1）。
pub async fn next_revision_number<C: ConnectionTrait>(
    db: &C,
    gamebox_id: Uuid,
) -> Result<i32, sea_orm::DbErr> {
    let max: Option<i32> = gamebox_revisions::Entity::find()
        .filter(gamebox_revisions::Column::GameboxId.eq(gamebox_id))
        .select_only()
        .column_as(gamebox_revisions::Column::RevisionNumber.max(), "max_rev")
        .into_tuple::<Option<i32>>()
        .one(db)
        .await?
        .flatten();
    Ok(max.unwrap_or(0) + 1)
}

pub async fn insert_building<C: ConnectionTrait>(
    db: &C,
    new: NewRevision,
) -> Result<gamebox_revisions::Model, sea_orm::DbErr> {
    let now = chrono::Utc::now().into();
    gamebox_revisions::ActiveModel {
        id: Set(Uuid::new_v4()),
        gamebox_id: Set(new.gamebox_id),
        version: Set(new.version),
        revision_number: Set(new.revision_number),
        source_toml: Set(new.source_toml),
        spec_json: Set(new.spec_json),
        spec_digest: Set(new.spec_digest),
        package_digest: Set(new.package_digest),
        image_ref: Set(new.image_ref),
        image_id: Set(None),
        image_repo_digest: Set(None),
        username: Set(new.username),
        recommended_cpu_millis: Set(new.recommended_cpu_millis),
        recommended_memory_bytes: Set(new.recommended_memory_bytes),
        recommended_pids_limit: Set(new.recommended_pids_limit),
        healthchecks_json: Set(new.healthchecks_json),
        judge_script_name: Set(new.judge_script_name),
        judge_script_content: Set(new.judge_script_content),
        judge_args_json: Set(new.judge_args_json),
        judge_timeout_secs: Set(new.judge_timeout_secs),
        judge_retry_interval_secs: Set(new.judge_retry_interval_secs),
        build_status: Set(BUILD_STATUS_BUILDING.to_string()),
        build_error: Set(None),
        created_at: Set(now),
    }
    .insert(db)
    .await
}

/// failed → building 重试：清空镜像字段与 error，更新 package/spec 字段（若 digest 相同则内容一致）。
pub async fn reset_to_building<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
    source_toml: String,
    spec_json: serde_json::Value,
    spec_digest: String,
    package_digest: String,
    image_ref: Option<String>,
    username: String,
    recommended_cpu_millis: i64,
    recommended_memory_bytes: i64,
    recommended_pids_limit: i64,
    healthchecks_json: serde_json::Value,
    judge_script_name: Option<String>,
    judge_script_content: Option<String>,
) -> Result<gamebox_revisions::Model, sea_orm::DbErr> {
    gamebox_revisions::ActiveModel {
        id: Set(id),
        source_toml: Set(source_toml),
        spec_json: Set(spec_json),
        spec_digest: Set(spec_digest),
        package_digest: Set(package_digest),
        image_ref: Set(image_ref),
        image_id: Set(None),
        image_repo_digest: Set(None),
        username: Set(username),
        recommended_cpu_millis: Set(recommended_cpu_millis),
        recommended_memory_bytes: Set(recommended_memory_bytes),
        recommended_pids_limit: Set(recommended_pids_limit),
        healthchecks_json: Set(healthchecks_json),
        judge_script_name: Set(judge_script_name),
        judge_script_content: Set(judge_script_content),
        build_status: Set(BUILD_STATUS_BUILDING.to_string()),
        build_error: Set(None),
        ..Default::default()
    }
    .update(db)
    .await
}

pub async fn mark_ready<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
    image_ref: String,
    image_id: String,
    image_repo_digest: Option<String>,
) -> Result<gamebox_revisions::Model, sea_orm::DbErr> {
    gamebox_revisions::ActiveModel {
        id: Set(id),
        image_ref: Set(Some(image_ref)),
        image_id: Set(Some(image_id)),
        image_repo_digest: Set(image_repo_digest),
        build_status: Set(BUILD_STATUS_READY.to_string()),
        build_error: Set(None),
        ..Default::default()
    }
    .update(db)
    .await
}

pub async fn mark_failed<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
    error: String,
) -> Result<gamebox_revisions::Model, sea_orm::DbErr> {
    gamebox_revisions::ActiveModel {
        id: Set(id),
        build_status: Set(BUILD_STATUS_FAILED.to_string()),
        build_error: Set(Some(error)),
        ..Default::default()
    }
    .update(db)
    .await
}
