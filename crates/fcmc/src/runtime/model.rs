//! Shared runtime models for container and network operations.

use std::collections::HashMap;
use std::time::Duration;

/// Default stop timeout used by Jeopardy-style graceful stops.
pub const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// AWD / bulk cleanup prefers an immediate signal.
pub const IMMEDIATE_STOP_TIMEOUT: Duration = Duration::from_secs(0);

/// Generic Docker network creation request.
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

/// Handle returned after network create/inspect.
#[derive(Debug, Clone)]
pub struct NetworkHandle {
    pub network_id: String,
    pub network_name: String,
}

/// High-level network inspect view.
#[derive(Debug, Clone)]
pub struct NetworkInspect {
    pub network_id: String,
    pub network_name: String,
    pub exists: bool,
    pub driver: Option<String>,
    pub subnet: Option<String>,
}

/// Port publish on the host.
#[derive(Debug, Clone)]
pub struct PortBinding {
    pub container_port: String, // e.g. "80/tcp"
    pub host_ip: Option<String>,
    pub host_port: Option<String>,
}

/// Resource + security limits for a container.
#[derive(Debug, Clone, Default)]
pub struct ResourceLimits {
    pub cpu_millis: Option<i64>,
    pub memory_bytes: Option<i64>,
    pub pids_limit: Option<i64>,
    pub cap_drop: Vec<String>,
    pub privileged: bool,
    pub extra_hosts: Vec<String>,
}

/// Optional Docker healthcheck configuration.
#[derive(Debug, Clone)]
pub struct HealthcheckSpec {
    pub test: Vec<String>,
    pub interval_secs: i64,
    pub timeout_secs: i64,
    pub retries: i64,
    pub start_period_secs: i64,
}

/// Generic container create request (mode-agnostic).
#[derive(Debug, Clone)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub env: Vec<String>,
    pub labels: HashMap<String, String>,
    /// Attach to this network name (also sets network_mode when no host networking).
    pub network_name: Option<String>,
    pub fixed_ip: Option<String>,
    pub port_bindings: Vec<PortBinding>,
    pub auto_remove: bool,
    pub resources: ResourceLimits,
    /// When set, overrides network_mode (e.g. "host").
    pub network_mode: Option<String>,
    pub healthcheck: Option<HealthcheckSpec>,
}

/// Handle returned after create/start.
#[derive(Debug, Clone)]
pub struct ContainerHandle {
    pub container_id: String,
    pub container_name: String,
}

/// Inspect view shared by Jeopardy and AWD.
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
}

/// Filter for listing containers.
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
