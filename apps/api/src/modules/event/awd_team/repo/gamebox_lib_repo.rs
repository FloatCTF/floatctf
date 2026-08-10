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
    }
    .insert(db)
    .await
}

/// 更新身份元数据（name/category/description/hidden）。不改 safe_name。
pub struct GameBoxIdentityPatch {
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub hidden: Option<bool>,
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
