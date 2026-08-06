//! Weapon request / form DTOs.

use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateWeaponRequest {
    pub name: String,
    pub category: String,
    pub description: Option<String>,
    pub has_file: bool,
    pub file_url: String,
    pub download_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchWeaponRequest {
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub has_file: Option<bool>,
    pub file_url: Option<String>,
    pub download_count: Option<i64>,
}

#[derive(Debug, MultipartForm)]
pub struct WeaponForm {
    #[multipart(limit = "10240MB")]
    pub weapon: TempFile,
}

// --- Response DTO (from entity) ---

use crate::entity::weapons;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};

#[derive(Debug, Serialize)]
pub struct WeaponsDto {
    pub id: Uuid,
    pub name: String,
    pub category: String,
    pub description: Option<String>,
    pub has_file: bool,
    pub download_count: i64,
    pub file_url: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl From<weapons::Model> for WeaponsDto {
    fn from(m: weapons::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            category: m.category,
            description: m.description,
            has_file: m.has_file,
            download_count: m.download_count,
            file_url: m.file_url,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
