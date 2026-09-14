//! PlatformNetworkService（§75）：平台级 AWD 网络——settings / host capability / allocations。
//!
//! 平台页只负责「全局默认、资源池、Host 健康、分配可见性」（§109）。
//! 赛事级网络操作回 Event 页面（EventNetworkService）。

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use super::super::domain::network::{Ipv4Cidr, NetworkPool, WireGuardPortRange};
use super::super::repo::network_allocation_repo;
use super::super::repo::network_settings_repo::{self, NetworkSettingsPatch};
use super::super::system::command::CommandRunner;
use super::super::{AwdError, AwdResult};
use crate::entity::events;

/// 读取平台网络设置（singleton）。
pub async fn get_settings(
    db: &DatabaseConnection,
) -> AwdResult<crate::entity::awd_network_settings::Model> {
    network_settings_repo::get(db).await
}

/// 更新平台网络设置（§10/§11/§31）：
/// - gamebox_pool 与 wireguard_pool 不允许重叠；
/// - 前缀顺序 pool ≤ event ≤ team；
/// - WG 端口范围合法；
/// - 容量显式计算（event/team/host/port）。
/// 设置变更只影响 future allocations；已分配的 Event 网络保持不变（§31/§32）。
pub async fn update_settings(
    db: &DatabaseConnection,
    patch: NetworkSettingsPatch,
) -> AwdResult<crate::entity::awd_network_settings::Model> {
    // 构造「候选设置」用于联合校验（未修改项取现值）
    let current = network_settings_repo::get(db).await?;

    let gb_pool_str = patch
        .gamebox_pool
        .clone()
        .unwrap_or(current.gamebox_pool.to_string());
    let wg_pool_str = patch
        .wireguard_pool
        .clone()
        .unwrap_or(current.wireguard_pool.to_string());
    let gb_event = patch
        .gamebox_event_prefix
        .unwrap_or(current.gamebox_event_prefix) as u8;
    let gb_team = patch
        .gamebox_team_prefix
        .unwrap_or(current.gamebox_team_prefix) as u8;
    let wg_event = patch
        .wireguard_event_prefix
        .unwrap_or(current.wireguard_event_prefix) as u8;
    let wg_team = patch
        .wireguard_team_prefix
        .unwrap_or(current.wireguard_team_prefix) as u8;
    let port_min = patch
        .wireguard_port_min
        .unwrap_or(current.wireguard_port_min) as u16;
    let port_max = patch
        .wireguard_port_max
        .unwrap_or(current.wireguard_port_max) as u16;

    // §10：前缀顺序 + 合法 CIDR
    let gb_pool = NetworkPool::new(Ipv4Cidr::parse(&gb_pool_str)?, gb_event, gb_team)?;
    let wg_pool = NetworkPool::new(Ipv4Cidr::parse(&wg_pool_str)?, wg_event, wg_team)?;

    // §10：两个池不允许重叠
    if gb_pool.pool.overlaps(&wg_pool.pool) {
        return Err(AwdError::Validation(format!(
            "gamebox_pool {} 与 wireguard_pool {} 重叠",
            gb_pool.pool.to_string(),
            wg_pool.pool.to_string()
        )));
    }

    // §29/§30：WG 端口范围
    WireGuardPortRange::new(port_min, port_max)?;

    network_settings_repo::update(db, patch).await
}

/// 平台 Host 状态（§4.1）：纯观测，FloatCTF 不允许在此切换 Docker backend /
/// 关闭 firewalld / 修改 firewalld。
pub async fn host_status(_db: &DatabaseConnection) -> AwdResult<PlatformHostStatus> {
    use super::super::infrastructure::firewall::env;

    let env_snap = env::discover_environment().await;
    let capability = env::check_host_capability().await;

    // WireGuard：内核模块 / 命令存在性（观测）
    let wg_healthy = std::path::Path::new("/sys/module/wireguard").exists() || {
        let runner = super::super::system::command::RealCommandRunner;
        CommandRunner::run(&runner, "wg", &["--version".to_string()])
            .await
            .map(|r| r.exit_code == 0)
            .unwrap_or(false)
    };

    // IPv4 转发（观测）
    let ipv4_forwarding = std::fs::read_to_string("/proc/sys/net/ipv4/ip_forward")
        .ok()
        .map(|s| s.trim().to_string());

    Ok(PlatformHostStatus {
        nftables: env_snap
            .nft_version
            .clone()
            .map(|v| format!("Healthy ({v})"))
            .unwrap_or_else(|| "Missing".to_string()),
        wireguard: if wg_healthy {
            "Healthy".to_string()
        } else {
            "Missing".to_string()
        },
        docker: if ipv4_forwarding.is_some() {
            "Available".to_string()
        } else {
            "Unknown".to_string()
        },
        firewall_runtime: "native nftables".to_string(),
        floatctf_table: "inet floatctf_awd".to_string(),
        docker_firewall_backend: env_snap.docker_firewall_backend.clone(),
        firewalld: if env_snap.firewalld_active {
            "active".to_string()
        } else {
            "inactive".to_string()
        },
        ipv4_forwarding: ipv4_forwarding.map(|v| {
            if v == "1" {
                "enabled".into()
            } else {
                "disabled".into()
            }
        }),
        ipv6_policy: "blocked".to_string(), // AWD v6 无路由 + 规则默认 drop（§dda2c98 决策）
        capability_supported: matches!(capability, Ok(env::HostNetworkCapability::Supported)),
        notes: env_snap.notes.clone(),
    })
}

/// 平台分配可见性（§7/§66）：Event 的独占 CIDR 账本（active 优先）。
pub async fn allocations_view(db: &DatabaseConnection) -> AwdResult<Vec<PlatformAllocation>> {
    let allocations = network_allocation_repo::list_all(db).await?;

    let event_ids: Vec<uuid::Uuid> = allocations.iter().map(|a| a.event_id).collect();
    let events_map: std::collections::HashMap<uuid::Uuid, String> = events::Entity::find()
        .filter(events::Column::Id.is_in(event_ids))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .into_iter()
        .map(|e| (e.id, e.title))
        .collect();

    Ok(allocations
        .into_iter()
        .map(|a| PlatformAllocation {
            event_id: a.event_id,
            event_title: events_map.get(&a.event_id).cloned(),
            kind: format!("{:?}", a.kind).to_lowercase(),
            cidr: a.cidr.to_string(),
            allocated_at: a.allocated_at.to_rfc3339(),
            released_at: a.released_at.map(|t| t.to_rfc3339()),
            active: a.released_at.is_none(),
        })
        .collect())
}

/// Host 状态响应（观测信息，非密钥）。
#[derive(Debug, serde::Serialize)]
pub struct PlatformHostStatus {
    pub nftables: String,
    pub wireguard: String,
    pub docker: String,
    pub firewall_runtime: String,
    pub floatctf_table: String,
    pub docker_firewall_backend: Option<String>,
    pub firewalld: String,
    pub ipv4_forwarding: Option<String>,
    pub ipv6_policy: String,
    pub capability_supported: bool,
    pub notes: Vec<String>,
}

/// 平台分配行（§7/§66）。
#[derive(Debug, serde::Serialize)]
pub struct PlatformAllocation {
    pub event_id: uuid::Uuid,
    pub event_title: Option<String>,
    pub kind: String,
    pub cidr: String,
    pub allocated_at: String,
    pub released_at: Option<String>,
    pub active: bool,
}
