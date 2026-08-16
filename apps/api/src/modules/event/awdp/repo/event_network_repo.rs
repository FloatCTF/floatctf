//! awdp_event_networks 仓储：每赛事独立 Docker 网络资源（子网分配 / judge 固定 IP / 生命周期）。
//!
//! 练习虚拟赛事（AWDPlusPractice）不落此表——沿用 config.awdp 固定网络，见
//! `domain::judge::is_practice_event`。

use chrono::Utc;
use ipnetwork::{IpNetwork, Ipv4Network};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use std::net::Ipv4Addr;
use uuid::Uuid;

use crate::entity::awdp_event_networks;
use crate::modules::event::awdp::{AwdpError, AwdpResult};

pub const STATUS_ALLOCATED: &str = "allocated";
pub const STATUS_DEPLOYED: &str = "deployed";
pub const STATUS_RELEASED: &str = "released";

pub async fn find_by_event_id(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Option<awdp_event_networks::Model>> {
    awdp_event_networks::Entity::find_by_id(event_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

pub async fn require_by_event_id(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<awdp_event_networks::Model> {
    find_by_event_id(db, event_id)
        .await?
        .ok_or_else(|| AwdpError::NotFound("awdp event network not allocated".into()))
}

pub async fn list_all(db: &DatabaseConnection) -> AwdpResult<Vec<awdp_event_networks::Model>> {
    awdp_event_networks::Entity::find()
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

// ────────────────────────────────────────────────────────────────────────────
// 子网分配（纯函数，可测）
// ────────────────────────────────────────────────────────────────────────────

/// 两个 IPv4 网络是否重叠（同族；CIDR 语义包含）。
fn v4_overlaps(a: &Ipv4Network, b: &IpNetwork) -> bool {
    match b {
        IpNetwork::V4(v4) => a.contains(v4.ip()) || v4.contains(a.ip()),
        IpNetwork::V6(_) => false,
    }
}

/// 从池中取首个与现有行/已占用子网不重叠的 `event_netmask` 大小子网。
/// 池必须是 IPv4 且 `event_netmask >= pool 前缀长度`。
/// `docker_subnets`：docker 实际已存在的网络子网（DB 被删但网络残留时避免撞池）。
pub fn pick_subnet(
    pool: &IpNetwork,
    event_netmask: u8,
    existing: &[awdp_event_networks::Model],
    docker_subnets: &[IpNetwork],
) -> Option<Ipv4Network> {
    let pool_v4 = match pool {
        IpNetwork::V4(v4) => *v4,
        IpNetwork::V6(_) => return None,
    };
    if event_netmask < pool_v4.prefix() || event_netmask > 32 {
        return None;
    }
    let pool_base = u32::from(pool_v4.network());
    let block_size = 1u32 << (32 - event_netmask);
    let block_count = 1u32 << (event_netmask - pool_v4.prefix());
    for i in 0..block_count {
        let base = pool_base + i * block_size;
        let cand = Ipv4Network::new(Ipv4Addr::from(base), event_netmask).ok()?;
        let overlaps_db = existing.iter().any(|m| v4_overlaps(&cand, &m.subnet_cidr));
        let overlaps_docker = docker_subnets.iter().any(|s| v4_overlaps(&cand, s));
        if !overlaps_db && !overlaps_docker {
            return Some(cand);
        }
    }
    None
}

/// 子网后半段动态池（如 /24 → .128/25，与旧 practice 网络推导一致）。
pub fn dynamic_pool_for(subnet: &Ipv4Network) -> IpNetwork {
    let prefix = subnet.prefix();
    let pool_prefix = prefix + 1;
    let host_half = 1u32 << (31 - prefix);
    let pool_net = u32::from(subnet.network()) + host_half;
    IpNetwork::V4(
        Ipv4Network::new(Ipv4Addr::from(pool_net), pool_prefix).expect("pool prefix <= 32"),
    )
}

/// 子网内 JudgeServer 固定 IP：network + 2（避开 .1 网关；位于动态池外）。
pub fn judge_ip_for(subnet: &Ipv4Network) -> IpNetwork {
    let ip = subnet.nth(2).unwrap_or(subnet.network());
    IpNetwork::V4(Ipv4Network::new(ip, 32).expect("host prefix 32"))
}

// ────────────────────────────────────────────────────────────────────────────
// 生命周期
// ────────────────────────────────────────────────────────────────────────────

/// 分配赛事网络资源（幂等：已分配直接返回；并发冲突重读返回胜出行）。
/// `docker_subnets`：docker 实际已存在的网络子网（防 DB 删后网络残留撞池）。
pub async fn allocate_event_network(
    db: &DatabaseConnection,
    event_id: Uuid,
    network_name: &str,
    pool: &IpNetwork,
    event_netmask: u8,
    docker_subnets: &[IpNetwork],
) -> AwdpResult<awdp_event_networks::Model> {
    if let Some(m) = find_by_event_id(db, event_id).await? {
        return Ok(m);
    }
    let existing = list_all(db).await?;
    let subnet = pick_subnet(pool, event_netmask, &existing, docker_subnets)
        .ok_or_else(|| AwdpError::Internal("awdp event network pool exhausted".into()))?;
    let dynamic_pool = dynamic_pool_for(&subnet);
    let judge_ip = judge_ip_for(&subnet);
    let now = Utc::now();
    let am = awdp_event_networks::ActiveModel {
        event_id: Set(event_id),
        network_name: Set(network_name.to_string()),
        subnet_cidr: Set(IpNetwork::V4(subnet)),
        dynamic_pool_cidr: Set(dynamic_pool),
        judge_ip: Set(judge_ip),
        docker_network_id: Set(None),
        status: Set(STATUS_ALLOCATED.to_string()),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
    };
    match am.insert(db).await {
        Ok(m) => Ok(m),
        Err(e) => {
            // 并发分配冲突（PK/UNIQUE）→ 重读胜出行。
            if e.to_string().contains("duplicate key") {
                if let Some(m) = find_by_event_id(db, event_id).await? {
                    return Ok(m);
                }
            }
            Err(AwdpError::Database(e.to_string()))
        }
    }
}

/// 网络创建成功后：记录真实 docker network id + status=deployed。
///
/// 注意：**不要**用 `Model.into()` 转 ActiveModel——SeaORM 宏生成的转换把所有字段
/// 标为 `Unchanged`，`prepare_values` 收集不到 Set 字段 → `is_noop()` 跳过 UPDATE
/// （只回读 SELECT，返回 Ok 但数据库不变）。必须显式 `Set` 要更新的字段，
/// 其余字段保持 `NotSet`（不进入 SET 子句）。
pub async fn mark_deployed(
    db: &DatabaseConnection,
    event_id: Uuid,
    docker_network_id: &str,
) -> AwdpResult<()> {
    let now = Utc::now();
    let am = awdp_event_networks::ActiveModel {
        event_id: Set(event_id),
        docker_network_id: Set(Some(docker_network_id.to_string())),
        status: Set(STATUS_DEPLOYED.to_string()),
        updated_at: Set(now.into()),
        ..Default::default()
    };
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    tracing::info!(event_id = %event_id, "[EventNetwork] mark_deployed done");
    Ok(())
}

/// 清理完成后：status=released（子网归还池；行保留供审计）。
///
/// 同上：显式 `Set` 更新字段，避免 `Model.into()` 的 Unchanged 陷阱（UPDATE 被跳过）。
pub async fn mark_released(db: &DatabaseConnection, event_id: Uuid) -> AwdpResult<()> {
    let now = Utc::now();
    let am = awdp_event_networks::ActiveModel {
        event_id: Set(event_id),
        docker_network_id: Set(None),
        status: Set(STATUS_RELEASED.to_string()),
        updated_at: Set(now.into()),
        ..Default::default()
    };
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(subnet: &str) -> awdp_event_networks::Model {
        use sea_orm::entity::prelude::*;
        awdp_event_networks::Model {
            event_id: Uuid::new_v4(),
            network_name: format!("fctf-awdp-test-{}", subnet.replace('/', "-")),
            subnet_cidr: subnet.parse().unwrap(),
            dynamic_pool_cidr: "10.42.0.128/25".parse().unwrap(),
            judge_ip: "10.42.0.2".parse().unwrap(),
            docker_network_id: None,
            status: STATUS_DEPLOYED.to_string(),
            created_at: Utc::now().into(),
            updated_at: Utc::now().into(),
        }
    }

    #[test]
    fn pick_first_free_block() {
        let pool: IpNetwork = "10.42.0.0/16".parse().unwrap();
        let free = pick_subnet(&pool, 24, &[], &[]).unwrap();
        assert_eq!(free.to_string(), "10.42.0.0/24");
    }

    #[test]
    fn pick_skips_overlapping_blocks() {
        let pool: IpNetwork = "10.42.0.0/16".parse().unwrap();
        let existing = vec![row("10.42.0.0/24"), row("10.42.1.0/24")];
        let free = pick_subnet(&pool, 24, &existing, &[]).unwrap();
        assert_eq!(free.to_string(), "10.42.2.0/24");
    }

    #[test]
    fn pick_skips_docker_residual_subnets() {
        // DB 行被删但 docker 网络残留：分配器必须跳过实际占用子网。
        let pool: IpNetwork = "10.43.0.0/16".parse().unwrap();
        let docker_subnets: Vec<IpNetwork> = vec!["10.43.0.0/24".parse().unwrap()];
        let free = pick_subnet(&pool, 24, &[], &docker_subnets).unwrap();
        assert_eq!(free.to_string(), "10.43.1.0/24");
    }

    #[test]
    fn dynamic_pool_and_judge_ip_follow_old_practice_layout() {
        let subnet: Ipv4Network = "10.42.2.0/24".parse().unwrap();
        let pool = dynamic_pool_for(&subnet);
        assert_eq!(pool.to_string(), "10.42.2.128/25");
        let judge = judge_ip_for(&subnet);
        assert_eq!(judge.ip().to_string(), "10.42.2.2");
        // judge 在子网内、动态池外。
        assert!(!pool.contains(judge.ip()));
    }

    #[test]
    fn non_24_subnets_also_derive_pool() {
        // /22 = 1024 地址；后半段 512 个从 network+512 开始。
        let subnet: Ipv4Network = "10.42.0.0/22".parse().unwrap();
        let pool = dynamic_pool_for(&subnet);
        assert_eq!(pool.to_string(), "10.42.2.0/23");
        let judge = judge_ip_for(&subnet);
        assert_eq!(judge.ip().to_string(), "10.42.0.2");
    }
}
