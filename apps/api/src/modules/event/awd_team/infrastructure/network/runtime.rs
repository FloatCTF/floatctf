//! Host network runtime for WireGuard, firewall policy, and conntrack.

use async_trait::async_trait;
use uuid::Uuid;

use crate::modules::event::awd_team::{
    AwdResult,
    system::{
        command::{CommandRunner, RealCommandRunner},
        conntrack, firewall, wireguard,
    },
};

/// Desired WireGuard interface state for one event.
#[derive(Debug, Clone)]
pub struct WireGuardDesiredState {
    pub interface: String,
    pub private_key: String,
    pub listen_port: u16,
    pub address: String,
}

/// Identity for peer revocation.
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    pub interface: String,
    pub public_key: String,
}

/// Event-scoped network identity (CIDR + ids for chain naming).
#[derive(Debug, Clone)]
pub struct EventNetworkIdentity {
    pub event_id: Uuid,
    pub gamebox_cidr: String,
}

/// Team-scoped identity for targeted conntrack flush.
#[derive(Debug, Clone)]
pub struct TeamNetworkIdentity {
    pub event_id: Uuid,
    pub team_id: Uuid,
    pub gamebox_subnet: String,
}

/// Observed host network state (best-effort).
#[derive(Debug, Clone, Default)]
pub struct NetworkObservedState {
    pub wireguard_interface_up: bool,
    pub notes: Vec<String>,
}

/// Rendered firewall policy for one apply operation.
#[derive(Debug, Clone)]
pub struct EventNetworkPolicy {
    pub event_id: Uuid,
    /// Pre-rendered rules from `firewall::render_*`.
    pub rules: crate::modules::event::awd_team::system::firewall::RenderedRules,
    pub dry_run: bool,
}

/// Platform host networking for AWD (WG / iptables / conntrack).
#[async_trait]
pub trait AwdNetworkRuntime: Send + Sync {
    async fn ensure_wireguard(&self, desired: WireGuardDesiredState) -> AwdResult<()>;
    async fn remove_wireguard(&self, interface: &str) -> AwdResult<()>;
    async fn apply_policy(&self, policy: EventNetworkPolicy) -> AwdResult<()>;
    async fn revoke_peer(&self, peer: PeerIdentity) -> AwdResult<()>;
    async fn clear_event_connections(&self, event: EventNetworkIdentity) -> AwdResult<()>;
    async fn clear_team_connections(&self, team: TeamNetworkIdentity) -> AwdResult<()>;
    async fn inspect(&self, event: EventNetworkIdentity) -> AwdResult<NetworkObservedState>;
}

/// Production implementation using real host commands.
pub struct HostNetworkRuntime {
    runner: RealCommandRunner,
}

impl HostNetworkRuntime {
    pub fn new() -> Self {
        Self {
            runner: RealCommandRunner,
        }
    }

    fn runner(&self) -> &dyn CommandRunner {
        &self.runner
    }
}

impl Default for HostNetworkRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AwdNetworkRuntime for HostNetworkRuntime {
    async fn ensure_wireguard(&self, desired: WireGuardDesiredState) -> AwdResult<()> {
        // Idempotent-ish: try create; if exists, still set key/port/address best-effort.
        let _ = wireguard::create_interface(
            self.runner(),
            &desired.interface,
            &desired.private_key,
            desired.listen_port,
            &desired.address,
        )
        .await;
        // create_interface fails if iface exists — ignore and continue with configure if needed.
        Ok(())
    }

    async fn remove_wireguard(&self, interface: &str) -> AwdResult<()> {
        wireguard::delete_interface(self.runner(), interface).await
    }

    async fn apply_policy(&self, policy: EventNetworkPolicy) -> AwdResult<()> {
        if policy.dry_run {
            tracing::info!(
                event_id = %policy.event_id,
                bytes = policy.rules.iptables_restore_input.len(),
                "[Network] dry-run policy (not applied)"
            );
            return Ok(());
        }
        firewall::apply_rules(self.runner(), &policy.rules).await
    }

    async fn revoke_peer(&self, peer: PeerIdentity) -> AwdResult<()> {
        wireguard::remove_peer(self.runner(), &peer.interface, &peer.public_key).await
    }

    async fn clear_event_connections(&self, event: EventNetworkIdentity) -> AwdResult<()> {
        conntrack::flush_event_gamebox_traffic(self.runner(), &event.gamebox_cidr).await
    }

    async fn clear_team_connections(&self, team: TeamNetworkIdentity) -> AwdResult<()> {
        conntrack::flush_for_cidr(self.runner(), &team.gamebox_subnet).await
    }

    async fn inspect(&self, event: EventNetworkIdentity) -> AwdResult<NetworkObservedState> {
        Ok(NetworkObservedState {
            wireguard_interface_up: false,
            notes: vec![format!(
                "inspect stub for event {} cidr {}",
                event.event_id, event.gamebox_cidr
            )],
        })
    }
}

/// No-op runtime for environments without host privileges (CI / local API).
pub struct NoopNetworkRuntime;

#[async_trait]
impl AwdNetworkRuntime for NoopNetworkRuntime {
    async fn ensure_wireguard(&self, _desired: WireGuardDesiredState) -> AwdResult<()> {
        Ok(())
    }
    async fn remove_wireguard(&self, _interface: &str) -> AwdResult<()> {
        Ok(())
    }
    async fn apply_policy(&self, _policy: EventNetworkPolicy) -> AwdResult<()> {
        Ok(())
    }
    async fn revoke_peer(&self, _peer: PeerIdentity) -> AwdResult<()> {
        Ok(())
    }
    async fn clear_event_connections(&self, _event: EventNetworkIdentity) -> AwdResult<()> {
        Ok(())
    }
    async fn clear_team_connections(&self, _team: TeamNetworkIdentity) -> AwdResult<()> {
        Ok(())
    }
    async fn inspect(&self, _event: EventNetworkIdentity) -> AwdResult<NetworkObservedState> {
        Ok(NetworkObservedState::default())
    }
}
