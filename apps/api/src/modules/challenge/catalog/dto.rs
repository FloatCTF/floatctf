//! Challenge catalog DTOs（单版本：identity 直接承载当前 package 字段）。
//!
//! Static flag values are NEVER included in any DTO (secret; admin-only
//! retrieval via a dedicated endpoint if ever needed).

use sea_orm::entity::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::challenges;

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
    /// 当前版本（无 package 时为 None）。
    pub version: Option<String>,
    /// building | ready | failed（无 package 时为 None）。
    pub build_status: Option<String>,
    /// 当前版本镜像 tag（admin 可见）。
    pub image_ref: Option<String>,
    pub attachment: Option<ChallengeAttachmentDto>,
}

impl From<challenges::Model> for ChallengesDto {
    fn from(m: challenges::Model) -> Self {
        Self::from(&m)
    }
}

impl From<&challenges::Model> for ChallengesDto {
    fn from(m: &challenges::Model) -> Self {
        Self {
            id: m.id,
            name: m.name.clone(),
            safe_name: m.safe_name.clone(),
            category: m.category.clone(),
            description: m.description.clone(),
            hidden: m.hidden,
            created_at: m.created_at,
            updated_at: m.updated_at,
            version: m.version.clone(),
            build_status: m.build_status.clone(),
            image_ref: m.image_ref.clone(),
            attachment: m
                .attachment_name
                .clone()
                .map(|name| ChallengeAttachmentDto {
                    name,
                    path: m.attachment_path.clone().unwrap_or_default(),
                    size: m.attachment_size,
                }),
        }
    }
}
