//! Bollard-backed unified container runtime.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use bollard::Docker;
use bollard::query_parameters::{
    InspectContainerOptions, ListContainersOptions, RemoveContainerOptions, StopContainerOptions,
};
use futures_util::StreamExt;
use tracing::{info, warn};

use super::model::{
    ContainerFilter, ContainerHandle, ContainerSpec, ContainerState, NetworkHandle, NetworkInspect,
    NetworkSpec,
};

/// Unified Docker runtime used by Jeopardy instances, AWD, and CLI.
#[async_trait]
pub trait ContainerRuntime: Send + Sync {
    async fn create_network(&self, spec: NetworkSpec) -> anyhow::Result<NetworkHandle>;
    async fn remove_network(&self, id_or_name: &str) -> anyhow::Result<()>;
    async fn inspect_network(&self, id_or_name: &str) -> anyhow::Result<NetworkInspect>;

    async fn create_container(&self, spec: ContainerSpec) -> anyhow::Result<ContainerHandle>;
    async fn start_container(&self, id_or_name: &str) -> anyhow::Result<()>;
    async fn create_and_start(&self, spec: ContainerSpec) -> anyhow::Result<ContainerHandle> {
        let handle = self.create_container(spec).await?;
        self.start_container(&handle.container_id).await?;
        Ok(handle)
    }

    async fn inspect_container(&self, id_or_name: &str) -> anyhow::Result<ContainerState>;
    async fn stop_container(&self, id_or_name: &str, timeout: Duration) -> anyhow::Result<()>;
    async fn remove_container(&self, id_or_name: &str, force: bool) -> anyhow::Result<()>;

    /// Stop then remove. Missing containers are success; other remove errors propagate.
    async fn stop_and_remove(
        &self,
        id_or_name: &str,
        stop_timeout: Duration,
    ) -> anyhow::Result<()> {
        if let Err(e) = self.stop_container(id_or_name, stop_timeout).await {
            warn!(
                "stop container {} before remove: {} (continuing to remove)",
                id_or_name, e
            );
        }
        self.remove_container(id_or_name, true).await
    }

    async fn list_containers(&self, filter: ContainerFilter)
    -> anyhow::Result<Vec<ContainerState>>;
    async fn logs(&self, id_or_name: &str, limit: usize) -> anyhow::Result<Vec<String>>;
}

/// Default Docker implementation of [`ContainerRuntime`].
pub struct DockerContainerRuntime {
    docker: Docker,
}

impl DockerContainerRuntime {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    pub fn from_defaults() -> anyhow::Result<Self> {
        Ok(Self::new(Docker::connect_with_defaults()?))
    }

    pub fn inner(&self) -> &Docker {
        &self.docker
    }

    /// Convenience matching the historical free-function default (immediate stop).
    pub async fn stop_and_remove_immediate(&self, id_or_name: &str) -> anyhow::Result<()> {
        self.stop_and_remove(id_or_name, Duration::from_secs(0))
            .await
    }

    /// Build a Docker image from a build context directory (tar streamed to the daemon).
    pub async fn build_image(
        &self,
        image_tag: &str,
        context_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        use bollard::{body_full, query_parameters::BuildImageOptionsBuilder, secret::BuildInfo};
        use std::fs::File;
        use std::io::Read;
        use tar::Builder;
        use tempfile::NamedTempFile;
        use tokio_util::bytes::Bytes;
        use tracing::error;

        let options = BuildImageOptionsBuilder::default().t(image_tag).build();

        let tmp = NamedTempFile::new()?;
        {
            let file = File::create(tmp.path())?;
            let mut tar_builder = Builder::new(file);
            tar_builder.append_dir_all(".", context_path)?;
            tar_builder.finish()?;
        }

        let mut buf = Vec::new();
        File::open(tmp.path())?.read_to_end(&mut buf)?;

        let body = body_full(Bytes::from(buf));

        let mut build_stream = self.docker.build_image(options, None, Some(body));

        let mut infos = Vec::new();
        while let Some(update) = build_stream.next().await {
            let info: BuildInfo = update?;

            if let Some(ref stream_msg) = info.stream {
                infos.push(stream_msg.trim().to_owned());
            }
            if let Some(ref err) = info.error {
                error!("ERROR: {}", err);
            }
        }

        for msg in infos {
            info!("{}", msg);
        }

        Ok(())
    }
}

#[async_trait]
impl ContainerRuntime for DockerContainerRuntime {
    async fn create_network(&self, spec: NetworkSpec) -> anyhow::Result<NetworkHandle> {
        // Recreate: best-effort remove first (legacy behaviour).
        let _ = self.docker.remove_network(&spec.name).await;

        let mut network_options = HashMap::new();
        if let Some(bridge) = &spec.bridge_name {
            network_options.insert("com.docker.network.bridge.name".to_string(), bridge.clone());
        } else {
            network_options.insert(
                "com.docker.network.bridge.name".to_string(),
                spec.name.clone(),
            );
        }

        #[allow(deprecated)]
        let conf = bollard::network::CreateNetworkOptions {
            name: spec.name.clone(),
            driver: "bridge".to_string(),
            internal: spec.internal,
            check_duplicate: spec.check_duplicate,
            ipam: bollard::secret::Ipam {
                config: Some(vec![bollard::secret::IpamConfig {
                    subnet: Some(spec.subnet_cidr),
                    ..Default::default()
                }]),
                ..Default::default()
            },
            options: network_options,
            ..Default::default()
        };

        let created = self.docker.create_network(conf).await?;
        let network_id = if created.id.is_empty() {
            spec.name.clone()
        } else {
            created.id
        };
        Ok(NetworkHandle {
            network_id,
            network_name: spec.name,
        })
    }

    async fn remove_network(&self, id_or_name: &str) -> anyhow::Result<()> {
        match self.docker.remove_network(id_or_name).await {
            Ok(()) => Ok(()),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn inspect_network(&self, id_or_name: &str) -> anyhow::Result<NetworkInspect> {
        match self
            .docker
            .inspect_network(
                id_or_name,
                None::<bollard::query_parameters::InspectNetworkOptions>,
            )
            .await
        {
            Ok(net) => {
                let subnet = net
                    .ipam
                    .as_ref()
                    .and_then(|i| i.config.as_ref())
                    .and_then(|c| c.first())
                    .and_then(|c| c.subnet.clone());
                Ok(NetworkInspect {
                    network_id: net.id.unwrap_or_default(),
                    network_name: net.name.unwrap_or_else(|| id_or_name.to_string()),
                    exists: true,
                    driver: net.driver,
                    subnet,
                })
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(NetworkInspect {
                network_id: String::new(),
                network_name: id_or_name.to_string(),
                exists: false,
                driver: None,
                subnet: None,
            }),
            Err(e) => Err(e.into()),
        }
    }

    async fn create_container(&self, spec: ContainerSpec) -> anyhow::Result<ContainerHandle> {
        use bollard::query_parameters::CreateContainerOptionsBuilder;
        use bollard::secret::{ContainerCreateBody, HostConfig, NetworkingConfig};

        let options = CreateContainerOptionsBuilder::new()
            .name(&spec.name)
            .build();

        let mut host_config = HostConfig {
            auto_remove: Some(spec.auto_remove),
            privileged: Some(spec.resources.privileged),
            ..Default::default()
        };

        if let Some(mode) = &spec.network_mode {
            host_config.network_mode = Some(mode.clone());
        } else if let Some(net) = &spec.network_name {
            host_config.network_mode = Some(net.clone());
        }

        if let Some(cpu) = spec.resources.cpu_millis {
            host_config.cpu_period = Some(100_000i64);
            host_config.cpu_quota = Some(cpu * 100);
        }
        if let Some(mem) = spec.resources.memory_bytes {
            host_config.memory = Some(mem);
        }
        if let Some(pids) = spec.resources.pids_limit {
            host_config.pids_limit = Some(pids);
        }
        if !spec.resources.cap_drop.is_empty() {
            host_config.cap_drop = Some(spec.resources.cap_drop.clone());
        }
        if !spec.resources.extra_hosts.is_empty() {
            host_config.extra_hosts = Some(spec.resources.extra_hosts.clone());
        }

        if !spec.port_bindings.is_empty() {
            let mut map = HashMap::new();
            for pb in &spec.port_bindings {
                map.insert(
                    pb.container_port.clone(),
                    Some(vec![bollard::models::PortBinding {
                        host_ip: pb.host_ip.clone().or_else(|| Some("0.0.0.0".into())),
                        host_port: pb.host_port.clone(),
                    }]),
                );
            }
            host_config.port_bindings = Some(map);
        }

        let networking_config =
            if let (Some(net), Some(ip)) = (spec.network_name.as_ref(), spec.fixed_ip.as_ref()) {
                let mut endpoints = HashMap::new();
                endpoints.insert(
                    net.clone(),
                    bollard::models::EndpointSettings {
                        ipam_config: Some(bollard::models::EndpointIpamConfig {
                            ipv4_address: Some(ip.clone()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                );
                Some(NetworkingConfig {
                    endpoints_config: Some(endpoints),
                })
            } else {
                None
            };

        let healthcheck = spec.healthcheck.map(|hc| bollard::secret::HealthConfig {
            test: Some(hc.test),
            interval: Some(hc.interval_secs * 1_000_000_000),
            timeout: Some(hc.timeout_secs * 1_000_000_000),
            retries: Some(hc.retries),
            start_period: Some(hc.start_period_secs * 1_000_000_000),
            ..Default::default()
        });

        let body = ContainerCreateBody {
            image: Some(spec.image),
            env: if spec.env.is_empty() {
                None
            } else {
                Some(spec.env)
            },
            labels: if spec.labels.is_empty() {
                None
            } else {
                Some(spec.labels)
            },
            healthcheck,
            host_config: Some(host_config),
            networking_config,
            ..Default::default()
        };

        let created = self.docker.create_container(Some(options), body).await?;
        Ok(ContainerHandle {
            container_id: created.id,
            container_name: spec.name,
        })
    }

    async fn start_container(&self, id_or_name: &str) -> anyhow::Result<()> {
        self.docker
            .start_container(
                id_or_name,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await?;
        Ok(())
    }

    async fn inspect_container(&self, id_or_name: &str) -> anyhow::Result<ContainerState> {
        let info = self
            .docker
            .inspect_container(id_or_name, None::<InspectContainerOptions>)
            .await?;

        let status = info
            .state
            .as_ref()
            .and_then(|s| s.status.as_ref())
            .map(|s| format!("{:?}", s).to_lowercase())
            .unwrap_or_else(|| "unknown".to_string());

        let mut published_ports = HashMap::new();
        if let Some(ports) = info
            .network_settings
            .as_ref()
            .and_then(|n| n.ports.as_ref())
        {
            for (container_port, bindings) in ports {
                if let Some(bindings) = bindings
                    && let Some(b) = bindings.first()
                    && let Some(hp) = &b.host_port
                    && let Ok(p) = hp.parse::<u16>()
                {
                    published_ports.insert(container_port.clone(), p);
                }
            }
        }

        Ok(ContainerState {
            container_id: info.id.unwrap_or_default(),
            container_name: info
                .name
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string(),
            image: info
                .config
                .as_ref()
                .and_then(|c| c.image.clone())
                .unwrap_or_default(),
            status,
            running: info.state.as_ref().and_then(|s| s.running).unwrap_or(false),
            labels: info
                .config
                .as_ref()
                .and_then(|c| c.labels.clone())
                .unwrap_or_default(),
            created_at: info.created,
            published_ports,
        })
    }

    async fn stop_container(&self, id_or_name: &str, timeout: Duration) -> anyhow::Result<()> {
        let options = StopContainerOptions {
            t: Some(timeout.as_secs() as i32),
            ..Default::default()
        };
        match self.docker.stop_container(id_or_name, Some(options)).await {
            Ok(()) => Ok(()),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404 | 304,
                ..
            }) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn remove_container(&self, id_or_name: &str, force: bool) -> anyhow::Result<()> {
        let options = RemoveContainerOptions {
            v: true,
            force,
            link: false,
        };
        match self
            .docker
            .remove_container(id_or_name, Some(options))
            .await
        {
            Ok(()) => {
                info!("container {} removed", id_or_name);
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                info!("container {} already gone", id_or_name);
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn list_containers(
        &self,
        filter: ContainerFilter,
    ) -> anyhow::Result<Vec<ContainerState>> {
        let filters = filter.to_bollard_filters();
        let options = ListContainersOptions {
            all: filter.all,
            filters: if filters.is_empty() {
                None
            } else {
                Some(filters)
            },
            ..Default::default()
        };
        let containers = self.docker.list_containers(Some(options)).await?;
        let mut states = Vec::new();
        for c in containers {
            if let Some(id) = &c.id {
                match self.inspect_container(id).await {
                    Ok(state) => states.push(state),
                    Err(e) => warn!("inspect after list failed for {}: {}", id, e),
                }
            }
        }
        Ok(states)
    }

    async fn logs(&self, id_or_name: &str, limit: usize) -> anyhow::Result<Vec<String>> {
        use bollard::query_parameters::LogsOptionsBuilder;

        let tail = if limit == 0 {
            "50".to_string()
        } else {
            limit.to_string()
        };
        let options = LogsOptionsBuilder::default()
            .stdout(true)
            .stderr(true)
            .tail(&tail)
            .build();
        let mut stream = self.docker.logs(id_or_name, Some(options));
        let mut lines = Vec::new();
        while let Some(Ok(msg)) = stream.next().await {
            lines.push(msg.to_string());
        }
        Ok(lines)
    }
}

pub use super::model::DEFAULT_STOP_TIMEOUT as STOP_TIMEOUT_DEFAULT;
