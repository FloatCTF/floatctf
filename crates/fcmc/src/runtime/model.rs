//! 运行时领域模型（健康检查、端口、容器规格等）。

use std::collections::HashMap;
use std::time::Duration;

/// Jeopardy 风格优雅停止使用的默认 stop 超时。
pub const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// AWD / 批量清理偏好立即信号。
pub const IMMEDIATE_STOP_TIMEOUT: Duration = Duration::from_secs(0);

/// 通用 Docker 网络创建请求。
#[derive(Debug, Clone)]
pub struct NetworkSpec {
    pub name: String,
    pub subnet_cidr: String,
    /// When true, containers cannot reach outside the network (AWD default).
    pub internal: bool,
    /// Optional Linux bridge interface name (`com.docker.network.bridge.name`).
    pub bridge_name: Option<String>,
    pub check_duplicate: bool,
}

/// 网络 create/inspect 后返回的句柄。
#[derive(Debug, Clone)]
pub struct NetworkHandle {
    pub network_id: String,
    pub network_name: String,
}

/// 高层网络 inspect 视图。
#[derive(Debug, Clone)]
pub struct NetworkInspect {
    pub network_id: String,
    pub network_name: String,
    pub exists: bool,
    pub driver: Option<String>,
    pub subnet: Option<String>,
}

/// 宿主端口发布。
#[derive(Debug, Clone)]
pub struct PortBinding {
    pub container_port: String, // e.g. "80/tcp"
    pub host_ip: Option<String>,
    pub host_port: Option<String>,
}

/// 容器资源与安全限制。
#[derive(Debug, Clone, Default)]
pub struct ResourceLimits {
    pub cpu_millis: Option<i64>,
    pub memory_bytes: Option<i64>,
    pub pids_limit: Option<i64>,
    pub cap_drop: Vec<String>,
    pub privileged: bool,
    pub extra_hosts: Vec<String>,
}

/// 可选的 Docker 健康检查配置。
#[derive(Debug, Clone)]
pub struct HealthcheckSpec {
    pub test: Vec<String>,
    pub interval_secs: i64,
    pub timeout_secs: i64,
    pub retries: i64,
    pub start_period_secs: i64,
}

/// 通用容器创建请求（与赛制无关）。
#[derive(Debug, Clone)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub env: Vec<String>,
    pub labels: HashMap<String, String>,
    /// Attach to this network name (also sets network_mode when no host networking).
    pub network_name: Option<String>,
    pub fixed_ip: Option<String>,
    /// 容器在 primary 网络上的 DNS aliases（Docker embedded DNS 可解析）。
    pub network_aliases: Vec<String>,
    pub port_bindings: Vec<PortBinding>,
    pub auto_remove: bool,
    pub resources: ResourceLimits,
    /// When set, overrides network_mode (e.g. "host").
    pub network_mode: Option<String>,
    pub healthcheck: Option<HealthcheckSpec>,
}

/// create/start 后返回的句柄。
#[derive(Debug, Clone)]
pub struct ContainerHandle {
    pub container_id: String,
    pub container_name: String,
}

/// 容器内 exec 参数（AWD-P patch / 运维探测共用）。
#[derive(Debug, Clone)]
pub struct ExecOptions {
    /// 命令及其参数（在容器内执行，如 `["/bin/sh", "patch.sh"]`）。
    pub cmd: Vec<String>,
    /// 附加环境变量（`VAR=value` 形式；空列表表示继承容器环境）。
    pub env: Vec<String>,
    /// 容器内工作目录。
    pub workdir: Option<String>,
    /// 整体超时；超时后 `ExecOutcome::timed_out = true`（部分输出仍返回）。
    pub timeout: Duration,
    /// 可选 stdin 内容（如 `/bin/sh -s` 的脚本）。None = 不附加 stdin。
    pub stdin: Option<Vec<u8>>,
    /// stdout 字节上限（超限截断）。
    pub stdout_limit: usize,
    /// stderr 字节上限（超限截断）。
    pub stderr_limit: usize,
}

/// exec 执行结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutcome {
    /// exec 进程退出码；超时或 inspect 前仍在运行时为 `None`。
    pub exit_code: Option<i64>,
    /// 截断后的 stdout（UTF-8 lossy）。
    pub stdout: String,
    /// 截断后的 stderr（UTF-8 lossy）。
    pub stderr: String,
    /// 从 start_exec 到流结束/超时的耗时（毫秒）。
    pub duration_ms: u64,
    /// 是否因超时中断（此时输出为已收集的部分输出）。
    pub timed_out: bool,
}

/// `copy_from_container` 单次导出的硬上限（1 GiB，纵深防御）。
/// 源码目录打包（AWD-P source.zip）与镜像内文件验证共用。
pub const MAX_COPY_BYTES: usize = 1 << 30;

/// Jeopardy 与 AWD 共用的检查视图。
#[derive(Debug, Clone)]
pub struct ContainerState {
    pub container_id: String,
    pub container_name: String,
    pub image: String,
    pub status: String,
    pub running: bool,
    pub labels: HashMap<String, String>,
    pub created_at: Option<String>,
    /// Published host ports: container_port -> host_port
    pub published_ports: HashMap<String, u16>,
    /// Container IP on its Docker network (default bridge when no custom network).
    pub ip_address: Option<String>,
}

/// 列出容器时的过滤条件。
#[derive(Debug, Clone, Default)]
pub struct ContainerFilter {
    pub all: bool,
    pub label_equals: Vec<(String, String)>,
    pub name: Option<String>,
}

impl ContainerFilter {
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.label_equals.push((key.into(), value.into()));
        self
    }

    pub fn all(mut self) -> Self {
        self.all = true;
        self
    }

    /// Convert to bollard list filters map.
    pub fn to_bollard_filters(&self) -> HashMap<String, Vec<String>> {
        let mut filters = HashMap::new();
        if !self.label_equals.is_empty() {
            let labels: Vec<String> = self
                .label_equals
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            filters.insert("label".to_string(), labels);
        }
        if let Some(name) = &self.name {
            filters.insert("name".to_string(), vec![name.clone()]);
        }
        filters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_filter_to_bollard_labels() {
        let f = ContainerFilter::default()
            .all()
            .with_label("awd.event_id", "abc");
        let map = f.to_bollard_filters();
        assert_eq!(
            map.get("label").unwrap(),
            &vec!["awd.event_id=abc".to_string()]
        );
        assert!(f.all);
    }

    #[test]
    fn network_spec_defaults_check_duplicate() {
        let s = NetworkSpec {
            name: "n1".into(),
            subnet_cidr: "10.0.0.0/16".into(),
            internal: true,
            bridge_name: Some("br-n1".into()),
            check_duplicate: true,
        };
        assert!(s.internal);
        assert_eq!(s.bridge_name.as_deref(), Some("br-n1"));
    }
}
