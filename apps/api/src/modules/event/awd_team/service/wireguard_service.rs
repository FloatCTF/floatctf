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
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    crypto::{AwdCrypto, EncryptedBlob},
    domain::Ipv4Cidr,
    infrastructure::network::{AwdNetworkRuntime, WgKeyPair, generate_keypair},
    repo::wireguard_repo,
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

    let awd_event = awd_events::Entity::find()
        .filter(awd_events::Column::EventId.eq(event_id))
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

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

/// Revoke a peer (disables the WireGuard tunnel for that user).
pub async fn revoke_peer(db: &DatabaseConnection, peer_id: Uuid) -> AwdResult<()> {
    wireguard_repo::revoke_peer(db, peer_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

/// Rotate a peer's keys while preserving the assigned IP.
pub async fn rotate_peer_keys(
    db: &DatabaseConnection,
    crypto: &AwdCrypto,
    event_id: Uuid,
    peer_id: Uuid,
) -> AwdResult<()> {
    let kp = generate_keypair();
    let aad = AwdCrypto::build_aad(event_id, "wg_peer_private_key");
    let blob = crypto
        .encrypt(kp.private_key.as_bytes(), &aad, 1)
        .map_err(|e| AwdError::Crypto(e.to_string()))?;
    wireguard_repo::rotate_peer(db, peer_id, &kp.public_key, &blob.ciphertext, &blob.nonce)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}
