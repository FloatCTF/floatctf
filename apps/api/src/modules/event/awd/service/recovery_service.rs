//! AWD 故障恢复服务。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    TransactionTrait,
};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::entity::{
    awd_event_networks, awd_events, event_gamebox_instances, scheduled_tasks,
    sea_orm_active_enums::{AwdEventStatus, AwdPhase, GameboxStatus},
};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    crypto::{AwdCrypto, EncryptedBlob},
    domain::Ipv4Cidr,
    infrastructure::{
        firewall::FirewallRuntime,
        network::{AwdNetworkRuntime, WireGuardDesiredState},
    },
    repo::{event_network_repo, event_repo, round_repo},
    service::{event_service, firewall_service, round_service},
};
use crate::scheduler::TaskKey;
use fcmc::AwdContainerRuntime;

/// 启动时对全部活跃 AWD 赛事执行恢复。
pub async fn recover_all(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    crypto: &AwdCrypto,
    publisher: &dyn crate::infrastructure::realtime::EventPublisher,
) -> AwdResult<u32> {
    let active_events = event_repo::find_active_events(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let mut recovered = 0u32;

    for event in &active_events {
        // Event Network 是恢复的 Desired State 源（§54）
        let event_network = match event_network_repo::find_by_event_id(db, event.event_id).await {
            Ok(Some(net)) => net,
            Ok(None) => {
                warn!(
                    "[Recovery] Event {} 无 Event Network（Draft？）— 跳过",
                    event.event_id
                );
                continue;
            }
            Err(e) => {
                error!(
                    "[Recovery] Event {} network load failed: {}",
                    event.event_id, e
                );
                continue;
            }
        };
        match recover_event(
            db,
            containers,
            network,
            firewall,
            crypto,
            publisher,
            event,
            &event_network,
        )
        .await
        {
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

/// 恢复单场 AWD 赛事的资源。
async fn recover_event(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    crypto: &AwdCrypto,
    publisher: &dyn crate::infrastructure::realtime::EventPublisher,
    event: &awd_events::Model,
    event_network: &awd_event_networks::Model,
) -> AwdResult<u32> {
    let mut recovered = 0u32;

    // ── 1. Reconcile GameBox containers ──
    let live = containers
        .list_event_containers(event.event_id)
        .await
        .map_err(|e| AwdError::Docker(e.to_string()))?;
    let live_by_name: std::collections::HashMap<String, &fcmc::ContainerState> =
        live.iter().map(|c| (c.container_name.clone(), c)).collect();

    let instances =
        crate::modules::event::awd::repo::gamebox_repo::find_instances_by_event(db, event.event_id)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;

    for (instance, root) in &instances {
        match live_by_name.get(&root.container_name) {
            Some(state) if state.running => {
                if root.container_id.as_deref() != Some(state.container_id.as_str()) {
                    // 根：容器实现同步 + running；扩展：Ready（§54 desired-state 对账）
                    crate::modules::event::awd::repo::gamebox_repo::update_runtime_root(
                        db,
                        root.id,
                        Some(&state.container_id),
                        "running",
                        None,
                    )
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                    let active: event_gamebox_instances::ActiveModel =
                        event_gamebox_instances::ActiveModel {
                            id: Set(instance.id),
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
                        instance.id, root.container_name
                    );
                    crate::modules::event::awd::repo::gamebox_repo::update_instance_status(
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
                let wg_cidr_str = event_network.wireguard_cidr.to_string();
                let prefix = wg_cidr_str.split('/').nth(1).unwrap_or("16");
                let server_host = Ipv4Cidr::parse(&wg_cidr_str)
                    .ok()
                    .and_then(|c| c.nth_host(0))
                    .map(|ip| ip.to_string())
                    .unwrap_or_else(|| "10.1.0.1".into());
                network
                    .ensure_wireguard(WireGuardDesiredState {
                        interface: event_network.wireguard_interface_name.clone(),
                        private_key,
                        listen_port: event_network.wireguard_listen_port as u16,
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
    firewall_service::flush_event_connections(
        network,
        event.event_id,
        &event_network.gamebox_cidr.to_string(),
    )
    .await;
    recovered += 1;

    // ── 4. Round / Hardening 调度恢复 ──
    // 4a. Hardening phase：确保 HardeningEnd 调度存在
    if event.phase == AwdPhase::Hardening && event.status == AwdEventStatus::Running {
        if let Some(deadline) = event.hardening_ends_at {
            let key = TaskKey::AwdHardeningEnd.to_string();
            let pending = scheduled_tasks::Entity::find()
                .filter(scheduled_tasks::Column::GroupId.eq(event.event_id))
                .filter(scheduled_tasks::Column::TaskKey.eq(&key))
                .filter(scheduled_tasks::Column::Status.eq("pending"))
                .one(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
            if pending.is_none() {
                let txn = db
                    .begin()
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                crate::modules::event::awd::scheduler::schedule_hardening_end(
                    &txn,
                    event.event_id,
                    deadline.fixed_offset(),
                )
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
                txn.commit()
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                recovered += 1;
                info!(
                    "[Recovery] Event {} restored HardeningEnd schedule",
                    event.event_id
                );
            }
        }
    }

    // 4b. Attack round 调度恢复
    let restored_tasks =
        round_service::restore_round_scheduling(db, event.event_id, network, firewall, publisher)
            .await?;
    if restored_tasks > 0 {
        info!(
            "[Recovery] Event {} restored {} round scheduling task(s)",
            event.event_id, restored_tasks
        );
    }
    recovered += restored_tasks as u32;

    // ── 4c. Judge batch deadline 恢复 ──
    let restored_batches =
        crate::modules::event::awd::scheduler::restore_batch_deadlines(db, event.event_id)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
    if restored_batches > 0 {
        info!(
            "[Recovery] Event {} restored {} judge batch deadline task(s)",
            event.event_id, restored_batches
        );
    }
    recovered += restored_batches as u32;

    // ── 5. Final Settlement / Finished recovery ──
    if event.status == AwdEventStatus::Running && event.phase == AwdPhase::Attack {
        let active_round = round_repo::find_active_round(db, event.event_id)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        if active_round.is_none() {
            // No active round — could be crash-gap or final settlement
            let latest = round_repo::find_latest_round(db, event.event_id)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
            let is_final = event_service::is_final_settlement(event, latest.as_ref());
            if is_final {
                info!(
                    "[Recovery] Event {} is in final settlement — attempting finish",
                    event.event_id
                );
                let _ = event_service::maybe_finish_event(
                    db,
                    network,
                    firewall,
                    publisher,
                    event.event_id,
                )
                .await;
            }
            // else: crash-gap — restore_round_scheduling above handled it
        }
    }

    if event.status == AwdEventStatus::Finished {
        // Reconcile Finished lockdown: ensure network is closed
        info!(
            "[Recovery] Event {} is Finished — reconciling lockdown",
            event.event_id
        );
        let _ = firewall_service::reconcile_global(
            db,
            firewall,
            firewall_service::next_network_revision(db).await?,
        )
        .await;
        if let Ok(event_network) =
            crate::modules::event::awd::repo::event_network_repo::require_by_event_id(
                db,
                event.event_id,
            )
            .await
        {
            firewall_service::flush_event_connections(
                network,
                event.event_id,
                &event_network.gamebox_cidr.to_string(),
            )
            .await;
        }
        recovered += 1;
    }

    Ok(recovered)
}

/// 处理网络错误：暂停赛事并记录错误态。
///
/// Saves current phase + remaining time (same as pause_event) so that
/// after admin recovery (NetworkError → Paused → Running), the correct
/// phase and remaining duration are restored.
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

    let now = chrono::Utc::now();
    let remaining: i32;

    // Compute remaining time (same logic as pause_event)
    match awd_event.phase {
        AwdPhase::Hardening => {
            remaining = awd_event
                .hardening_ends_at
                .map(|h| (h.with_timezone(&chrono::Utc) - now).num_seconds().max(0) as i32)
                .unwrap_or(0);

            // Cancel pending HardeningEnd task
            let _ =
                crate::modules::event::awd::scheduler::cancel_pending_hardening_end(db, event_id)
                    .await;
        }
        AwdPhase::Attack => {
            if let Some(round) =
                crate::modules::event::awd::repo::round_repo::find_active_round(db, event_id)
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?
            {
                remaining = (round.scheduled_end_at.with_timezone(&chrono::Utc) - now)
                    .num_seconds()
                    .max(0) as i32;

                crate::modules::event::awd::repo::round_repo::pause_round(db, round.id, remaining)
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
            } else {
                remaining = 0;
            }
        }
        _ => {
            remaining = 0;
        }
    }

    // Transition to NetworkError, saving paused_phase + remaining time
    let patch = event_repo::TransitionPatch {
        phase: Some(AwdPhase::Pause),
        paused_phase: Some(awd_event.phase),
        pause_remaining_secs: Some(remaining),
        hardening_ends_at: Some(None), // clear hardening deadline
        ..Default::default()
    };

    event_repo::transition_event(
        db,
        awd_event.id,
        awd_event.status.clone(),
        AwdEventStatus::NetworkError,
        patch,
    )
    .await?;

    // Pause any active round (NetworkError freezes round timers)
    if let Some(round) =
        crate::modules::event::awd::repo::round_repo::find_active_round(db, event_id)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?
    {
        use crate::entity::sea_orm_active_enums::RoundStatus;
        crate::modules::event::awd::repo::round_repo::update_round_status(
            db,
            round.id,
            RoundStatus::Paused,
        )
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    Ok(())
}
