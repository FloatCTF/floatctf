//! Deployment orchestration — Docker network, infra containers, GameBoxes,
//! WireGuard interface, and hardening firewall for an AWD event.
//!
//! Each step is idempotent: re-running deploy will not create duplicates.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::core::config::AwdStaticConfig;
use crate::entity::{
    awd_event_networks, awd_events, awd_gamebox_instances, awd_runtime_resources,
    awd_team_networks, event_teams,
    sea_orm_active_enums::{AwdEventStatus, GameboxStatus},
};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    crypto::{AwdCrypto, EncryptedBlob},
    domain::{AwdEventStatusExt, Ipv4Cidr, instance_ip_for_offset},
    infrastructure::{
        firewall::FirewallRuntime,
        network::{AwdNetworkRuntime, WireGuardDesiredState},
    },
    repo::{event_gamebox_repo, event_network_repo, event_repo, gamebox_repo},
    service::{event_network_service, firewall_service, gamebox_service, team_network_allocator},
};
use fcmc::{AwdContainerRuntime, EventNetworkSpec, InfrastructureContainerSpec};

/// Orchestrate full deployment of an AWD event.
///
/// Steps (idempotent):
/// 1. Docker network
/// 2. FlagServer container
/// 3. JudgeServer container
/// 4. Team networks (+ encrypted SSH passwords)
/// 5. GameBox instance rows + containers
/// 6. WireGuard interface (+ store server key material)
/// 7. Hardening firewall policy
/// 8. Mark Deployed
pub async fn deploy_event(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    network: &dyn AwdNetworkRuntime,
    firewall: &dyn FirewallRuntime,
    crypto: &AwdCrypto,
    awd_config: &AwdStaticConfig,
    event_id: Uuid,
) -> AwdResult<()> {
    let mut awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    if !awd_event.status.is_configurable()
        && awd_event.status != AwdEventStatus::Deploying
        && awd_event.status != AwdEventStatus::Deployed
    {
        return Err(AwdError::InvalidState(format!(
            "Cannot deploy event in {:?} status",
            awd_event.status
        )));
    }

    // 状态机唯一入口（Phase 0）：仅当尚未处于 Deploying 时才发起转移（幂等重入）。
    if awd_event.status != AwdEventStatus::Deploying {
        event_repo::transition_event(
            db,
            awd_event.id,
            awd_event.status.clone(),
            AwdEventStatus::Deploying,
            Default::default(),
        )
        .await?;
    }

    // ── 部署步骤（失败 → DeployFailed，P1-16）──
    let deploy_result: AwdResult<()> = async {
        // 0. Event Network 必须已分配（§21/§51），Deploy 时锁定 addressing（§34）
        let event_network = event_network_repo::require_by_event_id(db, event_id).await?;
        event_network_service::lock(db, event_id).await?;

        // 1. Docker network（Desired name 来自 Event Network；Observed ID 进 runtime resources）
        let (docker_network_id, docker_network_name) =
            ensure_docker_network(db, containers, &event_network, event_id).await?;

        // ── 2–3. FlagServer + JudgeServer ──
        ensure_infra_container(
            db,
            containers,
            crypto,
            &awd_event,
            event_id,
            "flagserver",
            &event_network.flagserver_ip.ip().to_string(),
            &docker_network_name,
            awd_config.flagserver_image.clone(),
            &awd_event.flagserver_token_ciphertext,
            &awd_event.flagserver_token_nonce,
        )
        .await?;
        ensure_infra_container(
            db,
            containers,
            crypto,
            &awd_event,
            event_id,
            "judgeserver",
            &event_network.judgeserver_ip.ip().to_string(),
            &docker_network_name,
            awd_config.judgeserver_image.clone(),
            &awd_event.judgeserver_token_ciphertext,
            &awd_event.judgeserver_token_nonce,
        )
        .await?;

        // ── 4–5. Team nets + GameBoxes ──
        ensure_teams_and_gameboxes(
            db,
            containers,
            crypto,
            &awd_event,
            &event_network,
            event_id,
            &docker_network_id,
            &docker_network_name,
        )
        .await?;

        // ── 6. WireGuard ──
        ensure_wireguard(
            db,
            network,
            crypto,
            awd_config,
            &awd_event,
            &event_network,
            event_id,
        )
        .await?;

        // ── 7. Hardening firewall（全局 desired-state reconcile，Phase 1 P1-10）──
        firewall_service::reconcile_global(
            db,
            firewall,
            firewall_service::next_network_revision(db).await?,
        )
        .await?;

        // 8. Deployed（状态机唯一入口）
        event_repo::transition_event(
            db,
            awd_event.id,
            AwdEventStatus::Deploying,
            AwdEventStatus::Deployed,
            Default::default(),
        )
        .await?;

        info!("[Deploy] Event {} fully deployed", event_id);
        Ok(())
    }
    .await;

    match deploy_result {
        Ok(()) => Ok(()),
        Err(e) => {
            // DeployFailed 写入路径（P1-16）：部署失败不再停留在 Deploying。
            // 仅当仍处于 Deploying 时转移（幂等重入/并发场景不覆盖新状态）。
            if let Ok(Some(ev)) = event_repo::find_by_event_id(db, event_id).await {
                if ev.status == AwdEventStatus::Deploying {
                    if let Err(te) = event_repo::transition_event(
                        db,
                        ev.id,
                        AwdEventStatus::Deploying,
                        AwdEventStatus::DeployFailed,
                        Default::default(),
                    )
                    .await
                    {
                        tracing::error!(
                            "[Deploy] failed to record DeployFailed for event {}: {}",
                            event_id,
                            te
                        );
                    }
                }
            }
            tracing::error!("[Deploy] Event {} deployment failed: {}", event_id, e);
            Err(e)
        }
    }
}

async fn ensure_docker_network(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    event_network: &awd_event_networks::Model,
    event_id: Uuid,
) -> AwdResult<(String, String)> {
    let net_name = event_network.docker_network_name.clone();

    // Observed network ID（§14）：runtime ID 归 awd_runtime_resources 管理
    let db_tracked_id = awd_runtime_resources::Entity::find()
        .filter(awd_runtime_resources::Column::EventId.eq(event_id))
        .filter(awd_runtime_resources::Column::ResourceType.eq("docker_network"))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .map(|r| r.resource_id);

    if let Some(id) = db_tracked_id {
        // Verify still exists; recreate if missing
        match containers.inspect_event_network(&id).await {
            Ok(state) if state.exists => return Ok((id, net_name)),
            _ => {
                warn!("[Deploy] docker network {} missing — recreating", net_name);
            }
        }
    }

    info!("[Deploy] Creating Docker network for event {}", event_id);
    let handle = containers
        .create_event_network(EventNetworkSpec {
            event_id,
            network_name: net_name.clone(),
            subnet_cidr: event_network.gamebox_cidr.to_string(),
            internal: true,
        })
        .await
        .map_err(|e| AwdError::Docker(e.to_string()))?;

    // Observed 记录（不写 awd_events）
    record_resource(db, event_id, "docker_network", &handle.network_id, None).await?;

    Ok((handle.network_id, handle.network_name))
}

async fn ensure_infra_container(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    crypto: &AwdCrypto,
    awd_event: &awd_events::Model,
    event_id: Uuid,
    kind: &str,
    fixed_ip: &str,
    network_name: &str,
    image_ref: String,
    token_ct: &Option<Vec<u8>>,
    token_nonce: &Option<Vec<u8>>,
) -> AwdResult<()> {
    let container_name = format!("fctf-{}-{}", kind, &event_id.to_string()[..8]);
    let resource_id = container_name.clone();

    // P1-16：仅查 DB 行不够——必须核验实际容器。DB 有记录但容器已消失 → 重建。
    let db_tracked = resource_exists(db, event_id, kind, &resource_id).await?;
    let container_alive = containers
        .inspect_container(&container_name)
        .await
        .map(|state| state.running)
        .unwrap_or(false);

    if db_tracked && container_alive {
        info!(
            "[Deploy] {} already tracked and running — skip create",
            kind
        );
        return Ok(());
    }
    if db_tracked && !container_alive {
        warn!(
            "[Deploy] {} tracked in DB but container missing/stopped — recreating",
            kind
        );
    }

    let token = match (token_ct, token_nonce) {
        (Some(ct), Some(nonce)) => {
            let blob = EncryptedBlob {
                ciphertext: ct.clone(),
                nonce: nonce.clone(),
                key_version: awd_event.key_version,
            };
            let aad = AwdCrypto::build_aad(event_id, "internal_token");
            let bytes = crypto
                .decrypt(&blob, &aad)
                .map_err(|e| AwdError::Crypto(e.to_string()))?;
            String::from_utf8(bytes).map_err(|e| AwdError::Crypto(e.to_string()))?
        }
        _ => {
            return Err(AwdError::InvalidState(format!(
                "{kind} token not configured"
            )));
        }
    };

    let handle = containers
        .create_infrastructure_container(InfrastructureContainerSpec {
            event_id,
            container_name: container_name.clone(),
            image_ref,
            network_name: network_name.to_string(),
            fixed_ip: fixed_ip.to_string(),
            env: vec![
                format!("EVENT_ID={event_id}"),
                format!("INTERNAL_TOKEN={token}"),
                format!("LISTEN_ADDR=0.0.0.0:8080"),
            ],
            cpu_millis: Some(500),
            memory_bytes: Some(256 * 1024 * 1024),
        })
        .await
        .map_err(|e| AwdError::Docker(e.to_string()))?;

    record_resource(
        db,
        event_id,
        kind,
        &handle.container_id,
        Some(&container_name),
    )
    .await?;
    info!("[Deploy] {} created as {}", kind, handle.container_id);
    Ok(())
}

async fn ensure_teams_and_gameboxes(
    db: &DatabaseConnection,
    containers: &dyn AwdContainerRuntime,
    crypto: &AwdCrypto,
    awd_event: &awd_events::Model,
    event_network: &awd_event_networks::Model,
    event_id: Uuid,
    docker_network_id: &str,
    docker_network_name: &str,
) -> AwdResult<()> {
    // ── 部署前跨赛事重叠校验（新数据源：awd_event_networks，§19）──
    let other_event_networks = event_network_repo::list_all(db)
        .await?
        .into_iter()
        .filter(|en| en.event_id != event_id)
        .collect::<Vec<_>>();
    let other_networks = awd_team_networks::Entity::find()
        .filter(awd_team_networks::Column::EventId.ne(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    validate_no_cross_event_overlap(
        &other_event_networks,
        &other_networks,
        &event_network.gamebox_cidr.to_string(),
        &event_network.wireguard_cidr.to_string(),
        &event_network.flagserver_ip.ip().to_string(),
        &event_network.judgeserver_ip.ip().to_string(),
    )?;

    // ── Team 子网：TeamNetworkAllocator（§36-39，稳定持久分配，不依赖顺序/名称）──
    let allocated =
        team_network_allocator::ensure_team_networks(db, event_id, crypto, awd_event.key_version)
            .await?;
    let team_network_map: std::collections::HashMap<Uuid, awd_team_networks::Model> =
        allocated.into_iter().map(|n| (n.team_id, n)).collect();

    // ── teams 枚举（GameBox 循环用，确定性顺序）──
    let teams = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .order_by_asc(event_teams::Column::CreatedAt)
        .order_by_asc(event_teams::Column::Id)
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // ── GameBox 实例行 + 容器创建（事务外：Docker 调用不占用事务连接）──
    // 确定性 IP：instance_ip = team.gamebox_subnet + event_gamebox.host_offset（§13/§66），
    // 与模板枚举顺序无关；新增队伍/新增 GameBox 不影响已分配 IP。
    let event_gameboxes = event_gamebox_repo::find_event_gameboxes_by_event(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    for team in &teams {
        let team_network = team_network_map.get(&team.id).ok_or_else(|| {
            AwdError::Internal(format!("team network missing for team {}", team.id))
        })?;
        // Decrypt SSH password（team-level 凭据，§22.1）
        let password =
            gamebox_service::decrypt_team_ssh_password(crypto, event_id, team_network).await?;

        for eg in &event_gameboxes {
            // 单一 resolver：Revision 默认值 + Event 覆盖（§49/§50）
            let resolved = gamebox_service::resolve_event_gamebox_spec(db, eg.id).await?;

            let gamebox_ip =
                instance_ip_for_offset(&team_network.gamebox_subnet.to_string(), eg.host_offset)
                    .ok_or_else(|| {
                        AwdError::Network(format!(
                            "no host IP for subnet {} offset {}",
                            team_network.gamebox_subnet, eg.host_offset
                        ))
                    })?;

            let existing =
                gamebox_repo::find_instance_by_event_gamebox_team(db, event_id, eg.id, team.id)
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;

            let (instance_id, container_name) = if let Some(inst) = existing {
                if inst.current_container_id.is_some()
                    && matches!(
                        inst.status,
                        GameboxStatus::Ready | GameboxStatus::Running | GameboxStatus::Creating
                    )
                {
                    continue;
                }
                (inst.id, inst.container_name)
            } else {
                let container_name = format!(
                    "fctf-{}-t{}-team{}",
                    &event_id.to_string()[..8],
                    &eg.id.to_string()[..4],
                    &team.id.to_string()[..4]
                );
                let id = Uuid::new_v4();
                awd_gamebox_instances::ActiveModel {
                    id: Set(id),
                    event_id: Set(event_id),
                    event_gamebox_id: Set(eg.id),
                    team_id: Set(team.id),
                    status: Set(GameboxStatus::Pending),
                    container_name: Set(container_name.clone()),
                    gamebox_ip: Set(gamebox_ip.parse().map_err(|e| {
                        AwdError::Validation(format!("invalid gamebox_ip {gamebox_ip}: {e}"))
                    })?),
                    runtime_generation: Set(1),
                    health_status: Set("unknown".to_string()),
                    ..Default::default()
                }
                .insert(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
                (id, container_name)
            };

            // Mark creating then create container（runtime_generation=1 首次部署）
            gamebox_set_status(db, instance_id, GameboxStatus::Creating).await?;

            let spec = gamebox_service::build_gamebox_runtime_spec(
                &resolved,
                awd_event,
                event_network,
                instance_id,
                eg.id,
                team.id,
                &container_name,
                &gamebox_ip,
                docker_network_name,
                password.clone(),
                1,
            )?;

            let handle = containers
                .create_gamebox(spec)
                .await
                .map_err(|e| AwdError::Docker(e.to_string()));

            match handle {
                Ok(h) => {
                    let mut active: awd_gamebox_instances::ActiveModel =
                        awd_gamebox_instances::ActiveModel {
                            id: Set(instance_id),
                            current_container_id: Set(Some(h.container_id)),
                            status: Set(GameboxStatus::Ready),
                            health_status: Set("unknown".to_string()),
                            ..Default::default()
                        };
                    active
                        .update(db)
                        .await
                        .map_err(|e| AwdError::Database(e.to_string()))?;
                }
                Err(e) => {
                    gamebox_set_status(db, instance_id, GameboxStatus::StartFailed).await?;
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

async fn ensure_wireguard(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    crypto: &AwdCrypto,
    awd_config: &AwdStaticConfig,
    awd_event: &awd_events::Model,
    event_network: &awd_event_networks::Model,
    event_id: Uuid,
) -> AwdResult<()> {
    // Load or generate WG private key
    let (private_key, public_key) = if let (Some(ct), Some(nonce), Some(pub_k)) = (
        &awd_event.wg_server_private_key_ciphertext,
        &awd_event.wg_server_private_key_nonce,
        &awd_event.wg_server_public_key,
    ) {
        let blob = EncryptedBlob {
            ciphertext: ct.clone(),
            nonce: nonce.clone(),
            key_version: awd_event.key_version,
        };
        let aad = AwdCrypto::build_aad(event_id, "wg_server_private_key");
        let pk = crypto
            .decrypt(&blob, &aad)
            .map_err(|e| AwdError::Crypto(e.to_string()))?;
        (
            String::from_utf8(pk).map_err(|e| AwdError::Crypto(e.to_string()))?,
            pub_k.clone(),
        )
    } else {
        // Pure-Rust X25519 keygen (same as `wg genkey` / `wg pubkey`).
        let kp = crate::modules::event::awd::infrastructure::network::generate_keypair();
        let aad = AwdCrypto::build_aad(event_id, "wg_server_private_key");
        let blob = crypto
            .encrypt(kp.private_key.as_bytes(), &aad, awd_event.key_version)
            .map_err(|e| AwdError::Crypto(e.to_string()))?;
        let mut active: awd_events::ActiveModel = awd_events::ActiveModel {
            id: Set(awd_event.id),
            wg_server_private_key_ciphertext: Set(Some(blob.ciphertext)),
            wg_server_private_key_nonce: Set(Some(blob.nonce)),
            wg_server_public_key: Set(Some(kp.public_key.clone())),
            ..Default::default()
        };
        active
            .update(db)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        (kp.private_key, kp.public_key)
    };

    let _public_key = public_key; // stored on awd_events for client configs
    let server_addr = format!(
        "{}/{}",
        Ipv4Cidr::parse(&event_network.wireguard_cidr.to_string())
            .ok()
            .and_then(|c| c.nth_host(0))
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "10.1.0.1".into()),
        event_network
            .wireguard_cidr
            .to_string()
            .split('/')
            .nth(1)
            .unwrap_or("16")
    );

    network
        .ensure_wireguard(WireGuardDesiredState {
            interface: event_network.wireguard_interface_name.clone(),
            private_key,
            listen_port: event_network.wireguard_listen_port as u16,
            address: server_addr,
        })
        .await?;

    info!(
        "[Deploy] WireGuard interface {} ensured",
        event_network.wireguard_interface_name
    );
    Ok(())
}

async fn resource_exists(
    db: &DatabaseConnection,
    event_id: Uuid,
    resource_type: &str,
    resource_id: &str,
) -> AwdResult<bool> {
    let found = awd_runtime_resources::Entity::find()
        .filter(awd_runtime_resources::Column::EventId.eq(event_id))
        .filter(awd_runtime_resources::Column::ResourceType.eq(resource_type))
        .filter(awd_runtime_resources::Column::ResourceId.eq(resource_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(found.is_some())
}

async fn record_resource(
    db: &DatabaseConnection,
    event_id: Uuid,
    resource_type: &str,
    resource_id: &str,
    resource_name: Option<&str>,
) -> AwdResult<()> {
    use chrono::Utc;
    awd_runtime_resources::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        resource_type: Set(resource_type.to_string()),
        resource_id: Set(resource_id.to_string()),
        resource_name: Set(resource_name.map(|s| s.to_string())),
        observed_state: Set(None),
        last_seen_at: Set(Utc::now().into()),
    }
    .insert(db)
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}

async fn gamebox_set_status(
    db: &DatabaseConnection,
    id: Uuid,
    status: GameboxStatus,
) -> AwdResult<()> {
    let mut active: awd_gamebox_instances::ActiveModel = awd_gamebox_instances::ActiveModel {
        id: Set(id),
        status: Set(status),
        ..Default::default()
    };
    active
        .update(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}

fn random_password(len: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

// ── P1-14：跨赛事 CIDR / IP 重叠校验（纯函数）──
//
// 调用方负责把 `events` / `networks` 过滤为**其他赛事**（不含当前赛事）：
// - `events`：其他赛事的 awd_event_networks 行（gamebox_cidr / wireguard_cidr，§19 新数据源）
// - `networks`：其他赛事已分配的队伍子网（awd_team_networks.gamebox_subnet / wireguard_subnet）
// - 其余四个参数：当前赛事的配置
//
// 校验规则：
// 1. 当前赛事 gamebox_cidr / wireguard_cidr 不得与其他赛事的 gamebox_cidr / wireguard_cidr 重叠；
// 2. 当前赛事 flagserver_ip / judgeserver_ip 不得落在其他赛事的 gamebox_cidr / wireguard_cidr /
//    队伍子网（gamebox_subnet / wireguard_subnet）内。
fn parse_cidr(cidr: &str, name: &str) -> AwdResult<Ipv4Cidr> {
    Ipv4Cidr::parse(cidr).map_err(|e| AwdError::Validation(format!("{name} ({cidr}) invalid: {e}")))
}

fn parse_ip(ip: &str, name: &str) -> AwdResult<std::net::Ipv4Addr> {
    ip.parse::<std::net::Ipv4Addr>()
        .map_err(|e| AwdError::Validation(format!("{name} ({ip}) invalid: {e}")))
}

pub fn validate_no_cross_event_overlap(
    events: &[awd_event_networks::Model],
    networks: &[awd_team_networks::Model],
    gamebox_cidr: &str,
    wireguard_cidr: &str,
    flagserver_ip: &str,
    judgeserver_ip: &str,
) -> AwdResult<()> {
    let gb = parse_cidr(gamebox_cidr, "gamebox_cidr")?;
    let wg = parse_cidr(wireguard_cidr, "wireguard_cidr")?;
    let fs_ip = parse_ip(flagserver_ip, "flagserver_ip")?;
    let js_ip = parse_ip(judgeserver_ip, "judgeserver_ip")?;

    // 1. 当前赛事 CIDR vs 其他赛事 CIDR（双向交叉：gamebox / wireguard）
    for other in events {
        let other_gb = parse_cidr(&other.gamebox_cidr.to_string(), "other gamebox_cidr")?;
        let other_wg = parse_cidr(&other.wireguard_cidr.to_string(), "other wireguard_cidr")?;

        if gb.overlaps(&other_gb) || gb.overlaps(&other_wg) {
            return Err(AwdError::Conflict(format!(
                "gamebox_cidr {gamebox_cidr} overlaps other event {} (gamebox {} / wireguard {})",
                other.event_id, other.gamebox_cidr, other.wireguard_cidr
            )));
        }
        if wg.overlaps(&other_gb) || wg.overlaps(&other_wg) {
            return Err(AwdError::Conflict(format!(
                "wireguard_cidr {wireguard_cidr} overlaps other event {} (gamebox {} / wireguard {})",
                other.event_id, other.gamebox_cidr, other.wireguard_cidr
            )));
        }

        // 2a. 端口 IP（flagserver / judgeserver）不得落在其他赛事 CIDR 内
        if other_gb.contains(fs_ip) || other_wg.contains(fs_ip) {
            return Err(AwdError::Conflict(format!(
                "flagserver_ip {flagserver_ip} falls inside other event {} (gamebox {} / wireguard {})",
                other.event_id, other.gamebox_cidr, other.wireguard_cidr
            )));
        }
        if other_gb.contains(js_ip) || other_wg.contains(js_ip) {
            return Err(AwdError::Conflict(format!(
                "judgeserver_ip {judgeserver_ip} falls inside other event {} (gamebox {} / wireguard {})",
                other.event_id, other.gamebox_cidr, other.wireguard_cidr
            )));
        }
    }

    // 2b. 端口 IP 不得落在其他赛事已分配队伍子网内
    for net in networks {
        let net_gb = parse_cidr(&net.gamebox_subnet.to_string(), "other team gamebox_subnet")?;
        let net_wg = parse_cidr(
            &net.wireguard_subnet.to_string(),
            "other team wireguard_subnet",
        )?;

        if net_gb.contains(fs_ip) || net_wg.contains(fs_ip) {
            return Err(AwdError::Conflict(format!(
                "flagserver_ip {flagserver_ip} falls inside other team subnet {} of event {}",
                net.gamebox_subnet, net.event_id
            )));
        }
        if net_gb.contains(js_ip) || net_wg.contains(js_ip) {
            return Err(AwdError::Conflict(format!(
                "judgeserver_ip {judgeserver_ip} falls inside other team subnet {} of event {}",
                net.gamebox_subnet, net.event_id
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个最小 awd_event_networks::Model（validate_no_cross_event_overlap 数据源）。
    fn event_model(
        id: u128,
        gamebox_cidr: &str,
        wireguard_cidr: &str,
        flagserver_ip: &str,
        judgeserver_ip: &str,
    ) -> awd_event_networks::Model {
        let now: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();
        awd_event_networks::Model {
            id: Uuid::from_u128(id),
            event_id: Uuid::from_u128(id + 1),
            allocation_mode: crate::entity::sea_orm_active_enums::AwdNetworkAllocationMode::Manual,
            gamebox_cidr: gamebox_cidr.parse().unwrap(),
            wireguard_cidr: wireguard_cidr.parse().unwrap(),
            infrastructure_subnet: format!(
                "10.{}.0.0/24",
                gamebox_cidr.split('.').nth(1).unwrap_or("0")
            )
            .parse()
            .unwrap(),
            flagserver_ip: flagserver_ip.parse().unwrap(),
            judgeserver_ip: judgeserver_ip.parse().unwrap(),
            wireguard_interface_name: format!("fawg_{id:08x}"),
            wireguard_listen_port: 51820,
            docker_network_name: format!("fctf-awd-{id:08x}"),
            locked_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// 构造一个最小 awd_team_networks::Model（队伍子网）。
    fn network_model(
        event_id: Uuid,
        gamebox_subnet: &str,
        wireguard_subnet: &str,
    ) -> awd_team_networks::Model {
        let now: chrono::DateTime<chrono::FixedOffset> = chrono::Utc::now().into();
        awd_team_networks::Model {
            id: Uuid::new_v4(),
            event_id,
            team_id: Uuid::new_v4(),
            gamebox_subnet: gamebox_subnet.parse().unwrap(),
            wireguard_subnet: wireguard_subnet.parse().unwrap(),
            ssh_password_ciphertext: vec![],
            ssh_password_nonce: vec![],
            key_version: 1,
            next_wireguard_host: 2,
            status: "allocated".to_string(),
            subnet_index: 1,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn test_validate_disjoint_cidrs_ok() {
        // 当前赛事与其他赛事 CIDR / 端口 IP 完全不相交 → 通过
        let events = vec![event_model(
            0x1000,
            "10.20.0.0/16",
            "172.20.0.0/16",
            "10.20.0.2",
            "10.20.0.3",
        )];
        let networks = vec![network_model(
            events[0].event_id,
            "10.20.1.0/24",
            "172.20.1.0/24",
        )];
        let result = validate_no_cross_event_overlap(
            &events,
            &networks,
            "10.30.0.0/16",
            "172.30.0.0/16",
            "10.30.0.2",
            "10.30.0.3",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_same_gamebox_cidr_rejected() {
        // 当前 gamebox_cidr 与其他赛事 gamebox_cidr 完全相同 → 重叠报错
        let events = vec![event_model(
            0x1000,
            "10.20.0.0/16",
            "172.20.0.0/16",
            "10.20.0.2",
            "10.20.0.3",
        )];
        let result = validate_no_cross_event_overlap(
            &events,
            &[],
            "10.20.0.0/16",
            "172.30.0.0/16",
            "10.30.0.2",
            "10.30.0.3",
        );
        assert!(matches!(result, Err(AwdError::Conflict(_))));
    }

    #[test]
    fn test_validate_wireguard_overlapping_other_gamebox_rejected() {
        // 跨 CIDR 域交叉：当前 wireguard_cidr 与其他赛事 gamebox_cidr 重叠
        let events = vec![event_model(
            0x1000,
            "10.20.0.0/16",
            "172.20.0.0/16",
            "10.20.0.2",
            "10.20.0.3",
        )];
        let result = validate_no_cross_event_overlap(
            &events,
            &[],
            "10.30.0.0/16",
            "10.20.128.0/16",
            "10.30.0.2",
            "10.30.0.3",
        );
        assert!(matches!(result, Err(AwdError::Conflict(_))));
    }

    #[test]
    fn test_validate_flagserver_ip_in_other_team_subnet_rejected() {
        // 端口 IP 冲突：当前 flagserver_ip 落在其他赛事已分配队伍子网内
        // （队伍子网不在其赛事父 CIDR 内的异常数据，专门走队伍子网校验分支）
        let events = vec![event_model(
            0x1000,
            "10.20.0.0/16",
            "172.20.0.0/16",
            "10.20.0.2",
            "10.20.0.3",
        )];
        let networks = vec![network_model(
            events[0].event_id,
            "10.99.1.0/24",
            "172.99.1.0/24",
        )];
        let result = validate_no_cross_event_overlap(
            &events,
            &networks,
            "10.30.0.0/16",
            "172.30.0.0/16",
            "10.99.1.5",
            "10.30.0.3",
        );
        assert!(matches!(result, Err(AwdError::Conflict(_))));
    }

    #[test]
    fn test_validate_judgeserver_ip_in_other_wireguard_rejected() {
        // 端口 IP 冲突：当前 judgeserver_ip 落在其他赛事 wireguard_cidr 内
        let events = vec![event_model(
            0x1000,
            "10.20.0.0/16",
            "172.20.0.0/16",
            "10.20.0.2",
            "10.20.0.3",
        )];
        let result = validate_no_cross_event_overlap(
            &events,
            &[],
            "10.30.0.0/16",
            "172.30.0.0/16",
            "10.30.0.2",
            "172.20.5.9",
        );
        assert!(matches!(result, Err(AwdError::Conflict(_))));
    }
}
