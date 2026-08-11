//! AWDP 实例仓储：generic `instances` 根 + `awdp_instances` extension。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder, TransactionTrait,
};
use uuid::Uuid;

use crate::entity::{awdp_instances, instances};
use crate::modules::event::awdp::{AwdpError, AwdpResult};

/// 创建逻辑实例（instances + awdp_instances 同事务写入；owner 双主体镜像）。
pub async fn create_instance(
    db: &DatabaseConnection,
    event_id: Uuid,
    event_gamebox_id: Uuid,
    owner_user_id: Option<Uuid>,
    owner_team_id: Option<Uuid>,
    container_name: &str,
    image_ref: &str,
) -> AwdpResult<(instances::Model, awdp_instances::Model)> {
    let txn = db
        .begin()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    let now = Utc::now().into();
    let instance = instances::ActiveModel {
        id: Set(Uuid::new_v4()),
        owner_user_id: Set(owner_user_id),
        owner_team_id: Set(owner_team_id),
        image_ref: Set(Some(image_ref.to_string())),
        container_id: Set(None),
        container_name: Set(container_name.to_string()),
        runtime_state: Set("starting".to_string()),
        runtime_generation: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .map_err(|e| AwdpError::Database(e.to_string()))?;

    let ext = awdp_instances::ActiveModel {
        instance_id: Set(instance.id),
        event_id: Set(event_id),
        event_gamebox_id: Set(event_gamebox_id),
        owner_user_id: Set(owner_user_id),
        owner_team_id: Set(owner_team_id),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .map_err(|e| match e {
        sea_orm::DbErr::Exec(inner) if inner.to_string().contains("awdp_instances_") => {
            AwdpError::Conflict("该 GameBox 已有一个进行中的实例（subject × gamebox 唯一）".into())
        }
        other => AwdpError::Database(other.to_string()),
    })?;

    txn.commit()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok((instance, ext))
}

/// 按 subject × event_gamebox 查找现有实例。
pub async fn find_instance_for_subject(
    db: &DatabaseConnection,
    event_id: Uuid,
    event_gamebox_id: Uuid,
    owner_user_id: Option<Uuid>,
    owner_team_id: Option<Uuid>,
) -> AwdpResult<Option<(instances::Model, awdp_instances::Model)>> {
    let mut q = awdp_instances::Entity::find()
        .filter(awdp_instances::Column::EventId.eq(event_id))
        .filter(awdp_instances::Column::EventGameboxId.eq(event_gamebox_id));
    q = match (owner_user_id, owner_team_id) {
        (Some(u), None) => q.filter(awdp_instances::Column::OwnerUserId.eq(u)),
        (None, Some(t)) => q.filter(awdp_instances::Column::OwnerTeamId.eq(t)),
        _ => return Err(AwdpError::Internal("exactly-one owner required".into())),
    };
    let ext = q
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let Some(ext) = ext else { return Ok(None) };
    let instance = instances::Entity::find_by_id(ext.instance_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::Internal("instance row missing for awdp_instances".into()))?;
    Ok(Some((instance, ext)))
}

/// 按 instance_id 查找（含 extension 与 event_gamebox）。
pub async fn find_by_instance_id(
    db: &DatabaseConnection,
    instance_id: Uuid,
) -> AwdpResult<(instances::Model, awdp_instances::Model)> {
    let ext = awdp_instances::Entity::find_by_id(instance_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("awdp instance not found".into()))?;
    let instance = instances::Entity::find_by_id(instance_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("instance not found".into()))?;
    Ok((instance, ext))
}

/// 更新 runtime 状态（容器 create/start/stop/reset 后）。
pub async fn update_runtime_state(
    db: &DatabaseConnection,
    instance_id: Uuid,
    runtime_state: &str,
    container_id: Option<&str>,
    generation: i64,
) -> AwdpResult<instances::Model> {
    let now = Utc::now();
    let mut am: instances::ActiveModel = instances::Entity::find_by_id(instance_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("instance not found".into()))?
        .into();
    am.runtime_state = Set(runtime_state.to_string());
    if let Some(cid) = container_id {
        am.container_id = Set(Some(cid.to_string()));
    }
    am.runtime_generation = Set(generation);
    if runtime_state == "running" {
        am.started_at = Set(Some(now.into()));
    } else if runtime_state == "stopped" {
        am.stopped_at = Set(Some(now.into()));
    }
    am.updated_at = Set(now.into());
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 列表：事件下全部实例（管理端 inspect）。
pub async fn list_for_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Vec<(instances::Model, awdp_instances::Model)>> {
    let exts = awdp_instances::Entity::find()
        .filter(awdp_instances::Column::EventId.eq(event_id))
        .order_by_asc(awdp_instances::Column::CreatedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let mut out = Vec::with_capacity(exts.len());
    for ext in exts {
        let instance = instances::Entity::find_by_id(ext.instance_id)
            .one(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            .ok_or_else(|| AwdpError::Internal("instance row missing".into()))?;
        out.push((instance, ext));
    }
    Ok(out)
}
