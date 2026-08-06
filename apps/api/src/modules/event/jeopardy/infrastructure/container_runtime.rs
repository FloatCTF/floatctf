//! Runtime adapter used by the instance application service.
//!
//! Thin façade over `fcmc::ContainerRuntime` so Jeopardy lifecycle does not
//! call bollard or free helpers directly.

use async_trait::async_trait;
use bollard::Docker;
use fcmc::{
    ChallengeMeta, ContainerRuntime, ContainerSpec, DockerContainerRuntime, IMMEDIATE_STOP_TIMEOUT,
    PortBinding, ResourceLimits,
};

#[async_trait]
pub trait InstanceRuntime: Send + Sync {
    async fn launch(
        &self,
        metadata: &ChallengeMeta,
        identifier: &str,
        flag: &str,
    ) -> anyhow::Result<u16>;
    async fn stop_and_remove(&self, identifier: &str) -> anyhow::Result<()>;
}

pub struct DockerInstanceRuntime {
    runtime: DockerContainerRuntime,
}

impl DockerInstanceRuntime {
    pub fn new(docker: Docker) -> Self {
        Self {
            runtime: DockerContainerRuntime::new(docker),
        }
    }
}

fn challenge_container_spec(
    metadata: &ChallengeMeta,
    identifier: &str,
    flag: &str,
) -> anyhow::Result<(ContainerSpec, String)> {
    let docker = metadata
        .docker
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No docker config found"))?;
    Ok((
        ContainerSpec {
            name: identifier.to_string(),
            image: docker.image_tag.clone(),
            env: vec![format!("{}={flag}", metadata.flag.env_var)],
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            port_bindings: vec![PortBinding {
                container_port: docker.port.clone(),
                host_ip: Some("0.0.0.0".into()),
                host_port: None,
            }],
            auto_remove: true,
            resources: ResourceLimits::default(),
            network_mode: None,
            healthcheck: None,
        },
        docker.port.clone(),
    ))
}

#[async_trait]
impl InstanceRuntime for DockerInstanceRuntime {
    async fn launch(
        &self,
        metadata: &ChallengeMeta,
        identifier: &str,
        flag: &str,
    ) -> anyhow::Result<u16> {
        let (spec, container_port) = challenge_container_spec(metadata, identifier, flag)?;
        let handle = self.runtime.create_and_start(spec).await?;
        let state = self.runtime.inspect_container(&handle.container_id).await?;
        state
            .published_ports
            .get(&container_port)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Host port not found for {container_port}"))
    }

    async fn stop_and_remove(&self, identifier: &str) -> anyhow::Result<()> {
        self.runtime
            .stop_and_remove(identifier, IMMEDIATE_STOP_TIMEOUT)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_spec_preserves_runtime_contract() {
        let metadata = ChallengeMeta::from_toml_str(
            r#"
name = "web"
author = "author"
category = "Web"
description = "test"

[flag]
value = ""
env_var = "CHALLENGE_FLAG"

[docker]
image_tag = "floatctf/web:latest"
port = "8080/tcp"
is_nc = false
"#,
        )
        .unwrap();

        let (spec, container_port) =
            challenge_container_spec(&metadata, "instance-1", "flag{test}").unwrap();

        assert_eq!(spec.name, "instance-1");
        assert_eq!(spec.image, "floatctf/web:latest");
        assert_eq!(spec.env, vec!["CHALLENGE_FLAG=flag{test}"]);
        assert_eq!(container_port, "8080/tcp");
        assert_eq!(spec.port_bindings.len(), 1);
        assert_eq!(spec.port_bindings[0].container_port, "8080/tcp");
        assert_eq!(spec.port_bindings[0].host_ip.as_deref(), Some("0.0.0.0"));
        assert_eq!(spec.port_bindings[0].host_port, None);
        assert!(spec.auto_remove);
    }
}
