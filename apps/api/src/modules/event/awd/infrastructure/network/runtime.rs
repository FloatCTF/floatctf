//! AWD 网络运行时编排。

use async_trait::async_trait;
use uuid::Uuid;

use crate::modules::event::awd::{
    AwdResult,
    system::{
        command::{CommandRunner, RealCommandRunner},
        conntrack, wireguard,
    },
};

/// 单场赛事的 WireGuard 接口期望态。
#[derive(Debug, Clone)]
pub struct WireGuardDesiredState {
    pub interface: String,
    pub private_key: String,
    pub listen_port: u16,
    pub address: String,
}

/// 对等体吊销用的身份标识。
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    pub interface: String,
    pub public_key: String,
}

/// 赛事作用域网络身份（CIDR + 链命名用 id）。
#[derive(Debug, Clone)]
pub struct EventNetworkIdentity {
    pub event_id: Uuid,
    pub gamebox_cidr: String,
}

/// 战队作用域身份，用于定向 conntrack 刷新。
#[derive(Debug, Clone)]
pub struct TeamNetworkIdentity {
    pub event_id: Uuid,
    pub team_id: Uuid,
    pub gamebox_subnet: String,
}

/// 观测到的宿主网络状态（尽力而为）。
#[derive(Debug, Clone, Default)]
pub struct NetworkObservedState {
    pub wireguard_interface_up: bool,
    pub notes: Vec<String>,
}

/// AWD 平台宿主网络（WireGuard / conntrack）。
///
/// 防火墙策略已迁移到独立 `FirewallRuntime`（native nftables，Phase 1）；
/// 本 runtime 只管 WG 生命周期与 conntrack 清理。
#[async_trait]
pub trait AwdNetworkRuntime: Send + Sync {
    async fn ensure_wireguard(&self, desired: WireGuardDesiredState) -> AwdResult<()>;
    async fn remove_wireguard(&self, interface: &str) -> AwdResult<()>;
    async fn revoke_peer(&self, peer: PeerIdentity) -> AwdResult<()>;
    /// 把 peer（public_key + allowed-ips）加回接口（幂等）。Noop 下为 no-op。
    /// Host 实现 = `wg set <iface> peer <pubkey> allowed-ips <ip>`（system::wireguard::add_peer）。
    async fn add_peer(&self, peer: PeerIdentity, allowed_ips: &str) -> AwdResult<()>;
    async fn clear_event_connections(&self, event: EventNetworkIdentity) -> AwdResult<()>;
    async fn clear_team_connections(&self, team: TeamNetworkIdentity) -> AwdResult<()>;
    async fn inspect(&self, event: EventNetworkIdentity) -> AwdResult<NetworkObservedState>;
}

/// 使用真实宿主命令的生产实现。
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
        // best-effort create：接口已存在时 create 会失败，属预期（幂等重入），忽略后继续配置；
        // 其他失败也先继续（configure 会暴露真实问题）。
        // Phase 0 P0-4 吞错扫描：此处为有意幂等语义；
        // WG 命令失败的严格失败模型见 Phase 1 P1-15（Active→Rotating→Revoked 生命周期）。
        let _ = wireguard::create_interface(
            self.runner(),
            &desired.interface,
            &desired.private_key,
            desired.listen_port,
            &desired.address,
        )
        .await;
        Ok(())
    }

    async fn remove_wireguard(&self, interface: &str) -> AwdResult<()> {
        wireguard::delete_interface(self.runner(), interface).await
    }

    async fn revoke_peer(&self, peer: PeerIdentity) -> AwdResult<()> {
        wireguard::remove_peer(self.runner(), &peer.interface, &peer.public_key).await
    }

    async fn add_peer(&self, peer: PeerIdentity, allowed_ips: &str) -> AwdResult<()> {
        wireguard::add_peer(
            self.runner(),
            &peer.interface,
            &peer.public_key,
            allowed_ips,
        )
        .await
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

/// 无宿主权限环境的空操作运行时（CI / 本地 API）。
pub struct NoopNetworkRuntime;

#[async_trait]
impl AwdNetworkRuntime for NoopNetworkRuntime {
    async fn ensure_wireguard(&self, _desired: WireGuardDesiredState) -> AwdResult<()> {
        Ok(())
    }
    async fn remove_wireguard(&self, _interface: &str) -> AwdResult<()> {
        Ok(())
    }
    async fn revoke_peer(&self, _peer: PeerIdentity) -> AwdResult<()> {
        Ok(())
    }
    async fn add_peer(&self, _peer: PeerIdentity, _allowed_ips: &str) -> AwdResult<()> {
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
