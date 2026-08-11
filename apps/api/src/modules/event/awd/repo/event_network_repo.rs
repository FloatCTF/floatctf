//! awd_event_networks：一场 AWD Event 已分配并固化的网络配置（Desired State）。

use crate::entity::{awd_event_networks, sea_orm_active_enums::AwdNetworkAllocationMode};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter,
};
use uuid::Uuid;

use super::super::AwdError;

/// 字符串 → PG 原生 CIDR/INET 值（sqlx 以 IpNetwork 读写，§20）。
fn parse_ipnet(v: &str) -> Result<ipnetwork::IpNetwork, AwdError> {
    v.parse()
        .map_err(|e| AwdError::Validation(format!("invalid cidr/ip {v}: {e}")))
}
use super::event_repo;

pub async fn find_by_event_id(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Option<awd_event_networks::Model>, AwdError> {
    awd_event_networks::Entity::find()
        .filter(awd_event_networks::Column::EventId.eq(event_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

pub async fn require_by_event_id(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<awd_event_networks::Model, AwdError> {
    find_by_event_id(db, event_id)
        .await?
        .ok_or_else(|| AwdError::NotFound(format!("event network for {event_id}")))
}

/// 列出所有已固化的 Event Network（全量 desired state 构建用）。
pub async fn list_all<C: ConnectionTrait + Send>(
    db: &C,
) -> Result<Vec<awd_event_networks::Model>, AwdError> {
    awd_event_networks::Entity::find()
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

/// 已锁定的 Event Network（Deploy 之后 addressing 不可变）。
pub async fn list_locked(
    db: &DatabaseConnection,
) -> Result<Vec<awd_event_networks::Model>, AwdError> {
    awd_event_networks::Entity::find()
        .filter(awd_event_networks::Column::LockedAt.is_not_null())
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

pub struct NewEventNetwork {
    pub event_id: Uuid,
    pub allocation_mode: AwdNetworkAllocationMode,
    pub gamebox_cidr: String,
    pub wireguard_cidr: String,
    pub infrastructure_subnet: String,
    pub flagserver_ip: String,
    pub judgeserver_ip: String,
    pub wireguard_interface_name: String,
    pub wireguard_listen_port: i32,
    pub docker_network_name: String,
    pub locked_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

/// 在事务内插入 Event Network（配合分配器事务使用）。
pub async fn create_in_tx<C>(
    tx: &C,
    net: NewEventNetwork,
) -> Result<awd_event_networks::Model, AwdError>
where
    C: ConnectionTrait,
{
    awd_event_networks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(net.event_id),
        allocation_mode: Set(net.allocation_mode),
        gamebox_cidr: Set(parse_ipnet(&net.gamebox_cidr)?),
        wireguard_cidr: Set(parse_ipnet(&net.wireguard_cidr)?),
        infrastructure_subnet: Set(parse_ipnet(&net.infrastructure_subnet)?),
        flagserver_ip: Set(parse_ipnet(&net.flagserver_ip)?),
        judgeserver_ip: Set(parse_ipnet(&net.judgeserver_ip)?),
        wireguard_interface_name: Set(net.wireguard_interface_name),
        wireguard_listen_port: Set(net.wireguard_listen_port),
        docker_network_name: Set(net.docker_network_name),
        locked_at: Set(net.locked_at),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(tx)
    .await
    .map_err(|e| AwdError::Database(e.to_string()))
}

/// 更新 Event Network 的非锁定字段（allocation 变更 / locked_at）。
/// 已锁定（locked_at 非空）时只允许变更 locked_at 本身。
pub async fn update_in_tx<C>(
    tx: &C,
    current: &awd_event_networks::Model,
    patch: EventNetworkPatch,
) -> Result<awd_event_networks::Model, AwdError>
where
    C: ConnectionTrait,
{
    let mut am: awd_event_networks::ActiveModel = current.clone().into();
    if let Some(mode) = patch.allocation_mode {
        am.allocation_mode = Set(mode);
    }
    if let Some(cidr) = patch.gamebox_cidr {
        am.gamebox_cidr = Set(parse_ipnet(&cidr)?);
    }
    if let Some(cidr) = patch.wireguard_cidr {
        am.wireguard_cidr = Set(parse_ipnet(&cidr)?);
    }
    if let Some(subnet) = patch.infrastructure_subnet {
        am.infrastructure_subnet = Set(parse_ipnet(&subnet)?);
    }
    if let Some(ip) = patch.flagserver_ip {
        am.flagserver_ip = Set(parse_ipnet(&ip)?);
    }
    if let Some(ip) = patch.judgeserver_ip {
        am.judgeserver_ip = Set(parse_ipnet(&ip)?);
    }
    if let Some(name) = patch.wireguard_interface_name {
        am.wireguard_interface_name = Set(name);
    }
    if let Some(port) = patch.wireguard_listen_port {
        am.wireguard_listen_port = Set(port);
    }
    if let Some(name) = patch.docker_network_name {
        am.docker_network_name = Set(name);
    }
    if let Some(locked) = patch.locked_at {
        am.locked_at = Set(Some(locked));
    }
    if let Some(unlock) = patch.unlock {
        if unlock {
            am.locked_at = Set(None);
        }
    }
    am.updated_at = Set(chrono::Utc::now().into());
    am.update(tx)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

pub struct EventNetworkPatch {
    pub allocation_mode: Option<AwdNetworkAllocationMode>,
    pub gamebox_cidr: Option<String>,
    pub wireguard_cidr: Option<String>,
    pub infrastructure_subnet: Option<String>,
    pub flagserver_ip: Option<String>,
    pub judgeserver_ip: Option<String>,
    pub wireguard_interface_name: Option<String>,
    pub wireguard_listen_port: Option<i32>,
    pub docker_network_name: Option<String>,
    pub locked_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub unlock: Option<bool>,
}

impl Default for EventNetworkPatch {
    fn default() -> Self {
        Self {
            allocation_mode: None,
            gamebox_cidr: None,
            wireguard_cidr: None,
            infrastructure_subnet: None,
            flagserver_ip: None,
            judgeserver_ip: None,
            wireguard_interface_name: None,
            wireguard_listen_port: None,
            docker_network_name: None,
            locked_at: None,
            unlock: None,
        }
    }
}

/// 校验 Event 是否处于「网络可编辑」状态（Draft/Configuring，§34/§49/§50）。
/// 需要读取 awd_events 状态，因此依赖 event_repo 查询。
pub fn assert_network_editable(
    status: &crate::entity::sea_orm_active_enums::AwdEventStatus,
) -> Result<(), AwdError> {
    use crate::entity::sea_orm_active_enums::AwdEventStatus;
    match status {
        AwdEventStatus::Draft | AwdEventStatus::Configuring => Ok(()),
        other => Err(AwdError::NetworkLocked(format!(
            "network addressing locked in status {:?}（仅 Draft/Configuring 可改）",
            other
        ))),
    }
}

/// event_repo 辅助：查找 awd_events 行（复用现有 repo 以避免重复实现）。
pub async fn find_awd_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<crate::entity::awd_events::Model, AwdError> {
    event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound(format!("awd event {event_id}")))
}
