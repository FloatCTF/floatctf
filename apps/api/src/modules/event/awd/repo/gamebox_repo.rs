//! GameBox instance repository — logical runtime instances (AWD 域).
//!
//! Instance 是「某场 Event 中一个 EventGameBox 为某个 Team 创建的稳定逻辑靶机」：
//! Docker Container 只是它的 current runtime realization（runtime_generation + 1）。
//! 全局 GameBox identity / Revision / EventGameBox 见 gamebox_lib_repo / event_gamebox_repo。
//!
//! 归一化根（event-instances-single-root）：逻辑实例的运行时字段（容器实现/镜像/代际/
//! 生命周期）统一落在 `event_instances` 根表；`event_gamebox_instances` 只存 AWD 领域
//! 状态（GameboxStatus / gamebox_ip / health）。所有查询统一经 `find_also_related`
//! 返回 `AwdInstanceRow` pair。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::{
    event_gamebox_instances, event_instances, sea_orm_active_enums::GameboxStatus,
};

/// 统一实例查询结果：`(event_gamebox_instances::Model, event_instances::Model)`。
/// 扩展表（AWD 领域状态：GameboxStatus/gamebox_ip/health）+ 归一化根
/// （容器实现/镜像/代际/生命周期）成对返回。所有 AWD 逻辑实例查询统一返回该 pair。
pub type AwdInstanceRow = (event_gamebox_instances::Model, event_instances::Model);

/// 防御性收敛 `find_also_related` 的 Option：FK 保证 instance_id → event_instances
/// 恒存在；孤儿行（数据异常）直接过滤。
fn to_pairs(
    rows: Vec<(
        event_gamebox_instances::Model,
        Option<event_instances::Model>,
    )>,
) -> Vec<AwdInstanceRow> {
    rows.into_iter()
        .filter_map(|(ext, root)| root.map(|r| (ext, r)))
        .collect()
}

// ── Instances ──

pub async fn find_instances_by_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Vec<AwdInstanceRow>, sea_orm::DbErr> {
    event_gamebox_instances::Entity::find()
        .filter(event_gamebox_instances::Column::EventId.eq(event_id))
        .order_by_asc(event_gamebox_instances::Column::GameboxIp)
        .find_also_related(event_instances::Entity)
        .all(db)
        .await
        .map(to_pairs)
}

pub async fn find_instances_by_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<Vec<AwdInstanceRow>, sea_orm::DbErr> {
    event_gamebox_instances::Entity::find()
        .filter(event_gamebox_instances::Column::EventId.eq(event_id))
        .filter(event_gamebox_instances::Column::TeamId.eq(team_id))
        .find_also_related(event_instances::Entity)
        .all(db)
        .await
        .map(to_pairs)
}

/// 按 gamebox_ip 查询（FlagServer 回调侧：只需领域状态/归属，不加载归一化根）。
pub async fn find_instance_by_ip(
    db: &DatabaseConnection,
    event_id: Uuid,
    ip: &str,
) -> Result<Option<event_gamebox_instances::Model>, sea_orm::DbErr> {
    event_gamebox_instances::Entity::find()
        .filter(event_gamebox_instances::Column::EventId.eq(event_id))
        .filter(event_gamebox_instances::Column::GameboxIp.eq(ip))
        .one(db)
        .await
}

pub async fn find_instance_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<AwdInstanceRow>, sea_orm::DbErr> {
    Ok(event_gamebox_instances::Entity::find_by_id(id)
        .find_also_related(event_instances::Entity)
        .one(db)
        .await?
        .and_then(|(ext, root)| root.map(|r| (ext, r))))
}

/// 查询某 EventGameBox × Team 的逻辑实例（幂等部署/对账用）。
pub async fn find_instance_by_event_gamebox_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    event_gamebox_id: Uuid,
    team_id: Uuid,
) -> Result<Option<AwdInstanceRow>, sea_orm::DbErr> {
    Ok(event_gamebox_instances::Entity::find()
        .filter(event_gamebox_instances::Column::EventId.eq(event_id))
        .filter(event_gamebox_instances::Column::EventGameboxId.eq(event_gamebox_id))
        .filter(event_gamebox_instances::Column::TeamId.eq(team_id))
        .find_also_related(event_instances::Entity)
        .one(db)
        .await?
        .and_then(|(ext, root)| root.map(|r| (ext, r))))
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
    let mut active: event_gamebox_instances::ActiveModel = event_gamebox_instances::ActiveModel {
        id: Set(id),
        status: Set(status),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

/// 更新归一化根的运行时实现（Deploy/Reset/Recovery 成功后调用）。
/// `event_instances`：container_id / runtime_state / runtime_generation（None 表示不变）。
/// 不改变逻辑身份（id / event_gamebox_id / team_id / gamebox_ip / container_name）。
pub async fn update_runtime_root(
    db: &DatabaseConnection,
    instance_id: Uuid,
    container_id: Option<&str>,
    runtime_state: &str,
    runtime_generation: Option<i64>,
) -> Result<(), sea_orm::DbErr> {
    let now = chrono::Utc::now().into();
    let mut am: event_instances::ActiveModel = event_instances::Entity::find_by_id(instance_id)
        .one(db)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound(format!("instance {instance_id} not found")))?
        .into();
    if let Some(cid) = container_id {
        am.container_id = Set(Some(cid.to_string()));
    }
    am.runtime_state = Set(runtime_state.to_string());
    if let Some(generation) = runtime_generation {
        am.runtime_generation = Set(generation);
    }
    am.updated_at = Set(now);
    am.update(db).await?;
    Ok(())
}
