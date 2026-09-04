//! AWDP data plane 网络 ACL（host nftables，plan §32-§34；每赛事独立 table）。
//!
//! 目标策略（GameBox 是**不可信**的——玩家攻陷自身靶机后可自由出站）：
//!
//! | 方向 | 策略 |
//! |------|------|
//! | GameBox → JudgeServer DATA port (80) | ACCEPT（Break flag / proof） |
//! | JudgeServer → GameBox declared ports | ACCEPT（healthcheck / judge / exploit） |
//! | GameBox A → GameBox B | DROP（横向隔离） |
//! | GameBox → JudgeServer CONTROL（不存在监听） | 路由不可达（control 网络 GameBox 无权加入） |
//! | GameBox → Docker host 管理/服务端口（API/Postgres/RustFS/Docker API） | DROP |
//! | GameBox → Docker socket | 无挂载（不可能） |
//!
//! 实现：host nftables **每赛事独立 table**（`inet floatctf_awdp_{event 前 8}`；
//! 练习固定 `inet floatctf_awdp_practice`）：
//! - `forward` hook（priority 1）：池→池 DROP、池→judge 非 data port DROP；
//! - `input` hook（priority 1）：池→宿主指定端口 DROP（judge 固定 IP 豁免、established 豁免）。
//!
//! 应用为 best-effort：`nft` 不可用 / 无权限时仅告警跳过（练习沙箱不阻塞实例启动）；
//! 规则一旦生效由 host/control plane 强制，不信任容器内规则。

use std::process::Stdio;

use tracing::warn;

use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::judge::{CONTROL_NETWORK_NAME, PRACTICE_DYNAMIC_POOL, PRACTICE_JUDGE_PORT},
};

/// 宿主管控端口黑名单（GameBox → host 一律 DROP）。按项目常见端口收敛，
/// 实际部署可在 [awdp] 配置扩展（见 AwdpStaticConfig.practice_acl_host_ports）。
pub const DEFAULT_BLOCKED_HOST_PORTS: &[u16] = &[9090, 5432, 9000, 2375, 2376, 8080, 8443];

/// 渲染完整的 nftables 规则集（table 级原子替换；不 flush 其它 table）。
///
/// `table_name`：nftables 表名（赛事专属 `floatctf_awdp_{event 前 8}`；练习固定）。
/// `bridge_iface`：data 网络宿主 bridge 接口（`br-<network_id 前 12 hex>`）。
/// `dynamic_pool`：本赛事动态 IP 池（GameBox 实例网段）。
/// `judge_ip`：本赛事 JudgeServer 固定 IP（ACL 豁免来源）。
/// `blocked_host_ports`：GameBox → host 需要 DROP 的端口。
pub fn render_ruleset(
    table_name: &str,
    bridge_iface: &str,
    dynamic_pool: &str,
    judge_ip: &str,
    blocked_host_ports: &[u16],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("table inet {table_name} {{\n"));
    // forward：池→池 DROP（横向隔离）；池→judge 仅 data port。
    out.push_str("    chain forward_filter {\n");
    out.push_str("        type filter hook forward priority 1; policy accept;\n");
    out.push_str(&format!(
        "        ip saddr {dynamic_pool} ip daddr {dynamic_pool} drop\n"
    ));
    out.push_str(&format!(
        "        ip saddr {dynamic_pool} ip daddr {judge_ip} tcp dport != {PRACTICE_JUDGE_PORT} drop\n"
    ));
    out.push_str(&format!(
        "        ip saddr {judge_ip} ip daddr {dynamic_pool} accept\n"
    ));
    out.push_str("    }\n");
    // input：池→宿主管控端口 DROP（judge 豁免；established 由 conntrack 豁免）。
    out.push_str("    chain input_filter {\n");
    out.push_str("        type filter hook input priority 1; policy accept;\n");
    out.push_str("        ct state established,related accept\n");
    for port in blocked_host_ports {
        out.push_str(&format!(
            "        iifname \"{bridge_iface}\" ip saddr {dynamic_pool} tcp dport {port} drop\n"
        ));
    }
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

/// 宿主 bridge 接口名：Docker bridge 网络 `br-<network id 前 12 hex>`。
pub fn bridge_iface_for_network(network_id: &str) -> String {
    let short: String = network_id
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(12)
        .collect();
    format!("br-{short}")
}

/// 应用 nftables 规则集（best-effort：`nft -c` 校验 → `nft -f` 原子应用）。
///
/// - `nft` 不存在 / 权限不足 → `Ok(false)`（跳过，告警由调用方记录）；
/// - 语法错误 → `Err`（配置问题需人工介入，不静默）。
pub async fn apply_ruleset(ruleset: &str) -> AwdpResult<bool> {
    let nft = match tokio::process::Command::new("nft").arg("-v").output().await {
        Ok(o) if o.status.success() => "nft".to_string(),
        Ok(_) => {
            warn!("[PracticeACL] nft 不可用，跳过 data plane ACL");
            return Ok(false);
        }
        Err(e) => {
            warn!(error = %e, "[PracticeACL] nft 启动失败，跳过 data plane ACL");
            return Ok(false);
        }
    };

    // 先写临时规则文件，再 `nft -c -f` 校验、`nft -f` 应用（与 AWD 防火墙同流程）。
    let tmp = std::env::temp_dir().join(format!("fctf-awdp-acl-{}.nft", uuid::Uuid::new_v4()));
    if let Err(e) = tokio::fs::write(&tmp, ruleset).await {
        // best-effort：临时目录/写盘异常（如 TMPDIR 指向不存在目录）不阻断练习沙箱启动。
        warn!(error = %e, tmp = %tmp.display(), "[PracticeACL] 写临时规则文件失败，跳过 data plane ACL");
        return Ok(false);
    }

    let check = tokio::process::Command::new(&nft)
        .args(["-c", "-f"])
        .arg(&tmp)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| AwdpError::Internal(format!("nft -c run: {e}")))?;
    if !check.status.success() {
        let _ = tokio::fs::remove_file(&tmp).await;
        let stderr = String::from_utf8_lossy(&check.stderr).trim().to_string();
        if stderr.contains("Operation not permitted")
            || stderr.contains("Permission denied")
            || stderr.contains("cache initialization failed")
        {
            // 无 CAP_NET_ADMIN：连语法校验都需要 netlink 权限 → best-effort 跳过。
            warn!(error = %stderr, "[PracticeACL] nft 无权限，跳过 data plane ACL");
            return Ok(false);
        }
        return Err(AwdpError::Internal(format!("nft -c 校验失败: {stderr}")));
    }

    let apply = tokio::process::Command::new(&nft)
        .args(["-f"])
        .arg(&tmp)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| AwdpError::Internal(format!("nft -f run: {e}")))?;
    let _ = tokio::fs::remove_file(&tmp).await;
    if !apply.status.success() {
        // 无权限 / 权限提升失败 → best-effort 跳过（练习沙箱）。
        let stderr = String::from_utf8_lossy(&apply.stderr).trim().to_string();
        warn!(error = %stderr, "[PracticeACL] nft 应用失败（可能无权限），跳过 data plane ACL");
        return Ok(false);
    }
    tracing::info!("[PracticeACL] data plane nftables rules applied");
    Ok(true)
}

/// 完整 ACL 编排：解析 data 网络 bridge 接口 → 渲染 → 应用（best-effort）。
///
/// `network_name`：本赛事 Docker 网络名（练习固定 / 赛事专属）。
/// `judge_ip`：本赛事 JudgeServer 固定 IP。
/// `table_name`：nftables 表名（练习固定 `floatctf_awdp_practice`；赛事专属）。
/// `dynamic_pool`：本赛事动态 IP 池。
pub async fn apply_practice_acl(
    docker: &bollard::Docker,
    network_name: &str,
    judge_ip: &str,
    blocked_host_ports: &[u16],
) -> AwdpResult<bool> {
    let network_id = match docker
        .inspect_network(
            network_name,
            None::<bollard::network::InspectNetworkOptions<String>>,
        )
        .await
    {
        Ok(n) => n.id.clone().unwrap_or_default(),
        Err(_) => String::new(),
    };
    if network_id.is_empty() {
        warn!("[PracticeACL] data 网络 {network_name} 不存在，跳过 ACL");
        return Ok(false);
    }
    let table_name = match network_name {
        // 练习固定表名（兼容既有部署）；赛事网络 → 由 event_id 推导（调用方传参）。
        name if name == crate::modules::event::awdp::domain::judge::PRACTICE_NETWORK_NAME => {
            crate::modules::event::awdp::domain::judge::PRACTICE_ACL_TABLE_NAME.to_string()
        }
        name => name.replace("fctf-awdp-", "floatctf_awdp_"),
    };
    let ruleset = render_ruleset(
        &table_name,
        &bridge_iface_for_network(&network_id),
        // 动态池：从网络 IPAM 取（ip_range）或默认练习池；此处用 ip_range。
        &network_dynamic_pool(docker, network_name)
            .await
            .unwrap_or_else(|| PRACTICE_DYNAMIC_POOL.to_string()),
        judge_ip,
        blocked_host_ports,
    );
    apply_ruleset(&ruleset).await
}

/// 从网络 IPAM config 读取动态池（ip_range）；失败时回退练习默认池。
async fn network_dynamic_pool(docker: &bollard::Docker, network_name: &str) -> Option<String> {
    let net = docker
        .inspect_network(
            network_name,
            None::<bollard::network::InspectNetworkOptions<String>>,
        )
        .await
        .ok()?;
    let ipam = net.ipam?;
    let cfg = ipam.config?;
    let first = cfg.first()?;
    first.ip_range.clone().or(first.subnet.clone())
}

/// 删除赛事 nftables ACL 表（best-effort；表不存在视为成功）。
pub async fn remove_acl_table(table_name: &str) -> AwdpResult<bool> {
    let nft = match tokio::process::Command::new("nft")
        .args(["-v"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => "nft".to_string(),
        _ => {
            warn!("[PracticeACL] nft 不可用，跳过 ACL 表删除");
            return Ok(false);
        }
    };
    let out = tokio::process::Command::new(&nft)
        .args(["delete", "table", "inet", table_name])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => {
            tracing::info!("[PracticeACL] nftables table {table_name} deleted");
            Ok(true)
        }
        Ok(_) => {
            // 表不存在（No such file or directory）视为已删除。
            let stderr = String::from_utf8_lossy(&out.unwrap().stderr)
                .trim()
                .to_string();
            if stderr.contains("No such file") || stderr.contains("does not exist") {
                return Ok(true);
            }
            warn!(error = %stderr, "[PracticeACL] nft 删除表失败（best-effort）");
            Ok(false)
        }
        Err(e) => {
            warn!(error = %e, "[PracticeACL] nft 删除表启动失败（best-effort）");
            Ok(false)
        }
    }
}

/// control 网络幂等 ensure（internal=true：GameBox 无权加入，仅 JudgeServer 使用）。
#[allow(deprecated)] // bollard CreateNetworkOptions
pub async fn ensure_control_network(docker: &bollard::Docker) -> AwdpResult<String> {
    use fcmc::ContainerRuntime;
    let runtime = fcmc::DockerContainerRuntime::new(docker.clone());
    if runtime
        .inspect_network(CONTROL_NETWORK_NAME)
        .await
        .map(|s| s.exists)
        .unwrap_or(false)
    {
        return Ok(CONTROL_NETWORK_NAME.to_string());
    }
    use bollard::network::CreateNetworkOptions;
    let conf = CreateNetworkOptions {
        name: CONTROL_NETWORK_NAME.to_string(),
        driver: "bridge".to_string(),
        internal: true,
        check_duplicate: true,
        ipam: bollard::secret::Ipam {
            config: Some(vec![bollard::secret::IpamConfig {
                subnet: Some("10.42.8.0/24".to_string()),
                ip_range: Some("10.42.8.128/25".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        },
        ..Default::default()
    };
    match docker.create_network(conf).await {
        Ok(_) => {
            tracing::info!(network = %CONTROL_NETWORK_NAME, "AWDP control network ensured");
        }
        Err(e) if practice_network_already_exists(&e) => {}
        Err(e) => {
            return Err(AwdpError::Docker(format!("create control network: {e}")));
        }
    }
    Ok(CONTROL_NETWORK_NAME.to_string())
}

fn practice_network_already_exists(e: &bollard::errors::Error) -> bool {
    match e {
        bollard::errors::Error::DockerResponseServerError {
            status_code: 409, ..
        } => true,
        bollard::errors::Error::DockerResponseServerError {
            status_code: 500,
            message,
        } => message.to_lowercase().contains("already exists"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_iface_derived_from_network_id() {
        assert_eq!(
            bridge_iface_for_network("fecbdbc53c61234"),
            "br-fecbdbc53c61"
        );
        // 非 hex 字符剔除。
        assert_eq!(
            bridge_iface_for_network("ab-12-34-56-78-90-xy"),
            "br-ab1234567890"
        );
    }

    #[test]
    fn ruleset_renders_all_policy_lines() {
        let rs = render_ruleset(
            "floatctf_awdp_practice",
            "br-abcdef123456",
            PRACTICE_DYNAMIC_POOL,
            "10.42.2.2",
            &[9090, 5432],
        );
        assert!(rs.contains("table inet floatctf_awdp_practice"));
        // 横向隔离：池→池 DROP。
        assert!(rs.contains(&format!(
            "ip saddr {PRACTICE_DYNAMIC_POOL} ip daddr {PRACTICE_DYNAMIC_POOL} drop"
        )));
        // 池→judge 仅 data port。
        assert!(rs.contains(&format!(
            "ip saddr {PRACTICE_DYNAMIC_POOL} ip daddr 10.42.2.2 tcp dport != {PRACTICE_JUDGE_PORT} drop"
        )));
        // judge→池 accept。
        assert!(rs.contains(&format!(
            "ip saddr 10.42.2.2 ip daddr {PRACTICE_DYNAMIC_POOL} accept"
        )));
        // 池→宿主管控端口 DROP（judge 豁免在 input 链由 judge IP 不在池内天然成立）。
        assert!(rs.contains(&format!(
            "iifname \"br-abcdef123456\" ip saddr {PRACTICE_DYNAMIC_POOL} tcp dport 9090 drop"
        )));
        assert!(rs.contains("tcp dport 5432 drop"));
        // established 豁免。
        assert!(rs.contains("ct state established,related accept"));
        // 不 flush 全局 ruleset。
        assert!(!rs.contains("flush ruleset"));
    }

    #[test]
    fn ruleset_table_name_is_parameterized() {
        // 赛事专属表名（event 前 8 hex）与练习表名都可渲染。
        let rs = render_ruleset(
            "floatctf_awdp_01234567",
            "br-abcdef123456",
            "10.42.5.128/25",
            "10.42.5.2",
            &[9090],
        );
        assert!(rs.contains("table inet floatctf_awdp_01234567"));
        assert!(rs.contains("ip saddr 10.42.5.128/25 ip daddr 10.42.5.128/25 drop"));
        assert!(!rs.contains("10.42.2.128/25"));
    }

    #[test]
    fn default_blocked_ports_cover_common_service_ports() {
        assert!(DEFAULT_BLOCKED_HOST_PORTS.contains(&9090));
        assert!(DEFAULT_BLOCKED_HOST_PORTS.contains(&5432));
    }
}
