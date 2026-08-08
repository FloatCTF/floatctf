//! Startup recovery — reconcile platform state with Docker after restart.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::entity::{
    awd_events, awd_gamebox_instances,
    sea_orm_active_enums::{AwdEventStatus, GameboxStatus},
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    crypto::{AwdCrypto, EncryptedBlob},
    domain::Ipv4Cidr,
    infrastructure::{
        firewall::FirewallRuntime,
        network::{AwdNetworkRuntime, WireGuardDesiredState},
    },
    repo::event_repo,
    service::firewall_service,
};
use fcmc::AwdContainerRuntime;

/// Run recovery for all active AWD events on startup.
pub async fn recover_all(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    crypto: &AwdCrypto,
) -> AwdResult<u32> {
    let active_events = event_repo::find_active_events(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let mut recovered = 0u32;

    for event in &active_events {
        match recover_event(db, containers, network, firewall, crypto, event).await {
            Ok(n) => {
                recovered += n;
                info!(
                    "[Recovery] Event {} recovered {} resources",
                    event.event_id, n
                );
            }
            Err(e) => {
                error!(
                    "[Recovery] Failed to recover event {}: {}",
                    event.event_id, e
                );
            }
        }
    }

    info!(
        "[Recovery] Complete. {}/{} events processed, {} resources recovered.",
        recovered,
        active_events.len(),
        recovered
    );

    Ok(recovered)
}

/// Recover a single AWD event's resources.
async fn recover_event(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    crypto: &AwdCrypto,
    event: &awd_events::Model,
) -> AwdResult<u32> {
    let mut recovered = 0u32;

    // ── 1. Reconcile GameBox containers ──
    let live = containers
        .list_event_containers(event.event_id)
        .await
        .map_err(|e| AwdError::Docker(e.to_string()))?;
    let live_by_name: std::collections::HashMap<String, &fcmc::ContainerState> =
        live.iter().map(|c| (c.container_name.clone(), c)).collect();

    let instances = crate::modules::event::awd_team::repo::gamebox_repo::find_instances_by_event(
        db,
        event.event_id,
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;

    for instance in &instances {
        match live_by_name.get(&instance.container_name) {
            Some(state) if state.running => {
                if instance.container_id.as_deref() != Some(state.container_id.as_str()) {
                    let mut active: awd_gamebox_instances::ActiveModel =
                        awd_gamebox_instances::ActiveModel {
                            id: Set(instance.id),
                            container_id: Set(Some(state.container_id.clone())),
                            status: Set(GameboxStatus::Ready),
                            ..Default::default()
                        };
                    active
                        .update(db)
                        .await
                        .map_err(|e| AwdError::Database(e.to_string()))?;
                }
                recovered += 1;
            }
            Some(_) | None => {
                if matches!(
                    instance.status,
                    GameboxStatus::Ready
                        | GameboxStatus::Running
                        | GameboxStatus::Creating
                        | GameboxStatus::Pending
                ) {
                    warn!(
                        "[Recovery] Instance {} ({}) missing in Docker — marking missing",
                        instance.id, instance.container_name
                    );
                    crate::modules::event::awd_team::repo::gamebox_repo::update_instance_status(
                        db,
                        instance.id,
                        GameboxStatus::Missing,
                    )
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                    recovered += 1;
                }
            }
        }
    }

    // ── 2. Restore WireGuard interface from encrypted key material ──
    if let (Some(ct), Some(nonce)) = (
        &event.wg_server_private_key_ciphertext,
        &event.wg_server_private_key_nonce,
    ) {
        let blob = EncryptedBlob {
            ciphertext: ct.clone(),
            nonce: nonce.clone(),
            key_version: event.key_version,
        };
        let aad = AwdCrypto::build_aad(event.event_id, "wg_server_private_key");
        match crypto.decrypt(&blob, &aad) {
            Ok(pk_bytes) => {
                let private_key =
                    String::from_utf8(pk_bytes).map_err(|e| AwdError::Crypto(e.to_string()))?;
                let prefix = event.wireguard_cidr.split('/').nth(1).unwrap_or("16");
                let server_host = Ipv4Cidr::parse(&event.wireguard_cidr)
                    .ok()
                    .and_then(|c| c.nth_host(0))
                    .map(|ip| ip.to_string())
                    .unwrap_or_else(|| "10.1.0.1".into());
                network
                    .ensure_wireguard(WireGuardDesiredState {
                        interface: event.wireguard_interface_name.clone(),
                        private_key,
                        listen_port: event.wireguard_listen_port as u16,
                        address: format!("{server_host}/{prefix}"),
                    })
                    .await?;
                recovered += 1;
            }
            Err(e) => {
                return Err(AwdError::Crypto(format!(
                    "failed to decrypt WG private key for recovery: {e}"
                )));
            }
        }
    } else {
        warn!(
            "[Recovery] Event {} has no WG server key material — skip interface restore",
            event.event_id
        );
    }

    // ── 3. Restore firewall（nftables 全局 desired-state reconcile，DB 是事实源）──
    // 失败 Fail Closed：不吞错，由调用方决定是否置 NetworkError。
    firewall_service::reconcile_global(
        db,
        firewall,
        firewall_service::next_network_revision(db).await?,
    )
    .await?;
    firewall_service::flush_event_connections(network, event.event_id, &event.gamebox_cidr).await;
    recovered += 1;

    Ok(recovered)
}

/// Handle a network error: pause event, record error state.
pub async fn handle_network_error(
    db: &DatabaseConnection,
    event_id: Uuid,
    error_msg: &str,
) -> AwdResult<()> {
    warn!(
        "[Recovery] Network error for event {}: {}",
        event_id, error_msg
    );

    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    // 状态机唯一入口（Phase 0）：按当前状态发起 NetworkError 转移，
    // 非法来源（如 Paused→NetworkError 不在转移表）直接拒绝，Fail Closed。
    event_repo::transition_event(
        db,
        awd_event.id,
        awd_event.status.clone(),
        AwdEventStatus::NetworkError,
        Default::default(),
    )
    .await?;

    if let Some(round) =
        crate::modules::event::awd_team::repo::round_repo::find_active_round(db, event_id)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?
    {
        use crate::entity::sea_orm_active_enums::RoundStatus;
        crate::modules::event::awd_team::repo::round_repo::update_round_status(
            db,
            round.id,
            RoundStatus::Paused,
        )
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    Ok(())
}
