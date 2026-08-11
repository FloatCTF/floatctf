//! GameBox instance repository — logical runtime instances (AWD 域).
//!
//! Instance 是「某场 Event 中一个 EventGameBox 为某个 Team 创建的稳定逻辑靶机」：
//! Docker Container 只是它的 current runtime realization（runtime_generation + 1）。
//! 全局 GameBox identity / Revision / EventGameBox 见 gamebox_lib_repo / event_gamebox_repo。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::{awd_gamebox_instances, sea_orm_active_enums::GameboxStatus};

// ── Instances ──

pub async fn find_instances_by_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Vec<awd_gamebox_instances::Model>, sea_orm::DbErr> {
    awd_gamebox_instances::Entity::find()
        .filter(awd_gamebox_instances::Column::EventId.eq(event_id))
        .order_by_asc(awd_gamebox_instances::Column::GameboxIp)
        .all(db)
        .await
}

pub async fn find_instances_by_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<Vec<awd_gamebox_instances::Model>, sea_orm::DbErr> {
    awd_gamebox_instances::Entity::find()
        .filter(awd_gamebox_instances::Column::EventId.eq(event_id))
        .filter(awd_gamebox_instances::Column::TeamId.eq(team_id))
        .all(db)
        .await
}

pub async fn find_instance_by_ip(
    db: &DatabaseConnection,
    event_id: Uuid,
    ip: &str,
) -> Result<Option<awd_gamebox_instances::Model>, sea_orm::DbErr> {
    awd_gamebox_instances::Entity::find()
        .filter(awd_gamebox_instances::Column::EventId.eq(event_id))
        .filter(awd_gamebox_instances::Column::GameboxIp.eq(ip))
        .one(db)
        .await
}

pub async fn find_instance_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<awd_gamebox_instances::Model>, sea_orm::DbErr> {
    awd_gamebox_instances::Entity::find_by_id(id).one(db).await
}

/// 查询某 EventGameBox × Team 的逻辑实例（幂等部署/对账用）。
pub async fn find_instance_by_event_gamebox_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    event_gamebox_id: Uuid,
    team_id: Uuid,
) -> Result<Option<awd_gamebox_instances::Model>, sea_orm::DbErr> {
    awd_gamebox_instances::Entity::find()
        .filter(awd_gamebox_instances::Column::EventId.eq(event_id))
        .filter(awd_gamebox_instances::Column::EventGameboxId.eq(event_gamebox_id))
        .filter(awd_gamebox_instances::Column::TeamId.eq(team_id))
        .one(db)
        .await
}

/// 查询队伍网络分配（ban conntrack 清理用）。
pub async fn find_team_network(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<Option<crate::entity::awd_team_networks::Model>, sea_orm::DbErr> {
    crate::entity::awd_team_networks::Entity::find()
        .filter(crate::entity::awd_team_networks::Column::EventId.eq(event_id))
        .filter(crate::entity::awd_team_networks::Column::TeamId.eq(team_id))
        .one(db)
        .await
}

pub async fn update_instance_status(
    db: &DatabaseConnection,
    id: Uuid,
    status: GameboxStatus,
) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_gamebox_instances::ActiveModel = awd_gamebox_instances::ActiveModel {
        id: Set(id),
        status: Set(status),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

/// 更新当前容器（Reset/Deploy 成功后调用）。不改变逻辑身份。
pub async fn set_instance_container(
    db: &DatabaseConnection,
    id: Uuid,
    container_id: &str,
    status: GameboxStatus,
) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_gamebox_instances::ActiveModel = awd_gamebox_instances::ActiveModel {
        id: Set(id),
        current_container_id: Set(Some(container_id.to_string())),
        status: Set(status),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

/// Reset 成功后：替换 current container + runtime_generation + 1（§17/§20/§24）。
/// logical identity（id / event_gamebox_id / team_id / gamebox_ip）不变。
pub async fn bump_runtime_generation(
    db: &DatabaseConnection,
    id: Uuid,
    container_id: &str,
) -> Result<i64, sea_orm::DbErr> {
    let inst = find_instance_by_id(db, id)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound(format!("instance {id} not found")))?;
    let next_gen = inst.runtime_generation + 1;
    let mut active: awd_gamebox_instances::ActiveModel = awd_gamebox_instances::ActiveModel {
        id: Set(id),
        current_container_id: Set(Some(container_id.to_string())),
        runtime_generation: Set(next_gen),
        status: Set(GameboxStatus::Ready),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(next_gen)
}
