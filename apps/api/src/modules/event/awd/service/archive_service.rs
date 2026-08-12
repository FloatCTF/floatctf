//! AWD 赛事归档服务。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::entity::{
    awd_wireguard_peers,
    sea_orm_active_enums::{AwdEventStatus, WgPeerStatus},
};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    infrastructure::network::{AwdNetworkRuntime, PeerIdentity},
    repo::{event_repo, gamebox_repo},
};
use fcmc::AwdContainerRuntime;

/// 完整归档：销毁 Docker 与宿主网络资源，保留数据库记录。
///
/// 宿主 WG/iptables 操作经 `AwdNetworkRuntime`（Host 或 Noop）。
/// 移除宿主 WG 失败会记日志，且仅当运行时自身返回错误时表现为 `AwdError::Network`
/// （Noop 恒成功；Host 可能失败）。
pub async fn archive_event(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn crate::modules::event::awd::infrastructure::firewall::FirewallRuntime,
    event_id: Uuid,
) -> AwdResult<()> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;
    let event_network =
        crate::modules::event::awd::repo::event_network_repo::find_by_event_id(db, event_id)
            .await?;

    if awd_event.status == AwdEventStatus::Archived {
        return Ok(());
    }
    if awd_event.status != AwdEventStatus::Finished {
        return Err(AwdError::InvalidState(
            "Can only archive a finished event".into(),
        ));
    }

    // 1. Stop and remove all GameBox containers
    // pair：容器实现/名称在归一化根（event_instances）。
    let instances = gamebox_repo::find_instances_by_event(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    for (_, root) in &instances {
        let target = root
            .container_id
            .as_deref()
            .unwrap_or(root.container_name.as_str());
        if let Err(e) = containers.stop_container(target).await {
            info!(
                "[Archive] stop {} ({}): {} — continuing remove",
                target, root.container_name, e
            );
        }
        if let Err(e) = containers.remove_container(target).await {
            info!(
                "[Archive] remove {} ({}): {} — continuing",
                target, root.container_name, e
            );
        }
    }

    // 2. Revoke peers on host (via runtime) then mark DB revoked
    let peers = awd_wireguard_peers::Entity::find()
        .filter(awd_wireguard_peers::Column::EventId.eq(event_id))
        .filter(awd_wireguard_peers::Column::Status.eq(WgPeerStatus::Active))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    for peer in &peers {
        if let Err(e) = network
            .revoke_peer(PeerIdentity {
                interface: event_network
                    .as_ref()
                    .map(|n| n.wireguard_interface_name.clone())
                    .unwrap_or_default(),
                public_key: peer.public_key.clone(),
            })
            .await
        {
            warn!(
                "[Archive] host revoke peer {} failed: {} — continuing DB mark",
                peer.public_key, e
            );
        }
        let mut active: awd_wireguard_peers::ActiveModel = peer.clone().into();
        active.status = Set(WgPeerStatus::Revoked);
        active.revoked_at = Set(Some(chrono::Utc::now().into()));
        active
            .update(db)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    // 3. Remove WireGuard interface via runtime
    let wg_iface = event_network
        .as_ref()
        .map(|n| n.wireguard_interface_name.clone())
        .unwrap_or_default();
    if let Err(e) = network.remove_wireguard(&wg_iface).await {
        // Host may fail if iface never existed; surface as network error for Host,
        // Noop never fails. We still continue cleanup of Docker network.
        warn!(
            "[Archive] remove_wireguard {}: {} — continuing",
            wg_iface, e
        );
    }

    // 4. Remove Docker network（Observed ID 属 awd_runtime_resources，§14）
    use crate::entity::awd_runtime_resources;
    let docker_network_id = awd_runtime_resources::Entity::find()
        .filter(awd_runtime_resources::Column::EventId.eq(event_id))
        .filter(awd_runtime_resources::Column::ResourceType.eq("docker_network"))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .map(|r| r.resource_id);
    if let Some(ref network_id) = docker_network_id {
        if let Err(e) = containers.remove_event_network(network_id).await {
            info!("[Archive] remove network {}: {}", network_id, e);
        }
    }

    // 5. Mark as archived（状态机唯一入口，Phase 0）
    event_repo::transition_event(
        db,
        awd_event.id,
        awd_event.status.clone(),
        AwdEventStatus::Archived,
        Default::default(),
    )
    .await?;

    // 6. P4-13 desired-state 清理：该赛事移出 managed active desired set → 全局 reconcile
    // （该赛事 sets/event chains 被清理）；若已无任何赛事 → 删除整个 floatctf_awd table。
    let remaining = crate::modules::event::awd::service::firewall_service::build_desired_state(
        db,
        crate::modules::event::awd::service::firewall_service::current_network_revision(db).await,
    )
    .await?;
    let revision =
        crate::modules::event::awd::service::firewall_service::next_network_revision(db).await?;
    if remaining.is_empty() {
        crate::modules::event::awd::service::firewall_service::reconcile_empty(firewall, revision)
            .await?;
    } else {
        crate::modules::event::awd::service::firewall_service::reconcile_global(
            db, firewall, revision,
        )
        .await?;
    }

    info!("[Archive] Event {} archived", event_id);
    Ok(())
}

/// 快速归档：仅标记已归档，不清理 Docker（需人工清理）。
pub async fn quick_archive(db: &DatabaseConnection, event_id: Uuid) -> AwdResult<()> {
    // 先按 event_id（外键）解析真实主键，再走状态机唯一入口（Phase 0）。
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;
    event_repo::transition_event(
        db,
        awd_event.id,
        awd_event.status.clone(),
        AwdEventStatus::Archived,
        Default::default(),
    )
    .await?;
    Ok(())
}
