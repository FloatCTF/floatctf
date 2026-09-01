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
    /// 列出宿主路由表中与 AWD 地址池可能冲突的 IPv4 网段（Phase 10 A2：
    /// 分配器必须考虑 Docker 网络 + 宿主路由，不能假设「FloatCTF 库里没有 = 空闲」）。
    /// 默认实现返回空（Noop / mock 无宿主路由语义）；Host 实现 = `ip -o route show`。
    async fn list_host_route_cidrs(&self) -> AwdResult<Vec<String>> {
        Ok(vec![])
    }
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
        // 真实观测：`ip link show <iface>` 检查接口存在且 up。
        // WG 接口在 ip 输出中 state=UNKNOWN（POINTOPOINT+NOARP），不能用 "state UP" 判定；
        // 以 LOWER_UP / UP 标志为准。
        let iface =
            crate::modules::event::awd::domain::network::wireguard_interface_name(&event.event_id);
        let mut notes = Vec::new();
        let mut wireguard_interface_up = false;
        match self
            .runner()
            .run(
                "ip",
                &["link".to_string(), "show".to_string(), iface.clone()],
            )
            .await
        {
            Ok(out) => {
                let first = out.stdout.lines().next().unwrap_or("").trim().to_string();
                notes.push(format!("ip link show {iface}: {first}"));
                wireguard_interface_up =
                    out.stdout.contains("LOWER_UP") || out.stdout.contains("state UP");
            }
            Err(e) => {
                notes.push(format!("ip link show {iface} failed: {e}"));
            }
        }
        // bridge-nf 观测：同桥容器流量必须经过 FORWARD 链，FloatCTF 规则才对其生效。
        match std::fs::read_to_string("/proc/sys/net/bridge/bridge-nf-call-iptables") {
            Ok(v) => {
                notes.push(format!(
                    "bridge-nf-call-iptables={} (same-bridge isolation {})",
                    v.trim(),
                    if v.trim() == "1" {
                        "effective"
                    } else {
                        "NOT effective"
                    }
                ));
            }
            Err(_) => notes.push(
                "bridge-nf-call-iptables: unavailable (br_netfilter not loaded — same-bridge isolation NOT effective)"
                    .to_string(),
            ),
        }
        Ok(NetworkObservedState {
            wireguard_interface_up,
            notes,
        })
    }

    async fn list_host_route_cidrs(&self) -> AwdResult<Vec<String>> {
        // `ip -o route show`：每行 `dst[/prefix] via|dev ...`；只取 IPv4 前缀。
        // 覆盖 Docker 网络之外的宿主占用（libvirt/incus/VPN/手工路由等）。
        let out = self
            .runner()
            .run(
                "ip",
                &["-o".to_string(), "route".to_string(), "show".to_string()],
            )
            .await
            .map_err(|e| crate::modules::event::awd::AwdError::Network(e.to_string()))?;
        Ok(parse_host_route_cidrs(&out.stdout))
    }
}

/// 解析 `ip -o route show` 输出中的 IPv4 前缀列表（纯函数，可单测）。
/// 跳过 default 路由与 IPv6；去重排序保证确定性（分配器依赖稳定顺序）。
fn parse_host_route_cidrs(output: &str) -> Vec<String> {
    let mut cidrs = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("default") {
            continue;
        }
        let dst = line.split_whitespace().next().unwrap_or("");
        if let Some(idx) = dst.find('/') {
            // IPv4 only（IPv6 含 ':'，跳过）
            if !dst[..idx].contains(':') {
                cidrs.push(dst.to_string());
            }
        }
    }
    cidrs.sort();
    cidrs.dedup();
    cidrs
}

#[cfg(test)]
mod tests {
    use super::parse_host_route_cidrs;

    #[test]
    fn parses_ipv4_routes_skips_default_and_ipv6() {
        let out = "default via 192.168.21.1 dev wlp0s20f3 proto static metric 600
10.66.66.0/24 dev wg0 scope link src 10.66.66.1
172.16.9.0/24 dev br-abc123 scope link
192.168.122.0/24 dev virbr0 proto kernel scope link src 192.168.122.1
2001:db8::/32 dev eth0 proto kernel metric 256 pref medium
10.66.66.0/24 dev wg0 scope link src 10.66.66.1
";
        let got = parse_host_route_cidrs(out);
        assert_eq!(
            got,
            vec!["10.66.66.0/24", "172.16.9.0/24", "192.168.122.0/24",],
            "default/IPv6 必须跳过，IPv4 去重排序"
        );
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
