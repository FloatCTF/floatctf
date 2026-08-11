//! WireGuard 接口管理——宿主侧真实 `wg` 命令。
//!
//! 全部使用结构化参数列表，禁止 shell 拼接。
//! 操作尽可能幂等。

use crate::modules::event::awd::{
    AwdError, AwdResult,
    system::command::{CommandRunner, wireguard_cmd},
};
use tracing::info;

/// 使用给定私钥与监听端口创建 WireGuard 接口。
pub async fn create_interface(
    runner: &dyn CommandRunner,
    iface: &str,
    private_key: &str,
    listen_port: u16,
    address: &str, // e.g. "10.1.0.1/16"
) -> AwdResult<()> {
    // Create the interface
    runner
        .run(
            "ip",
            &[
                "link".to_string(),
                "add".to_string(),
                iface.to_string(),
                "type".to_string(),
                "wireguard".to_string(),
            ],
        )
        .await
        .map_err(|e| AwdError::Network(format!("Failed to create WG interface: {}", e)))?;

    // Set private key and listen port
    runner
        .run(
            "wg",
            &[
                "set".to_string(),
                iface.to_string(),
                "private-key".to_string(),
                "/dev/stdin".to_string(),
                "listen-port".to_string(),
                listen_port.to_string(),
            ],
        )
        .await
        .map_err(|e| AwdError::Network(format!("Failed to configure WG interface: {}", e)))?;

    // Assign IP address
    runner
        .run(
            "ip",
            &[
                "addr".to_string(),
                "add".to_string(),
                address.to_string(),
                "dev".to_string(),
                iface.to_string(),
            ],
        )
        .await
        .map_err(|e| AwdError::Network(format!("Failed to assign IP to WG interface: {}", e)))?;

    // Bring interface up
    runner
        .run(
            "ip",
            &[
                "link".to_string(),
                "set".to_string(),
                iface.to_string(),
                "up".to_string(),
            ],
        )
        .await
        .map_err(|e| AwdError::Network(format!("Failed to bring up WG interface: {}", e)))?;

    info!(
        "[WireGuard] Created interface {} on port {}",
        iface, listen_port
    );
    Ok(())
}

/// 删除 WireGuard 接口。
pub async fn delete_interface(runner: &dyn CommandRunner, iface: &str) -> AwdResult<()> {
    // Bring down first（best-effort：接口可能已处于 down 状态，Phase 0 P0-4 吞错扫描）。
    let _ = runner
        .run(
            "ip",
            &[
                "link".to_string(),
                "set".to_string(),
                iface.to_string(),
                "down".to_string(),
            ],
        )
        .await;

    runner
        .run(
            "ip",
            &["link".to_string(), "del".to_string(), iface.to_string()],
        )
        .await
        .map_err(|e| AwdError::Network(format!("Failed to delete WG interface: {}", e)))?;

    info!("[WireGuard] Deleted interface {}", iface);
    Ok(())
}

/// 向已有 WireGuard 接口添加对等体。
pub async fn add_peer(
    runner: &dyn CommandRunner,
    iface: &str,
    public_key: &str,
    allowed_ips: &str, // e.g. "10.1.1.2/32"
) -> AwdResult<()> {
    wireguard_cmd::add_peer(runner, iface, public_key, allowed_ips)
        .await
        .map_err(|e| AwdError::Network(format!("Failed to add WG peer: {}", e)))?;
    Ok(())
}

/// 从 WireGuard 接口移除对等体。
pub async fn remove_peer(
    runner: &dyn CommandRunner,
    iface: &str,
    public_key: &str,
) -> AwdResult<()> {
    wireguard_cmd::remove_peer(runner, iface, public_key)
        .await
        .map_err(|e| AwdError::Network(format!("Failed to remove WG peer: {}", e)))?;
    Ok(())
}

/// 启用 IP 转发。
pub async fn enable_ip_forwarding(runner: &dyn CommandRunner) -> AwdResult<()> {
    runner
        .run(
            "sysctl",
            &["-w".to_string(), "net.ipv4.ip_forward=1".to_string()],
        )
        .await
        .map_err(|e| AwdError::Network(format!("Failed to enable IP forwarding: {}", e)))?;
    Ok(())
}

/// 刷新某 CIDR 的全部 conntrack 条目（比按赛事范围调用 conntrack_cmd 更干净）。
pub async fn flush_event_conntrack(
    runner: &dyn CommandRunner,
    gamebox_cidr: &str,
) -> AwdResult<()> {
    crate::modules::event::awd::system::conntrack::flush_for_cidr(runner, gamebox_cidr).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interface_name_valid() {
        // WG interface names max 15 chars
        assert!("fctf-awd-1234".len() <= 15);
    }
}
