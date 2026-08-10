//! AWD container runtime — unified Docker lifecycle for AWD events.
//!
//! Provides the `AwdContainerRuntime` trait and its `Docker` implementation
//! for managing event networks, infrastructure containers, and GameBox
//! instances with AWD security constraints.

use bollard::Docker;
use std::collections::HashMap;
use uuid::Uuid;

// Re-export model types for convenience
pub use super::model::{
    ContainerHandle, ContainerSpec, ContainerState, HealthcheckSpec, NetworkHandle,
    NetworkInspect as NetworkState, ResourceLimits,
};

// ---------------------------------------------------------------------------
// Spec types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EventNetworkSpec {
    pub event_id: Uuid,
    pub network_name: String,
    pub subnet_cidr: String,
    pub internal: bool,
}

#[derive(Debug, Clone)]
pub struct InfrastructureContainerSpec {
    pub event_id: Uuid,
    pub container_name: String,
    pub image_ref: String,
    pub network_name: String,
    pub fixed_ip: String,
    pub env: Vec<String>,
    pub cpu_millis: Option<i64>,
    pub memory_bytes: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct GameBoxSpec {
    pub event_id: Uuid,
    pub team_id: Uuid,
    /// EventGameBox（逻辑靶机定义）；不再使用旧 template 概念。
    pub event_gamebox_id: Uuid,
    pub instance_id: Uuid,
    /// 当前 runtime generation（首次=1，Reset 后 +1；仅标签/审计，不参与逻辑身份）。
    pub runtime_generation: i64,
    pub container_name: String,
    pub image_ref: String,
    pub network_name: String,
    pub fixed_ip: String,
    pub username: String,
    pub password: String,
    pub cpu_millis: i64,
    pub memory_bytes: i64,
    pub pids_limit: i64,
    /// Docker-level healthcheck (CMD/CMD-SHELL). Manifest HTTP/TCP readiness
    /// probes are a separate concern and are NOT stored here.
    pub healthcheck: Option<HealthcheckSpec>,
    pub extra_hosts: Vec<String>,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct GameBoxResetSpec {
    pub event_id: Uuid,
    pub team_id: Uuid,
    pub event_gamebox_id: Uuid,
    pub instance_id: Uuid,
    pub container_name: String,
    pub recreate_spec: GameBoxSpec,
}

// ---------------------------------------------------------------------------
// Runtime trait
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
pub trait AwdContainerRuntime: Send + Sync {
    async fn create_event_network(&self, spec: EventNetworkSpec) -> anyhow::Result<NetworkHandle>;

    async fn inspect_event_network(&self, network_id: &str) -> anyhow::Result<NetworkState>;

    async fn remove_event_network(&self, network_id: &str) -> anyhow::Result<()>;

    async fn create_infrastructure_container(
        &self,
        spec: InfrastructureContainerSpec,
    ) -> anyhow::Result<ContainerHandle>;

    async fn create_gamebox(&self, spec: GameBoxSpec) -> anyhow::Result<ContainerHandle>;

    async fn reset_gamebox(&self, spec: GameBoxResetSpec) -> anyhow::Result<ContainerHandle>;

    async fn stop_container(&self, container_id: &str) -> anyhow::Result<()>;

    async fn remove_container(&self, container_id: &str) -> anyhow::Result<()>;

    async fn inspect_container(&self, container_id: &str) -> anyhow::Result<ContainerState>;

    async fn list_event_containers(&self, event_id: Uuid) -> anyhow::Result<Vec<ContainerState>>;

    async fn container_logs(&self, container_id: &str, limit: usize)
    -> anyhow::Result<Vec<String>>;
}

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

pub fn awd_labels(
    event_id: Uuid,
    team_id: Uuid,
    instance_id: Uuid,
    event_gamebox_id: Uuid,
    runtime_generation: i64,
    resource_kind: &str,
) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    labels.insert("awd.event_id".to_string(), event_id.to_string());
    labels.insert("awd.team_id".to_string(), team_id.to_string());
    labels.insert(
        "awd.gamebox_instance_id".to_string(),
        instance_id.to_string(),
    );
    labels.insert(
        "awd.event_gamebox_id".to_string(),
        event_gamebox_id.to_string(),
    );
    labels.insert(
        "awd.runtime_generation".to_string(),
        runtime_generation.to_string(),
    );
    labels.insert("awd.resource_kind".to_string(), resource_kind.to_string());
    labels
}

// ---------------------------------------------------------------------------
// Docker implementation
// ---------------------------------------------------------------------------

pub struct DockerRuntime {
    docker: Docker,
}

impl DockerRuntime {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    pub fn inner(&self) -> &Docker {
        &self.docker
    }
}

#[async_trait::async_trait]
impl AwdContainerRuntime for DockerRuntime {
    async fn create_event_network(&self, spec: EventNetworkSpec) -> anyhow::Result<NetworkHandle> {
        use super::{ContainerRuntime, DockerContainerRuntime, NetworkSpec};

        let rt = DockerContainerRuntime::new(self.docker.clone());
        let handle = rt
            .create_network(NetworkSpec {
                name: spec.network_name.clone(),
                subnet_cidr: spec.subnet_cidr,
                internal: spec.internal,
                bridge_name: Some(spec.network_name.clone()),
                check_duplicate: true,
            })
            .await?;

        Ok(NetworkHandle {
            network_id: handle.network_id,
            network_name: handle.network_name,
        })
    }

    async fn inspect_event_network(&self, network_id: &str) -> anyhow::Result<NetworkState> {
        use super::{ContainerRuntime, DockerContainerRuntime};
        let rt = DockerContainerRuntime::new(self.docker.clone());
        let net = rt.inspect_network(network_id).await?;
        Ok(NetworkState {
            network_id: net.network_id,
            network_name: net.network_name,
            subnet: net.subnet,
            exists: net.exists,
            driver: net.driver,
        })
    }

    async fn remove_event_network(&self, network_id: &str) -> anyhow::Result<()> {
        use super::{ContainerRuntime, DockerContainerRuntime};
        let rt = DockerContainerRuntime::new(self.docker.clone());
        rt.remove_network(network_id).await
    }

    async fn create_infrastructure_container(
        &self,
        spec: InfrastructureContainerSpec,
    ) -> anyhow::Result<ContainerHandle> {
        use super::{ContainerRuntime, ContainerSpec, DockerContainerRuntime, ResourceLimits};

        let labels = awd_labels(
            spec.event_id,
            Uuid::nil(),
            Uuid::nil(),
            Uuid::nil(),
            0,
            "infrastructure",
        );

        let cpu = spec.cpu_millis.unwrap_or(500);
        let mem = spec.memory_bytes.unwrap_or(256 * 1024 * 1024);

        let rt = DockerContainerRuntime::new(self.docker.clone());
        use super::image::ImageRuntime;
        ImageRuntime::ensure_image(&rt, &spec.image_ref, None)
            .await
            .map_err(|e| anyhow::anyhow!("ensure infra image {}: {e}", spec.image_ref))?;

        let handle = rt
            .create_and_start(ContainerSpec {
                name: spec.container_name.clone(),
                image: spec.image_ref,
                env: spec.env,
                labels,
                network_name: Some(spec.network_name),
                fixed_ip: Some(spec.fixed_ip),
                port_bindings: vec![],
                auto_remove: false,
                resources: ResourceLimits {
                    cpu_millis: Some(cpu),
                    memory_bytes: Some(mem),
                    pids_limit: Some(50),
                    cap_drop: vec![],
                    privileged: false,
                    extra_hosts: vec![],
                },
                network_mode: None,
                healthcheck: None,
            })
            .await?;

        Ok(ContainerHandle {
            container_id: handle.container_id,
            container_name: handle.container_name,
        })
    }

    async fn create_gamebox(&self, spec: GameBoxSpec) -> anyhow::Result<ContainerHandle> {
        use super::image::ImageRuntime;
        use super::{ContainerRuntime, ContainerSpec, DockerContainerRuntime, ResourceLimits};

        let labels = awd_labels(
            spec.event_id,
            spec.team_id,
            spec.instance_id,
            spec.event_gamebox_id,
            spec.runtime_generation,
            "gamebox",
        );

        let rt = DockerContainerRuntime::new(self.docker.clone());
        // Ensure pinned image is present locally (inspect; pull by digest/tag if missing).
        // Never rebuild from package — Runtime only uses the immutable pin from Revision.
        ImageRuntime::ensure_image(&rt, &spec.image_ref, None)
            .await
            .map_err(|e| anyhow::anyhow!("ensure gamebox image {}: {e}", spec.image_ref))?;

        let handle = rt
            .create_and_start(ContainerSpec {
                name: spec.container_name.clone(),
                image: spec.image_ref,
                env: vec![
                    format!("CTF_USER={}", spec.username),
                    format!("CTF_PASSWORD={}", spec.password),
                ],
                labels,
                network_name: Some(spec.network_name),
                fixed_ip: Some(spec.fixed_ip),
                port_bindings: vec![],
                auto_remove: false,
                resources: ResourceLimits {
                    cpu_millis: Some(spec.cpu_millis),
                    memory_bytes: Some(spec.memory_bytes),
                    pids_limit: Some(spec.pids_limit),
                    cap_drop: vec![
                        "NET_ADMIN".to_string(),
                        "NET_RAW".to_string(),
                        "SYS_ADMIN".to_string(),
                    ],
                    privileged: false,
                    extra_hosts: spec.extra_hosts,
                },
                network_mode: None,
                healthcheck: spec.healthcheck,
            })
            .await?;

        Ok(ContainerHandle {
            container_id: handle.container_id,
            container_name: handle.container_name,
        })
    }

    async fn reset_gamebox(&self, spec: GameBoxResetSpec) -> anyhow::Result<ContainerHandle> {
        use super::{ContainerRuntime, DockerContainerRuntime, IMMEDIATE_STOP_TIMEOUT};
        let rt = DockerContainerRuntime::new(self.docker.clone());
        rt.stop_and_remove(&spec.container_name, IMMEDIATE_STOP_TIMEOUT)
            .await?;
        self.create_gamebox(spec.recreate_spec).await
    }

    async fn stop_container(&self, container_id: &str) -> anyhow::Result<()> {
        use super::{ContainerRuntime, DEFAULT_STOP_TIMEOUT, DockerContainerRuntime};
        let rt = DockerContainerRuntime::new(self.docker.clone());
        rt.stop_container(container_id, DEFAULT_STOP_TIMEOUT).await
    }

    async fn remove_container(&self, container_id: &str) -> anyhow::Result<()> {
        use super::{ContainerRuntime, DockerContainerRuntime};
        let rt = DockerContainerRuntime::new(self.docker.clone());
        rt.remove_container(container_id, true).await
    }

    async fn inspect_container(&self, container_id: &str) -> anyhow::Result<ContainerState> {
        use super::{ContainerRuntime, DockerContainerRuntime};
        let rt = DockerContainerRuntime::new(self.docker.clone());
        rt.inspect_container(container_id).await
    }

    async fn list_event_containers(&self, event_id: Uuid) -> anyhow::Result<Vec<ContainerState>> {
        use super::{ContainerFilter, ContainerRuntime, DockerContainerRuntime};
        let rt = DockerContainerRuntime::new(self.docker.clone());
        rt.list_containers(
            ContainerFilter::default()
                .all()
                .with_label("awd.event_id", event_id.to_string()),
        )
        .await
    }

    async fn container_logs(
        &self,
        container_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<String>> {
        use super::{ContainerRuntime, DockerContainerRuntime};
        let rt = DockerContainerRuntime::new(self.docker.clone());
        rt.logs(container_id, limit).await
    }
}
