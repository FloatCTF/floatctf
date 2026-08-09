//! Team Network 分配器（§36-39/§74）：Event 添加 Team 时自动派生并持久化子网。
//!
//! - 已有 awd_team_networks 行永远复用（持久化 subnet 为事实，§38）；
//! - 新 Team 取第一个空闲 slot（index 0 = infra 保留）；
//! - used_indexes 读取「全部」行（含 status='released'），
//!   同一 Event 生命周期内不复用已释放的 slot（§39）。
//! - 禁止依赖 Team name / order 分配（§38 反模式）。

use crate::entity::{awd_team_networks, event_teams};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};

use super::super::crypto::AwdCrypto;
use super::super::domain::network::{Ipv4Cidr, TeamSubnetAllocator};
use super::super::repo::event_network_repo;
use super::super::{AwdError, AwdResult};

/// 确保 Event 下全部 Team 都有持久化的 gamebox/wireguard 子网。
/// Deploy 前调用（Configuring→Deploying 前，§50/§74）。
/// 已部署事件的 Team 分配不会变化（§95：rename/新增不影响已有子网）。
pub async fn ensure_team_networks(
    db: &DatabaseConnection,
    event_id: uuid::Uuid,
    crypto: &AwdCrypto,
    key_version: i32,
) -> AwdResult<Vec<awd_team_networks::Model>> {
    let net = event_network_repo::require_by_event_id(db, event_id).await?;
    let gb_cidr = Ipv4Cidr::parse(&net.gamebox_cidr.to_string())?;
    let wg_cidr = Ipv4Cidr::parse(&net.wireguard_cidr.to_string())?;
    // team prefix 从已固化的 infrastructure_subnet 前缀推导（与分配时一致）
    let team_prefix = Ipv4Cidr::parse(&net.infrastructure_subnet.to_string())?.prefix_len;

    // §38：确定性枚举（created_at + id 兜底；仅决定「处理顺序」，不决定子网号）
    let teams = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .order_by_asc(event_teams::Column::CreatedAt)
        .order_by_asc(event_teams::Column::Id)
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let existing = awd_team_networks::Entity::find()
        .filter(awd_team_networks::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    // 已分配过的 slot（含 released 行）→ 生命周期内不复用（§39）
    let mut used_indexes: Vec<u64> = existing.iter().map(|t| t.subnet_index as u64).collect();

    let txn = db
        .begin()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let mut result = Vec::with_capacity(teams.len());
    for team in &teams {
        if let Some(n) = existing.iter().find(|n| n.team_id == team.id) {
            result.push(n.clone());
            continue;
        }

        // 每次循环基于「当前已用 slot」计算：新插入的 team 立即计入 used，
        // 避免多个新 team 拿到同一 index（§38 稳定唯一）。块作用域结束借用。
        let index = {
            let allocator = TeamSubnetAllocator {
                event_cidr: &gb_cidr,
                team_prefix,
                used_indexes: &used_indexes,
            };
            allocator.next_free_index().ok_or_else(|| {
                AwdError::PoolExhausted(format!(
                    "AWD_TEAM_SUBNET_EXHAUSTED: Event {event_id} 的 team 子网容量已满"
                ))
            })?
        };
        used_indexes.push(index);
        let gb_subnet = gb_cidr
            .nth_subnet(team_prefix, index)
            .ok_or_else(|| AwdError::Internal(format!("team subnet for index {index} 派生失败")))?;
        let wg_subnet = wg_cidr
            .nth_subnet(team_prefix, index)
            .ok_or_else(|| AwdError::Internal(format!("wg subnet for index {index} 派生失败")))?;

        let ssh_password = random_password(16);
        let aad = AwdCrypto::build_aad(event_id, "ssh_password");
        let blob = crypto
            .encrypt(ssh_password.as_bytes(), &aad, key_version)
            .map_err(|e| AwdError::Crypto(e.to_string()))?;

        let model = awd_team_networks::ActiveModel {
            id: Set(uuid::Uuid::new_v4()),
            event_id: Set(event_id),
            team_id: Set(team.id),
            subnet_index: Set(index as i16),
            gamebox_subnet: Set(gb_subnet.to_ipnetwork()),
            wireguard_subnet: Set(wg_subnet.to_ipnetwork()),
            ssh_password_ciphertext: Set(blob.ciphertext),
            ssh_password_nonce: Set(blob.nonce),
            key_version: Set(key_version),
            next_wireguard_host: Set(2),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

        result.push(model);
    }

    txn.commit()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(result)
}

fn random_password(len: usize) -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}
