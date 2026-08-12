//! awdp_event_gameboxes 仓储（赛事 GameBox 选择）。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::entity::{awdp_event_gameboxes, gameboxes};
use crate::modules::event::awdp::{AwdpError, AwdpResult};
use crate::modules::gamebox::library;

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
) -> Result<Option<awdp_event_gameboxes::Model>, sea_orm::DbErr> {
    awdp_event_gameboxes::Entity::find_by_id(id).one(db).await
}

pub async fn require_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> AwdpResult<awdp_event_gameboxes::Model> {
    find_by_id(db, id)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("awdp event_gamebox not found".into()))
}

pub async fn list_for_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Vec<awdp_event_gameboxes::Model>> {
    awdp_event_gameboxes::Entity::find()
        .filter(awdp_event_gameboxes::Column::EventId.eq(event_id))
        .order_by_asc(awdp_event_gameboxes::Column::CreatedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// attach：复制 GameBox recommended 资源到赛事行（NOT NULL 风格，照 awd）。
pub async fn attach_gamebox(
    db: &DatabaseConnection,
    event_id: Uuid,
    gamebox_id: Uuid,
    hidden: bool,
) -> AwdpResult<awdp_event_gameboxes::Model> {
    let gamebox: gameboxes::Model = library::find_gamebox_by_id(db, gamebox_id)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("GameBox not found".into()))?;

    if gamebox.build_status.as_deref() != Some(crate::modules::gamebox::BUILD_STATUS_READY) {
        return Err(AwdpError::Validation(format!(
            "GameBox {} is not ready (status={:?}); AWDP 要求完整 [awdp] capability",
            gamebox.id, gamebox.build_status
        )));
    }
    // AWDP picker 只允许完整 [awdp] capability 的 GameBox。
    if gamebox.awdp_source_artifact_key.is_none() {
        return Err(AwdpError::Validation(format!(
            "GameBox {} 没有 [awdp] capability（缺少 source.zip 产物），不能用于 AWDP",
            gamebox.safe_name
        )));
    }

    let now = Utc::now().into();
    let model = awdp_event_gameboxes::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        gamebox_id: Set(gamebox_id),
        enabled: Set(true),
        hidden: Set(hidden),
        cpu_millis: Set(gamebox.recommended_cpu_millis),
        memory_bytes: Set(gamebox.recommended_memory_bytes),
        pids_limit: Set(gamebox.recommended_pids_limit),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let res = model.insert(db).await.map_err(|e| match e {
        sea_orm::DbErr::Exec(inner)
            if inner.to_string().contains("awdp_event_gameboxes_unique") =>
        {
            AwdpError::Conflict("该 GameBox 已挂载到本赛事".into())
        }
        other => AwdpError::Database(other.to_string()),
    })?;
    Ok(res)
}

pub async fn detach_gamebox(db: &DatabaseConnection, id: Uuid) -> AwdpResult<()> {
    awdp_event_gameboxes::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

/// 幂等挂载（练习模块专用）：确保 (event_id, gamebox_id) 挂载行存在。
/// 供 `start_training` 使用——练习 gamebox 统一挂到 `AWDPlusPractice` 虚拟赛事上，
/// 便于 admin 在赛事 GameBoxes/Instance 维度统一查看。已存在则直接返回（含并发安全）。
pub async fn ensure_mounted(
    db: &DatabaseConnection,
    event_id: Uuid,
    gamebox_id: Uuid,
) -> AwdpResult<awdp_event_gameboxes::Model> {
    let find = || async {
        awdp_event_gameboxes::Entity::find()
            .filter(awdp_event_gameboxes::Column::EventId.eq(event_id))
            .filter(awdp_event_gameboxes::Column::GameboxId.eq(gamebox_id))
            .one(db)
            .await
    };
    if let Some(existing) = find()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
    {
        return Ok(existing);
    }

    let gamebox: gameboxes::Model = library::find_gamebox_by_id(db, gamebox_id)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("GameBox not found".into()))?;
    let now = Utc::now().into();
    let model = awdp_event_gameboxes::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        gamebox_id: Set(gamebox_id),
        enabled: Set(true),
        hidden: Set(false),
        cpu_millis: Set(gamebox.recommended_cpu_millis),
        memory_bytes: Set(gamebox.recommended_memory_bytes),
        pids_limit: Set(gamebox.recommended_pids_limit),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    match model.insert(db).await {
        Ok(m) => Ok(m),
        // 并发 ensure：唯一冲突 → 重新查询。
        Err(_) => find()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            .ok_or_else(|| AwdpError::Database("ensure_mounted race".into())),
    }
}

pub async fn set_enabled(
    db: &DatabaseConnection,
    id: Uuid,
    enabled: bool,
    hidden: Option<bool>,
) -> AwdpResult<awdp_event_gameboxes::Model> {
    let mut am: awdp_event_gameboxes::ActiveModel = require_by_id(db, id).await?.into();
    am.enabled = Set(enabled);
    if let Some(h) = hidden {
        am.hidden = Set(h);
    }
    am.updated_at = Set(Utc::now().into());
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

pub async fn find_gamebox_identity(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
) -> AwdpResult<gameboxes::Model> {
    library::find_gamebox_by_id(db, gamebox_id)
        .await
        .map_err(|e: sea_orm::DbErr| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("GameBox identity not found".into()))
}

/// 获取 event_gamebox 的 effective 资源与 healthcheck（覆盖 > GameBox 默认）。
pub async fn effective_gamebox_spec(
    db: &DatabaseConnection,
    event_gamebox_id: Uuid,
) -> AwdpResult<(awdp_event_gameboxes::Model, gameboxes::Model)> {
    let eg = require_by_id(db, event_gamebox_id).await?;
    let gamebox = find_gamebox_identity(db, eg.gamebox_id).await?;
    Ok((eg, gamebox))
}

/// 按 (event_id, gamebox_id) 查找赛事挂载行（competition run 从 gamebox_id 解析 eg）。
pub async fn find_for_event_and_gamebox(
    db: &DatabaseConnection,
    event_id: Uuid,
    gamebox_id: Uuid,
) -> AwdpResult<Option<awdp_event_gameboxes::Model>> {
    awdp_event_gameboxes::Entity::find()
        .filter(awdp_event_gameboxes::Column::EventId.eq(event_id))
        .filter(awdp_event_gameboxes::Column::GameboxId.eq(gamebox_id))
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// practice 用的 GameBox 默认规格（无赛事 override）：直接返回 gamebox 身份。
pub async fn effective_gamebox_spec_by_gamebox(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
) -> AwdpResult<gameboxes::Model> {
    find_gamebox_identity(db, gamebox_id).await
}
