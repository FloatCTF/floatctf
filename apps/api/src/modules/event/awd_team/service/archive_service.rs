//! Archive service — clean up resources after event retention period.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::entity::{
    awd_gamebox_instances, awd_wireguard_peers,
    sea_orm_active_enums::{AwdEventStatus, WgPeerStatus},
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    infrastructure::network::{AwdNetworkRuntime, PeerIdentity},
    repo::event_repo,
};
use fcmc::AwdContainerRuntime;

/// Full archive: destroy Docker + host network resources, keep DB records.
///
/// Host WG/iptables operations go through `AwdNetworkRuntime` (Host or Noop).
/// Failure to remove host WG is logged and surfaces as `AwdError::Network` only when
/// the runtime itself returns an error (Noop always succeeds; Host may fail).
pub async fn archive_event(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn crate::modules::event::awd_team::infrastructure::firewall::FirewallRuntime,
    event_id: Uuid,
) -> AwdResult<()> {
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if awd_event.status == AwdEventStatus::Archived {
        return Ok(());
    }
    if awd_event.status != AwdEventStatus::Finished {
        return Err(AwdError::InvalidState(
            "Can only archive a finished event".into(),
        ));
    }

    // 1. Stop and remove all GameBox containers
    let instances = awd_gamebox_instances::Entity::find()
        .filter(awd_gamebox_instances::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    for instance in &instances {
        let target = instance
            .current_container_id
            .as_deref()
            .unwrap_or(instance.container_name.as_str());
        if let Err(e) = containers.stop_container(target).await {
            info!(
                "[Archive] stop {} ({}): {} — continuing remove",
                target, instance.container_name, e
            );
        }
        if let Err(e) = containers.remove_container(target).await {
            info!(
                "[Archive] remove {} ({}): {} — continuing",
                target, instance.container_name, e
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
                interface: awd_event.wireguard_interface_name.clone(),
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
    if let Err(e) = network
        .remove_wireguard(&awd_event.wireguard_interface_name)
        .await
    {
        // Host may fail if iface never existed; surface as network error for Host,
        // Noop never fails. We still continue cleanup of Docker network.
        warn!(
            "[Archive] remove_wireguard {}: {} — continuing",
            awd_event.wireguard_interface_name, e
        );
    }

    // 4. Remove Docker network
    if let Some(ref network_id) = awd_event.docker_network_id {
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
    let remaining =
        crate::modules::event::awd_team::service::firewall_service::build_desired_state(
            db,
            crate::modules::event::awd_team::service::firewall_service::current_network_revision(
                db,
            )
            .await,
        )
        .await?;
    let revision =
        crate::modules::event::awd_team::service::firewall_service::next_network_revision(db)
            .await?;
    if remaining.is_empty() {
        crate::modules::event::awd_team::service::firewall_service::reconcile_empty(
            firewall, revision,
        )
        .await?;
    } else {
        crate::modules::event::awd_team::service::firewall_service::reconcile_global(
            db, firewall, revision,
        )
        .await?;
    }

    info!("[Archive] Event {} archived", event_id);
    Ok(())
}

/// Quick archive: just mark as archived without cleaning Docker (manual cleanup).
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
