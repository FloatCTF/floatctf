//! 武器库应用辅助（数据库 / 存储编排）。

use aws_sdk_s3::primitives::ByteStream;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, Set,
};
use uuid::Uuid;

use super::dto::{CreateWeaponRequest, PatchWeaponRequest};
use crate::api::AppError;
use crate::entity::weapons;

pub async fn list_all_admin(db: &DatabaseConnection) -> Result<Vec<weapons::Model>, AppError> {
    Ok(weapons::Entity::find()
        .order_by_desc(weapons::Column::UpdatedAt)
        .all(db)
        .await?)
}

pub async fn create(
    db: &DatabaseConnection,
    req: CreateWeaponRequest,
) -> Result<weapons::Model, AppError> {
    let weapon = weapons::ActiveModel {
        name: Set(req.name),
        category: Set(req.category),
        description: Set(req.description),
        has_file: Set(req.has_file),
        file_url: Set(req.file_url),
        download_count: Set(req.download_count),
        ..Default::default()
    };
    Ok(weapon.insert(db).await?)
}

pub async fn patch(
    db: &DatabaseConnection,
    weapon_id: Uuid,
    req: PatchWeaponRequest,
) -> Result<weapons::Model, AppError> {
    let mut weapon = weapons::Entity::find_by_id(weapon_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound(format!(
            "Weapon {} not exist",
            weapon_id
        )))?
        .into_active_model();

    if let Some(name) = req.name {
        weapon.name = Set(name);
    }
    if let Some(category) = req.category {
        weapon.category = Set(category);
    }
    if let Some(description) = req.description {
        weapon.description = Set(Some(description));
    }
    if let Some(has_file) = req.has_file {
        weapon.has_file = Set(has_file);
    }
    if let Some(file_url) = req.file_url {
        weapon.file_url = Set(file_url);
    }
    if let Some(download_count) = req.download_count {
        weapon.download_count = Set(download_count);
    }

    Ok(weapon.update(db).await?)
}

pub async fn delete_many(db: &DatabaseConnection, id_list: Vec<Uuid>) -> Result<u64, AppError> {
    Ok(weapons::Entity::delete_many()
        .filter(weapons::Column::Id.is_in(id_list))
        .exec(db)
        .await?
        .rows_affected)
}

pub async fn upload_file(
    db: &DatabaseConnection,
    rustfs: &aws_sdk_s3::Client,
    weapon_id: Uuid,
    file_path: &std::path::Path,
    file_name: String,
) -> Result<(), AppError> {
    let s3_key = format!("weapons/{}", file_name);

    let body = ByteStream::from(
        tokio::fs::read(file_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read weapon file: {}", e)))?,
    );

    rustfs
        .put_object()
        .bucket("floatctf-public")
        .key(&s3_key)
        .body(body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to upload weapon to S3: {}", e)))?;

    let mut weapon = weapons::Entity::find_by_id(weapon_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound(format!(
            "Weapon {} not exist",
            weapon_id
        )))?
        .into_active_model();
    weapon.has_file = Set(true);
    weapon.file_url = Set(s3_key);
    weapon.update(db).await?;

    Ok(())
}
