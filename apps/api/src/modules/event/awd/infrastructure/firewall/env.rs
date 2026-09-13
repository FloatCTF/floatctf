//! Host firewall environment discovery (Phase 1 P1-1/P1-12, Phase 2 P2-13 快照基础)。
//!
//! 记录宿主 Netfilter/firewall 环境（非密钥，用于故障排查与 capability 判定）。
//! 最终正确性由 Precheck 的 connectivity probe 验证（Phase 2），这里只做事实收集。

use helper_protocol::Request;

use crate::{
    infrastructure::helper::HelperClient,
    modules::event::awd::{
        AwdResult,
        system::command::{CommandRunner, RealCommandRunner},
    },
};

/// 宿主防火墙环境快照（非密钥）。
#[derive(Debug, Clone, Default)]
pub struct HostFirewallEnvironment {
    pub nft_version: Option<String>,
    pub kernel_version: Option<String>,
    pub docker_firewall_backend: Option<String>,
    pub firewalld_active: bool,
    pub iptables_frontend: Option<String>,
    pub notes: Vec<String>,
}

/// 宿主防火墙 capability 判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostNetworkCapability {
    /// nft binary + nf_tables + 权限齐备，可以使用 NftablesFirewallRuntime。
    Supported,
    /// 缺少 nftables 能力 —— 不允许 fallback iptables，返回 `HostNetworkUnsupported`。
    Unsupported(String),
}

/// 探测宿主防火墙环境（只读命令，无副作用）。
pub async fn discover_environment() -> HostFirewallEnvironment {
    let runner = RealCommandRunner;
    let mut env = HostFirewallEnvironment::default();

    if let Ok(out) = runner.run("nft", &["--version".into()]).await {
        if out.exit_code == 0 {
            env.nft_version = Some(out.stdout.trim().to_string());
        }
    }

    if let Ok(out) = runner.run("uname", &["-r".into()]).await {
        if out.exit_code == 0 {
            env.kernel_version = Some(out.stdout.trim().to_string());
        }
    }

    if let Ok(out) = runner.run("iptables", &["--version".into()]).await {
        if out.exit_code == 0 {
            env.iptables_frontend = Some(out.stdout.trim().to_string());
        }
    }

    if let Ok(out) = runner.run("firewall-cmd", &["--state".into()]).await {
        env.firewalld_active = out.exit_code == 0 && out.stdout.trim() == "running";
    }

    env
}

/// 判定宿主网络控制面是否可用。正常运行时的权限探针由 `floatctf-helper` 承担，
/// API 自身无需 CAP_NET_ADMIN。
pub async fn check_host_capability() -> AwdResult<HostNetworkCapability> {
    let client = HelperClient::new(helper_protocol::DEFAULT_CONTROL_SOCKET_PATH);
    match client.call(Request::Ping).await {
        Ok(_) => Ok(HostNetworkCapability::Supported),
        Err(error) => Ok(HostNetworkCapability::Unsupported(format!(
            "floatctf-helper unavailable: {error}"
        ))),
    }
}
