//! Docker 运行时实现（bollard）。

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

/// 统一 Docker 运行时，供 Jeopardy 实例、AWD 与 CLI 使用。
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

/// [`ContainerRuntime`] 的默认 Docker 实现。
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

    /// Challenge-compatible thin wrapper around [`crate::runtime::ImageRuntime::build_image`].
    ///
    /// Prefer the typed `ImageBuildRequest` API via `ImageRuntime` for new call sites
    /// (this inherent method shadows the trait method on the concrete type).
    pub async fn build_image(
        &self,
        image_tag: &str,
        context_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        use crate::runtime::image::{ImageBuildRequest, ImageRuntime};
        use std::time::Duration;

        let req = ImageBuildRequest {
            context_dir: context_path.to_path_buf(),
            dockerfile: "Dockerfile".into(),
            target_ref: image_tag.to_string(),
            labels: Default::default(),
            timeout: Duration::from_secs(600),
            verbose: false,
            build_proxy: None,
        };
        ImageRuntime::build_image(self, req)
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!(e))
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

        let container_name = spec.name.clone();
        let options = CreateContainerOptionsBuilder::new()
            .name(&container_name)
            .build();

        let body = spec_to_create_body(spec);

        let created = self.docker.create_container(Some(options), body).await?;
        Ok(ContainerHandle {
            container_id: created.id,
            container_name,
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
            // bollard 0.19：IP 位于 NetworkSettings.Networks[].ip_address（顶层字段已废弃）。
            ip_address: info
                .network_settings
                .as_ref()
                .and_then(|n| n.networks.as_ref())
                .and_then(|nets| {
                    nets.iter()
                        .find_map(|(_, ep)| ep.ip_address.clone().filter(|ip| !ip.is_empty()))
                }),
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
            // 409 = removal already in progress (e.g. auto_remove fired on stop and
            // is racing this explicit remove). Same end state as 404: it's going away.
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 409, ..
            }) => {
                info!("container {} removal already in progress", id_or_name);
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

/// 将与赛制无关的 [`ContainerSpec`] 转为 bollard 容器创建体。
///
/// 纯转换，与 Docker API 调用分离，使映射规则
/// （CPU 配额计算、健康检查纳秒换算、端口/网络接线）
/// 可在无 daemon 时做单元测试。
pub(crate) fn spec_to_create_body(spec: ContainerSpec) -> bollard::secret::ContainerCreateBody {
    use bollard::secret::{HostConfig, NetworkingConfig};

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
        let mut map = std::collections::HashMap::new();
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
            let mut endpoints = std::collections::HashMap::new();
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

    bollard::secret::ContainerCreateBody {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::model::{HealthcheckSpec, PortBinding, ResourceLimits};

    fn base_spec() -> ContainerSpec {
        ContainerSpec {
            name: "test-c".into(),
            image: "img:v1".into(),
            env: vec!["A=1".into()],
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            port_bindings: vec![],
            auto_remove: true,
            resources: ResourceLimits::default(),
            network_mode: None,
            healthcheck: None,
        }
    }

    #[test]
    fn body_passes_basic_fields() {
        let mut spec = base_spec();
        spec.labels.insert("k".into(), "v".into());
        let body = spec_to_create_body(spec);
        assert_eq!(body.image.as_deref(), Some("img:v1"));
        assert_eq!(body.env.as_ref(), Some(&vec!["A=1".to_string()]));
        assert_eq!(
            body.labels.as_ref().and_then(|m| m.get("k")),
            Some(&"v".to_string())
        );
        let hc = body.host_config.as_ref().unwrap();
        assert_eq!(hc.auto_remove, Some(true));
        assert_eq!(hc.privileged, Some(false));
    }

    #[test]
    fn body_empty_env_and_labels_become_none() {
        let spec = ContainerSpec {
            env: vec![],
            ..base_spec()
        };
        let body = spec_to_create_body(spec);
        assert!(body.env.is_none(), "empty env must be None");
        assert!(body.labels.is_none(), "empty labels must be None");
    }

    #[test]
    fn body_cpu_quota_math() {
        let mut spec = base_spec();
        spec.resources.cpu_millis = Some(1000);
        let body = spec_to_create_body(spec);
        let hc = body.host_config.as_ref().unwrap();
        assert_eq!(hc.cpu_period, Some(100_000));
        assert_eq!(hc.cpu_quota, Some(100_000));

        let mut spec = base_spec();
        spec.resources.cpu_millis = Some(250);
        let body = spec_to_create_body(spec);
        assert_eq!(body.host_config.unwrap().cpu_quota, Some(25_000));
    }

    #[test]
    fn body_memory_pids_and_caps() {
        let mut spec = base_spec();
        spec.resources.memory_bytes = Some(512 * 1024 * 1024);
        spec.resources.pids_limit = Some(100);
        spec.resources.cap_drop = vec!["NET_ADMIN".into()];
        spec.resources.extra_hosts = vec!["host:1.2.3.4".into()];
        let body = spec_to_create_body(spec);
        let hc = body.host_config.as_ref().unwrap();
        assert_eq!(hc.memory, Some(512 * 1024 * 1024));
        assert_eq!(hc.pids_limit, Some(100));
        assert_eq!(hc.cap_drop.as_ref(), Some(&vec!["NET_ADMIN".to_string()]));
        assert_eq!(
            hc.extra_hosts.as_ref(),
            Some(&vec!["host:1.2.3.4".to_string()])
        );
    }

    #[test]
    fn body_port_bindings_default_host_ip() {
        let mut spec = base_spec();
        spec.port_bindings = vec![
            PortBinding {
                container_port: "80/tcp".into(),
                host_ip: None,
                host_port: Some("8080".into()),
            },
            PortBinding {
                container_port: "53/udp".into(),
                host_ip: Some("127.0.0.1".into()),
                host_port: None,
            },
        ];
        let body = spec_to_create_body(spec);
        let hc = body.host_config.as_ref().unwrap();
        let pb = hc.port_bindings.as_ref().unwrap();
        let first = pb.get("80/tcp").unwrap().as_ref().unwrap();
        assert_eq!(first[0].host_ip.as_deref(), Some("0.0.0.0"));
        assert_eq!(first[0].host_port.as_deref(), Some("8080"));
        let second = pb.get("53/udp").unwrap().as_ref().unwrap();
        assert_eq!(second[0].host_ip.as_deref(), Some("127.0.0.1"));
        assert!(second[0].host_port.is_none());
    }

    #[test]
    fn body_network_mode_overrides_network_name() {
        let mut spec = base_spec();
        spec.network_name = Some("br0".into());
        spec.network_mode = Some("host".into());
        let body = spec_to_create_body(spec);
        assert_eq!(
            body.host_config.as_ref().unwrap().network_mode.as_deref(),
            Some("host")
        );

        let mut spec = base_spec();
        spec.network_name = Some("br0".into());
        let body = spec_to_create_body(spec);
        assert_eq!(
            body.host_config.as_ref().unwrap().network_mode.as_deref(),
            Some("br0")
        );
    }

    #[test]
    fn body_fixed_ip_requires_network() {
        // fixed_ip without network_name -> no networking config
        let mut spec = base_spec();
        spec.fixed_ip = Some("10.0.0.5".into());
        let body = spec_to_create_body(spec);
        assert!(body.networking_config.is_none());

        // both set -> endpoint with ipam ipv4
        let mut spec = base_spec();
        spec.network_name = Some("evt-net".into());
        spec.fixed_ip = Some("10.0.0.5".into());
        let body = spec_to_create_body(spec);
        let endpoints = body.networking_config.unwrap().endpoints_config.unwrap();
        let ep = endpoints.get("evt-net").unwrap();
        assert_eq!(
            ep.ipam_config.as_ref().unwrap().ipv4_address.as_deref(),
            Some("10.0.0.5")
        );
    }

    #[test]
    fn body_healthcheck_converts_to_nanoseconds() {
        let mut spec = base_spec();
        spec.healthcheck = Some(HealthcheckSpec {
            test: vec!["CMD-SHELL".into(), "pgrep sshd".into()],
            interval_secs: 30,
            timeout_secs: 10,
            retries: 3,
            start_period_secs: 60,
        });
        let body = spec_to_create_body(spec);
        let hc = body.healthcheck.unwrap();
        assert_eq!(hc.test, Some(vec!["CMD-SHELL".into(), "pgrep sshd".into()]));
        assert_eq!(hc.interval, Some(30_000_000_000));
        assert_eq!(hc.timeout, Some(10_000_000_000));
        assert_eq!(hc.retries, Some(3));
        assert_eq!(hc.start_period, Some(60_000_000_000));
    }
}
