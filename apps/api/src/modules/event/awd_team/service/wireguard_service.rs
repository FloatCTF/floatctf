//! WireGuard interface and peer management.
//!
//! # Key architecture
//!
//! - One WireGuard interface per AWD event
//! - Peers are per-user (each team member gets their own peer)
//! - Client config uses split tunneling (GameBox CIDR + team WireGuard subnet)
//! - Database is the authoritative source — platform recovers from DB on restart
//! - Key generation is pure Rust (`infrastructure::network::keys`), not `wg genkey`

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::core::config::AwdStaticConfig;
use crate::entity::{
    awd_events, awd_team_networks, awd_wireguard_peers, sea_orm_active_enums::WgPeerStatus,
};
use tracing::warn;

use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    crypto::{AwdCrypto, EncryptedBlob},
    domain::Ipv4Cidr,
    infrastructure::network::{AwdNetworkRuntime, PeerIdentity, WgKeyPair, generate_keypair},
    repo::{ban_repo, wireguard_repo},
};

/// Generate a peer keypair (client private + public).
pub fn generate_peer_keypair() -> WgKeyPair {
    generate_keypair()
}

/// Generate a complete WireGuard client configuration.
pub fn build_client_config(
    peer_ip: &str,
    peer_private_key: &str,
    server_public_key: &str,
    server_endpoint: &str,
    server_port: u16,
    gamebox_cidr: &str,
    team_wg_subnet: &str,
) -> String {
    format!(
        r#"[Interface]
Address = {peer_ip}/32
PrivateKey = {peer_private_key}

[Peer]
PublicKey = {server_public_key}
Endpoint = {server_endpoint}:{server_port}
AllowedIPs = {gamebox_cidr}, {team_wg_subnet}
PersistentKeepalive = 25
"#
    )
}

/// Allocate the next available host address from a team's WireGuard subnet.
/// Returns the /32 address string.
pub fn allocate_peer_ip(wireguard_subnet: &Ipv4Cidr, next_host: u32) -> AwdResult<(String, u32)> {
    let ip = wireguard_subnet.nth_host(next_host).ok_or_else(|| {
        AwdError::Network(format!(
            "WireGuard subnet {:?} exhausted at host index {}",
            wireguard_subnet, next_host
        ))
    })?;
    Ok((format!("{}/32", ip), next_host + 1))
}

/// Ensure the user has an active WG peer; create one with pure-Rust keys if missing.
///
/// Returns `(peer_model, plaintext_private_key)` for config rendering only.
pub async fn ensure_peer_for_user(
    db: &DatabaseConnection,
    crypto: &AwdCrypto,
    network: &dyn AwdNetworkRuntime,
    awd_config: &AwdStaticConfig,
    event_id: Uuid,
    user_id: Uuid,
    team_id: Uuid,
) -> AwdResult<(awd_wireguard_peers::Model, String)> {
    if let Some(peer) = wireguard_repo::find_peer_by_user(db, event_id, user_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
    {
        if peer.status == WgPeerStatus::Active {
            let privkey = decrypt_peer_private_key(crypto, event_id, &peer)?;
            return Ok((peer, privkey));
        }
    }

    // 创建前检查：该 team 有 active ban 时拒绝创建 peer（ban 语义 = host 移除 + 禁止新建）。
    if ban_repo::find_active_ban(db, event_id, team_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .is_some()
    {
        return Err(AwdError::Forbidden(format!(
            "Team {} is banned for event {} — WG peer creation denied",
            team_id, event_id
        )));
    }

    let awd_event = load_awd_event(db, event_id).await?;

    let mut team_net = awd_team_networks::Entity::find()
        .filter(awd_team_networks::Column::EventId.eq(event_id))
        .filter(awd_team_networks::Column::TeamId.eq(team_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("Team network not configured".into()))?;

    let subnet = Ipv4Cidr::parse(&team_net.wireguard_subnet)
        .map_err(|e| AwdError::Validation(e.to_string()))?;
    let (assigned_ip, next_host) = allocate_peer_ip(&subnet, team_net.next_wireguard_host as u32)?;

    let kp = generate_keypair();
    let aad = AwdCrypto::build_aad(event_id, "wg_peer_private_key");
    let blob = crypto
        .encrypt(kp.private_key.as_bytes(), &aad, awd_event.key_version)
        .map_err(|e| AwdError::Crypto(e.to_string()))?;

    let peer = awd_wireguard_peers::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        team_id: Set(team_id),
        user_id: Set(user_id),
        status: Set(WgPeerStatus::Active),
        assigned_ip: Set(assigned_ip.clone()),
        public_key: Set(kp.public_key.clone()),
        private_key_ciphertext: Set(blob.ciphertext),
        private_key_nonce: Set(blob.nonce),
        key_version: Set(awd_event.key_version),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;

    // Bump next host allocation
    let mut tn: awd_team_networks::ActiveModel = team_net.into();
    tn.next_wireguard_host = Set(next_host as i32);
    tn.update(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // Host peer add when AWD_HOST_NETWORK is enabled (mirrors HostNetworkRuntime selection).
    let allowed = assigned_ip.clone();
    add_peer_on_host(
        awd_config.network_runtime == "host",
        &awd_event.wireguard_interface_name,
        &kp.public_key,
        &allowed,
    )
    .await?;

    let _network = network;
    Ok((peer, kp.private_key))
}

async fn add_peer_on_host(
    enabled: bool,
    iface: &str,
    public_key: &str,
    allowed_ips: &str,
) -> AwdResult<()> {
    if enabled {
        use crate::modules::event::awd_team::system::{command::RealCommandRunner, wireguard};
        wireguard::add_peer(&RealCommandRunner, iface, public_key, allowed_ips).await?;
    }
    Ok(())
}

fn decrypt_peer_private_key(
    crypto: &AwdCrypto,
    event_id: Uuid,
    peer: &awd_wireguard_peers::Model,
) -> AwdResult<String> {
    let blob = EncryptedBlob {
        ciphertext: peer.private_key_ciphertext.clone(),
        nonce: peer.private_key_nonce.clone(),
        key_version: peer.key_version,
    };
    let aad = AwdCrypto::build_aad(event_id, "wg_peer_private_key");
    let bytes = crypto
        .decrypt(&blob, &aad)
        .map_err(|e| AwdError::Crypto(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| AwdError::Crypto(e.to_string()))
}

/// Load all active peers for an event from DB and return their configs.
pub async fn load_active_peers(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdResult<Vec<crate::entity::awd_wireguard_peers::Model>> {
    wireguard_repo::find_active_peers_by_event(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

/// 查 peer（event 内，跨赛事隔离）。
async fn find_peer(
    db: &DatabaseConnection,
    event_id: Uuid,
    peer_id: Uuid,
) -> AwdResult<awd_wireguard_peers::Model> {
    wireguard_repo::find_peer_by_id(db, event_id, peer_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound(format!("WireGuard peer {} not found", peer_id)))
}

/// 查 AWD event（wireguard_interface_name / key_version 等）。
async fn load_awd_event(db: &DatabaseConnection, event_id: Uuid) -> AwdResult<awd_events::Model> {
    awd_events::Entity::find()
        .filter(awd_events::Column::EventId.eq(event_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))
}

/// Revoke a peer (disables the WireGuard tunnel for that user).
///
/// 闭环：host 先移除 peer（`wg set <iface> peer <pubkey> remove`，经 network runtime），
/// 成功后再写 DB 置 Revoked + revoked_at。host 移除失败则返回错误、DB 保持原状（可重试）。
pub async fn revoke_peer(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    event_id: Uuid,
    peer_id: Uuid,
) -> AwdResult<()> {
    let peer = find_peer(db, event_id, peer_id).await?;
    let awd_event = load_awd_event(db, event_id).await?;
    remove_peer_from_host(
        network,
        &awd_event.wireguard_interface_name,
        &peer.public_key,
    )
    .await?;
    wireguard_repo::revoke_peer(db, peer_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}

/// host 侧移除 peer（与 DB 写入分离，便于 Noop 语义与测试）。
async fn remove_peer_from_host(
    network: &dyn AwdNetworkRuntime,
    iface: &str,
    public_key: &str,
) -> AwdResult<()> {
    network
        .revoke_peer(PeerIdentity {
            interface: iface.to_string(),
            public_key: public_key.to_string(),
        })
        .await
}

/// 轮换前置守卫：仅 Active 状态允许轮换（幂等：已 Revoked / Rotating 拒绝）。
fn assert_peer_rotatable(peer: &awd_wireguard_peers::Model) -> AwdResult<()> {
    if peer.status != WgPeerStatus::Active {
        return Err(AwdError::InvalidState(format!(
            "WireGuard peer {} status is {:?}, rotation requires Active",
            peer.id, peer.status
        )));
    }
    Ok(())
}

/// host 侧轮换：先加新 peer（同一 allowed-ips 立即接管路由，隧道不中断），再移除旧 peer。
async fn apply_rotation_to_host(
    network: &dyn AwdNetworkRuntime,
    iface: &str,
    old_public_key: &str,
    new_public_key: &str,
    assigned_ip: &str,
) -> AwdResult<()> {
    network
        .add_peer(
            PeerIdentity {
                interface: iface.to_string(),
                public_key: new_public_key.to_string(),
            },
            assigned_ip,
        )
        .await?;
    network
        .revoke_peer(PeerIdentity {
            interface: iface.to_string(),
            public_key: old_public_key.to_string(),
        })
        .await?;
    Ok(())
}

/// Fail Closed：host 轮换失败 → 尽力把 peer 置 Revoked（记录告警，随后向上返回原始错误）。
async fn fail_closed_revoke(db: &DatabaseConnection, peer_id: Uuid, cause: &AwdError) {
    if let Err(db_err) = wireguard_repo::revoke_peer(db, peer_id).await {
        warn!(
            "[WireGuard] Failed to mark peer {} Revoked after rotation error ({}): {}",
            peer_id, cause, db_err
        );
    } else {
        warn!(
            "[WireGuard] Peer {} marked Revoked after rotation failure: {}",
            peer_id, cause
        );
    }
}

/// Rotate a peer's keys while preserving the assigned IP.
///
/// 生命周期闭环：Active → Rotating（DB 标记 + rotated_at）→ host 轮换（加新 peer、移除旧 peer）
/// → 成功置回 Active 并更新 public_key/private_key（key_version 用事件当前值）；host 失败置 Revoked
/// （Fail Closed）。幂等：peer 非 Active（已 Revoked / 轮换中）直接拒绝。
///
/// 返回新私钥**一次**，供调用方渲染客户端配置（旧私钥不再可读）。
pub async fn rotate_peer_keys(
    db: &DatabaseConnection,
    crypto: &AwdCrypto,
    network: &dyn AwdNetworkRuntime,
    event_id: Uuid,
    peer_id: Uuid,
) -> AwdResult<String> {
    let peer = find_peer(db, event_id, peer_id).await?;
    assert_peer_rotatable(&peer)?;
    let awd_event = load_awd_event(db, event_id).await?;

    // 新 keypair + 用事件当前 key_version 加密（不再硬编码 1）。
    let kp = generate_keypair();
    let aad = AwdCrypto::build_aad(event_id, "wg_peer_private_key");
    let blob = crypto
        .encrypt(kp.private_key.as_bytes(), &aad, awd_event.key_version)
        .map_err(|e| AwdError::Crypto(e.to_string()))?;

    // 1) DB 先置 Rotating（记录 rotated_at）——此后 host 失败则 Fail Closed 落到 Revoked。
    wireguard_repo::mark_peer_rotating(db, peer_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // 2) host 轮换：加新 peer + 移除旧 peer。
    if let Err(e) = apply_rotation_to_host(
        network,
        &awd_event.wireguard_interface_name,
        &peer.public_key,
        &kp.public_key,
        &peer.assigned_ip,
    )
    .await
    {
        fail_closed_revoke(db, peer_id, &e).await;
        return Err(e);
    }

    // 3) host 成功 → DB 置回 Active + 新密钥。
    wireguard_repo::rotate_peer(
        db,
        peer_id,
        &kp.public_key,
        &blob.ciphertext,
        &blob.nonce,
        awd_event.key_version,
    )
    .await
    .map_err(|e| AwdError::Database(e.to_string()))?;

    Ok(kp.private_key)
}

/// Recovery 辅助：把 event 下全部 Active peers（public_key + assigned_ip）幂等加回 host。
/// Noop runtime 下 add_peer 为 no-op。返回成功恢复的 peer 数量。
/// 注意：本函数只写恢复辅助，接线（recover_all 流程）由上层负责。
pub async fn restore_active_peers_to_host(
    db: &DatabaseConnection,
    network: &dyn AwdNetworkRuntime,
    event_id: Uuid,
) -> AwdResult<usize> {
    let awd_event = load_awd_event(db, event_id).await?;
    let peers = load_active_peers(db, event_id).await?;
    restore_peers_to_host(network, &awd_event.wireguard_interface_name, &peers).await
}

/// 逐个把 Active peer 加回 host（`wg set <iface> peer <pubkey> allowed-ips <ip>`，幂等）。
async fn restore_peers_to_host(
    network: &dyn AwdNetworkRuntime,
    iface: &str,
    peers: &[awd_wireguard_peers::Model],
) -> AwdResult<usize> {
    let mut restored = 0usize;
    for peer in peers {
        network
            .add_peer(
                PeerIdentity {
                    interface: iface.to_string(),
                    public_key: peer.public_key.clone(),
                },
                &peer.assigned_ip,
            )
            .await?;
        restored += 1;
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::awd_team::{
        infrastructure::network::{
            EventNetworkIdentity, NetworkObservedState, TeamNetworkIdentity, WireGuardDesiredState,
        },
        system::command::RecordingCommandRunner,
        system::wireguard,
    };
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// 记录型 mock network runtime：只记录 host 调用序列，不执行真实命令
    /// （参考 nftables.rs 的 FakeNftRunner 风格）。
    struct MockNetworkRuntime {
        calls: Arc<Mutex<Vec<String>>>,
        fail_add_peer: bool,
    }

    impl MockNetworkRuntime {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_add_peer: false,
            }
        }

        /// add_peer 必败的 runtime（验证 Fail Closed 路径）。
        fn failing_add() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_add_peer: true,
            }
        }

        fn recorded(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn record(&self, entry: String) {
            self.calls.lock().unwrap().push(entry);
        }
    }

    #[async_trait]
    impl AwdNetworkRuntime for MockNetworkRuntime {
        async fn ensure_wireguard(&self, desired: WireGuardDesiredState) -> AwdResult<()> {
            self.record(format!("ensure_wireguard:{}", desired.interface));
            Ok(())
        }
        async fn remove_wireguard(&self, interface: &str) -> AwdResult<()> {
            self.record(format!("remove_wireguard:{}", interface));
            Ok(())
        }
        async fn revoke_peer(&self, peer: PeerIdentity) -> AwdResult<()> {
            self.record(format!(
                "revoke_peer:{}:{}",
                peer.interface, peer.public_key
            ));
            Ok(())
        }
        async fn add_peer(&self, peer: PeerIdentity, allowed_ips: &str) -> AwdResult<()> {
            self.record(format!(
                "add_peer:{}:{}:{}",
                peer.interface, peer.public_key, allowed_ips
            ));
            if self.fail_add_peer {
                return Err(AwdError::Network("mock add_peer failure".into()));
            }
            Ok(())
        }
        async fn clear_event_connections(&self, event: EventNetworkIdentity) -> AwdResult<()> {
            self.record(format!("clear_event:{}", event.event_id));
            Ok(())
        }
        async fn clear_team_connections(&self, team: TeamNetworkIdentity) -> AwdResult<()> {
            self.record(format!("clear_team:{}", team.team_id));
            Ok(())
        }
        async fn inspect(&self, _event: EventNetworkIdentity) -> AwdResult<NetworkObservedState> {
            Ok(NetworkObservedState::default())
        }
    }

    fn sample_peer(status: WgPeerStatus) -> awd_wireguard_peers::Model {
        awd_wireguard_peers::Model {
            id: Uuid::new_v4(),
            event_id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            status,
            assigned_ip: "10.42.1.2/32".to_string(),
            public_key: "PUB_KEY_OLD".to_string(),
            private_key_ciphertext: vec![1, 2, 3],
            private_key_nonce: vec![4, 5, 6],
            key_version: 1,
            created_at: chrono::Utc::now().into(),
            rotated_at: None,
            revoked_at: None,
            config_fetched_at: None,
        }
    }

    #[test]
    fn rotation_requires_active_status() {
        // Active 可轮换
        assert!(assert_peer_rotatable(&sample_peer(WgPeerStatus::Active)).is_ok());
        // 幂等：已 Revoked / Rotating 拒绝
        for status in [WgPeerStatus::Revoked, WgPeerStatus::Rotating] {
            let err = assert_peer_rotatable(&sample_peer(status)).unwrap_err();
            assert!(matches!(err, AwdError::InvalidState(_)), "got {:?}", err);
        }
    }

    #[tokio::test]
    async fn apply_rotation_to_host_adds_new_then_removes_old() {
        let net = MockNetworkRuntime::new();
        apply_rotation_to_host(
            &net,
            "fctf-awd-1",
            "PUB_KEY_OLD",
            "PUB_KEY_NEW",
            "10.42.1.2/32",
        )
        .await
        .unwrap();

        let cmds = net.recorded();
        assert_eq!(
            cmds,
            vec![
                // 先加新 peer（allowed-ips 立即接管），再移除旧 peer
                "add_peer:fctf-awd-1:PUB_KEY_NEW:10.42.1.2/32".to_string(),
                "revoke_peer:fctf-awd-1:PUB_KEY_OLD".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn apply_rotation_to_host_propagates_add_failure() {
        let net = MockNetworkRuntime::failing_add();
        let err = apply_rotation_to_host(
            &net,
            "fctf-awd-1",
            "PUB_KEY_OLD",
            "PUB_KEY_NEW",
            "10.42.1.2/32",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AwdError::Network(_)), "got {:?}", err);
        // add 失败即中止，不会走到移除旧 peer（Fail Closed 由调用方落 Revoked）
        assert_eq!(
            net.recorded(),
            vec!["add_peer:fctf-awd-1:PUB_KEY_NEW:10.42.1.2/32".to_string()]
        );
    }

    #[tokio::test]
    async fn remove_peer_from_host_uses_event_interface_and_pubkey() {
        let net = MockNetworkRuntime::new();
        remove_peer_from_host(&net, "fctf-awd-2", "PUB_KEY_OLD")
            .await
            .unwrap();
        assert_eq!(
            net.recorded(),
            vec!["revoke_peer:fctf-awd-2:PUB_KEY_OLD".to_string()]
        );
    }

    #[tokio::test]
    async fn restore_peers_to_host_adds_each_active_peer() {
        let net = MockNetworkRuntime::new();
        let mut peers = vec![];
        for i in 0..3 {
            let mut p = sample_peer(WgPeerStatus::Active);
            p.public_key = format!("PUB_KEY_{}", i);
            p.assigned_ip = format!("10.42.1.{}/32", i + 2);
            peers.push(p);
        }

        let restored = restore_peers_to_host(&net, "fctf-awd-1", &peers)
            .await
            .unwrap();
        assert_eq!(restored, 3);
        let cmds = net.recorded();
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0], "add_peer:fctf-awd-1:PUB_KEY_0:10.42.1.2/32");
        assert_eq!(cmds[1], "add_peer:fctf-awd-1:PUB_KEY_1:10.42.1.3/32");
        assert_eq!(cmds[2], "add_peer:fctf-awd-1:PUB_KEY_2:10.42.1.4/32");
    }

    #[tokio::test]
    async fn restore_peers_to_host_propagates_add_failure() {
        let net = MockNetworkRuntime::failing_add();
        let peers = vec![sample_peer(WgPeerStatus::Active)];
        assert!(
            restore_peers_to_host(&net, "fctf-awd-1", &peers)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn wireguard_cmd_uses_structured_args() {
        // 命令序列断言：wg set <iface> peer <pub> allowed-ips <ip> / ... remove
        let runner = RecordingCommandRunner::new();
        wireguard::add_peer(&runner, "fctf-awd-1", "PUB_KEY_NEW", "10.42.1.2/32")
            .await
            .unwrap();
        wireguard::remove_peer(&runner, "fctf-awd-1", "PUB_KEY_OLD")
            .await
            .unwrap();

        let cmds = runner.recorded();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].0, "wg");
        assert_eq!(
            cmds[0].1,
            vec![
                "set".to_string(),
                "fctf-awd-1".to_string(),
                "peer".to_string(),
                "PUB_KEY_NEW".to_string(),
                "allowed-ips".to_string(),
                "10.42.1.2/32".to_string(),
            ]
        );
        assert_eq!(cmds[1].0, "wg");
        assert_eq!(
            cmds[1].1,
            vec![
                "set".to_string(),
                "fctf-awd-1".to_string(),
                "peer".to_string(),
                "PUB_KEY_OLD".to_string(),
                "remove".to_string(),
            ]
        );
    }
}
