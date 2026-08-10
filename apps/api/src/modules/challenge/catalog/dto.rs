//! Challenge catalog DTOs (identity + latest-revision summary).
//!
//! Static flag values are NEVER included in any DTO (secret; admin-only
//! retrieval via a dedicated endpoint if ever needed).

use sea_orm::DatabaseConnection;
use sea_orm::entity::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{challenge_revisions, challenges};
use crate::modules::challenge::build::revision_repo;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChallengeAttachmentDto {
    /// File name (display).
    pub name: String,
    /// Relative path inside the package (e.g. `attachment/src.zip`) — used to build the download href.
    pub path: String,
    pub size: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChallengesDto {
    pub id: Uuid,
    pub name: String,
    pub safe_name: String,
    pub category: String,
    pub description: String,
    pub hidden: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    /// Latest ready revision version (None when the challenge has no ready package).
    pub latest_version: Option<String>,
    pub latest_build_status: Option<String>,
    /// Immutable image pin of the latest ready revision (admin-visible).
    pub latest_image_ref: Option<String>,
    pub attachment: Option<ChallengeAttachmentDto>,
}

impl From<challenges::Model> for ChallengesDto {
    fn from(m: challenges::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            safe_name: m.safe_name,
            category: m.category,
            description: m.description,
            hidden: m.hidden,
            created_at: m.created_at,
            updated_at: m.updated_at,
            latest_version: None,
            latest_build_status: None,
            latest_image_ref: None,
            attachment: None,
        }
    }
}

impl ChallengesDto {
    /// Enrich with the latest ready revision (version / image pin / attachment).
    pub async fn from_model(
        db: &DatabaseConnection,
        model: &challenges::Model,
    ) -> Result<Self, sea_orm::DbErr> {
        let mut dto: ChallengesDto = model.clone().into();
        if let Some(rev) = revision_repo::find_latest_ready(db, model.id).await? {
            dto.latest_version = Some(rev.version.clone());
            dto.latest_build_status = Some(rev.build_status.clone());
            dto.latest_image_ref = rev.image_ref.clone();
            dto.attachment = rev
                .attachment_name
                .clone()
                .map(|name| ChallengeAttachmentDto {
                    name,
                    path: rev.attachment_path.clone().unwrap_or_default(),
                    size: rev.attachment_size,
                });
        }
        Ok(dto)
    }

    /// Batch-enrich models with their latest ready revisions (single query, avoids N+1).
    pub async fn from_models(
        db: &DatabaseConnection,
        models: &[challenges::Model],
    ) -> Result<Vec<Self>, sea_orm::DbErr> {
        let ids: Vec<Uuid> = models.iter().map(|m| m.id).collect();
        let rev_map = revision_repo::find_latest_ready_map(db, &ids).await?;
        Ok(models
            .iter()
            .map(|m| {
                let mut dto: ChallengesDto = m.clone().into();
                if let Some(rev) = rev_map.get(&m.id) {
                    dto.latest_version = Some(rev.version.clone());
                    dto.latest_build_status = Some(rev.build_status.clone());
                    dto.latest_image_ref = rev.image_ref.clone();
                    dto.attachment =
                        rev.attachment_name
                            .clone()
                            .map(|name| ChallengeAttachmentDto {
                                name,
                                path: rev.attachment_path.clone().unwrap_or_default(),
                                size: rev.attachment_size,
                            });
                }
                dto
            })
            .collect())
    }
}

/// Admin revision detail (never exposes `static_flag_value`).
#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeRevisionDto {
    pub id: Uuid,
    pub challenge_id: Uuid,
    pub revision_number: i32,
    pub version: String,
    pub build_status: String,
    pub build_error: Option<String>,
    pub flag_type: String,
    pub container_port: Option<i32>,
    pub recommended_cpu_millis: i64,
    pub recommended_memory_bytes: i64,
    pub recommended_pids_limit: i64,
    pub attachment_name: Option<String>,
    pub attachment_path: Option<String>,
    pub attachment_size: Option<i64>,
    pub spec_digest: String,
    pub package_digest: String,
    pub image_ref: Option<String>,
    pub image_repo_digest: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

impl From<challenge_revisions::Model> for ChallengeRevisionDto {
    fn from(r: challenge_revisions::Model) -> Self {
        Self {
            id: r.id,
            challenge_id: r.challenge_id,
            revision_number: r.revision_number,
            version: r.version,
            build_status: r.build_status,
            build_error: r.build_error,
            flag_type: r.flag_type,
            container_port: r.container_port,
            recommended_cpu_millis: r.recommended_cpu_millis,
            recommended_memory_bytes: r.recommended_memory_bytes,
            recommended_pids_limit: r.recommended_pids_limit,
            attachment_name: r.attachment_name,
            attachment_path: r.attachment_path,
            attachment_size: r.attachment_size,
            spec_digest: r.spec_digest,
            package_digest: r.package_digest,
            image_ref: r.image_ref,
            image_repo_digest: r.image_repo_digest,
            created_at: r.created_at,
        }
    }
}
