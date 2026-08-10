//! Persistence for `challenge_revisions` (immutable versions).

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::{challenge_revisions, challenges, prelude::ChallengeRevisions};

pub const BUILD_STATUS_BUILDING: &str = "building";
pub const BUILD_STATUS_READY: &str = "ready";
pub const BUILD_STATUS_FAILED: &str = "failed";

/// New revision payload (inserted as `building`).
#[derive(Debug, Clone)]
pub struct NewRevision {
    pub challenge_id: Uuid,
    pub version: String,
    pub revision_number: i32,
    pub source_toml: String,
    pub spec_json: serde_json::Value,
    pub spec_digest: String,
    pub package_digest: String,
    pub flag_type: String,
    pub static_flag_value: Option<String>,
    pub container_port: Option<i32>,
    pub recommended_cpu_millis: i64,
    pub recommended_memory_bytes: i64,
    pub recommended_pids_limit: i64,
    pub attachment_path: Option<String>,
    pub attachment_name: Option<String>,
    pub attachment_size: Option<i64>,
    pub attachment_sha256: Option<String>,
    pub image_ref: Option<String>,
}

pub async fn find_by_challenge_and_version<C: ConnectionTrait>(
    db: &C,
    challenge_id: Uuid,
    version: &str,
) -> Result<Option<challenge_revisions::Model>, sea_orm::DbErr> {
    ChallengeRevisions::find()
        .filter(challenge_revisions::Column::ChallengeId.eq(challenge_id))
        .filter(challenge_revisions::Column::Version.eq(version))
        .one(db)
        .await
}

/// Latest ready revision for a challenge (used by practice launch / event pin defaults).
pub async fn find_latest_ready<C: ConnectionTrait>(
    db: &C,
    challenge_id: Uuid,
) -> Result<Option<challenge_revisions::Model>, sea_orm::DbErr> {
    ChallengeRevisions::find()
        .filter(challenge_revisions::Column::ChallengeId.eq(challenge_id))
        .filter(challenge_revisions::Column::BuildStatus.eq(BUILD_STATUS_READY))
        .order_by_desc(challenge_revisions::Column::CreatedAt)
        .one(db)
        .await
}

/// Load a ready revision by id (also verifies it belongs to `challenge_id`).
pub async fn find_ready_for_challenge<C: ConnectionTrait>(
    db: &C,
    revision_id: Uuid,
    challenge_id: Uuid,
) -> Result<Option<challenge_revisions::Model>, sea_orm::DbErr> {
    ChallengeRevisions::find_by_id(revision_id)
        .filter(challenge_revisions::Column::ChallengeId.eq(challenge_id))
        .filter(challenge_revisions::Column::BuildStatus.eq(BUILD_STATUS_READY))
        .one(db)
        .await
}

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    revision_id: Uuid,
) -> Result<Option<challenge_revisions::Model>, sea_orm::DbErr> {
    ChallengeRevisions::find_by_id(revision_id).one(db).await
}

pub async fn next_revision_number<C: ConnectionTrait>(
    db: &C,
    challenge_id: Uuid,
) -> Result<i32, sea_orm::DbErr> {
    let last = ChallengeRevisions::find()
        .filter(challenge_revisions::Column::ChallengeId.eq(challenge_id))
        .order_by_desc(challenge_revisions::Column::RevisionNumber)
        .one(db)
        .await?;
    Ok(last.map(|m| m.revision_number + 1).unwrap_or(1))
}

/// Insert a revision as `building` (TX1, before docker build).
pub async fn insert_building<C: ConnectionTrait>(
    db: &C,
    new_rev: NewRevision,
) -> Result<challenge_revisions::Model, sea_orm::DbErr> {
    let model = challenge_revisions::ActiveModel {
        challenge_id: Set(new_rev.challenge_id),
        version: Set(new_rev.version),
        revision_number: Set(new_rev.revision_number),
        source_toml: Set(new_rev.source_toml),
        spec_json: Set(new_rev.spec_json),
        spec_digest: Set(new_rev.spec_digest),
        package_digest: Set(new_rev.package_digest),
        flag_type: Set(new_rev.flag_type),
        static_flag_value: Set(new_rev.static_flag_value),
        container_port: Set(new_rev.container_port),
        recommended_cpu_millis: Set(new_rev.recommended_cpu_millis),
        recommended_memory_bytes: Set(new_rev.recommended_memory_bytes),
        recommended_pids_limit: Set(new_rev.recommended_pids_limit),
        attachment_path: Set(new_rev.attachment_path),
        attachment_name: Set(new_rev.attachment_name),
        attachment_size: Set(new_rev.attachment_size),
        attachment_sha256: Set(new_rev.attachment_sha256),
        image_ref: Set(new_rev.image_ref),
        build_status: Set(BUILD_STATUS_BUILDING.to_string()),
        build_error: Set(None),
        ..Default::default()
    };
    model.insert(db).await
}

/// Reset a previously failed revision to `building` for retry (same package_digest).
pub async fn reset_to_building<C: ConnectionTrait>(
    db: &C,
    revision_id: Uuid,
    source_toml: String,
    spec_json: serde_json::Value,
    spec_digest: String,
    package_digest: String,
    flag_type: String,
    static_flag_value: Option<String>,
    container_port: Option<i32>,
    recommended_cpu_millis: i64,
    recommended_memory_bytes: i64,
    recommended_pids_limit: i64,
    attachment_path: Option<String>,
    attachment_name: Option<String>,
    attachment_size: Option<i64>,
    attachment_sha256: Option<String>,
    image_ref: Option<String>,
) -> Result<challenge_revisions::Model, sea_orm::DbErr> {
    let mut am: challenge_revisions::ActiveModel = challenge_revisions::ActiveModel {
        id: Set(revision_id),
        ..Default::default()
    };
    am.source_toml = Set(source_toml);
    am.spec_json = Set(spec_json);
    am.spec_digest = Set(spec_digest);
    am.package_digest = Set(package_digest);
    am.flag_type = Set(flag_type);
    am.static_flag_value = Set(static_flag_value);
    am.container_port = Set(container_port);
    am.recommended_cpu_millis = Set(recommended_cpu_millis);
    am.recommended_memory_bytes = Set(recommended_memory_bytes);
    am.recommended_pids_limit = Set(recommended_pids_limit);
    am.attachment_path = Set(attachment_path);
    am.attachment_name = Set(attachment_name);
    am.attachment_size = Set(attachment_size);
    am.attachment_sha256 = Set(attachment_sha256);
    am.image_ref = Set(image_ref);
    am.build_status = Set(BUILD_STATUS_BUILDING.to_string());
    am.build_error = Set(None);
    am.update(db).await
}

/// Mark ready with image pins (TX2, after build/push). Image pins are immutable once set.
pub async fn mark_ready<C: ConnectionTrait>(
    db: &C,
    revision_id: Uuid,
    image_ref: String,
    image_id: String,
    image_repo_digest: Option<String>,
) -> Result<challenge_revisions::Model, sea_orm::DbErr> {
    let mut am: challenge_revisions::ActiveModel = challenge_revisions::ActiveModel {
        id: Set(revision_id),
        ..Default::default()
    };
    am.image_ref = Set(Some(image_ref));
    am.image_id = Set(Some(image_id));
    am.image_repo_digest = Set(image_repo_digest);
    am.build_status = Set(BUILD_STATUS_READY.to_string());
    am.build_error = Set(None);
    am.update(db).await
}

/// Mark build failed with a bounded sanitized error (revision kept for diagnostics).
pub async fn mark_failed<C: ConnectionTrait>(
    db: &C,
    revision_id: Uuid,
    build_error: String,
) -> Result<challenge_revisions::Model, sea_orm::DbErr> {
    let mut am: challenge_revisions::ActiveModel = challenge_revisions::ActiveModel {
        id: Set(revision_id),
        ..Default::default()
    };
    am.build_status = Set(BUILD_STATUS_FAILED.to_string());
    am.build_error = Set(Some(build_error));
    am.update(db).await
}

/// List all revisions of a challenge (admin detail), newest first.
pub async fn list_for_challenge<C: ConnectionTrait>(
    db: &C,
    challenge_id: Uuid,
) -> Result<Vec<challenge_revisions::Model>, sea_orm::DbErr> {
    ChallengeRevisions::find()
        .filter(challenge_revisions::Column::ChallengeId.eq(challenge_id))
        .order_by_desc(challenge_revisions::Column::RevisionNumber)
        .all(db)
        .await
}

/// Effective immutable image pin: `image_repo_digest` > `image_id`, else error.
///
/// Ready revisions must have at least one pin (LocalOnly mode stores image_id only).
pub fn effective_image_ref(revision: &challenge_revisions::Model) -> Result<String, String> {
    if let Some(ref d) = revision.image_repo_digest {
        return Ok(d.clone());
    }
    if let Some(ref id) = revision.image_id {
        return Ok(id.clone());
    }
    Err(format!(
        "ready revision {} has no image pin (image_repo_digest/image_id)",
        revision.id
    ))
}

/// Load challenge identity for a revision (error mapped by caller).
pub async fn challenge_for_revision<C: ConnectionTrait>(
    db: &C,
    revision_id: Uuid,
) -> Result<Option<challenges::Model>, sea_orm::DbErr> {
    use sea_orm::ModelTrait;
    let rev = ChallengeRevisions::find_by_id(revision_id).one(db).await?;
    match rev {
        Some(r) => r.find_related(challenges::Entity).one(db).await,
        None => Ok(None),
    }
}
