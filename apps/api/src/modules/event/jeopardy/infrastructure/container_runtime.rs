//! 实例应用服务使用的容器运行时适配。
//!
//! 对 `fcmc::ContainerRuntime` + `ImageRuntime` 的薄封装，避免 Jeopardy
//! 生命周期直接调用 bollard 或零散辅助函数。
//!
//! 运行契约来自 Challenge 身份行（单版本模型）：镜像为不可变钉扎
//! （RepoDigest 优先于 image_id），动态 Flag 经固定环境变量 `FLAG` 注入。

use async_trait::async_trait;
use bollard::Docker;
use fcmc::{
    ContainerRuntime, ContainerSpec, DockerContainerRuntime, IMMEDIATE_STOP_TIMEOUT, ImageRuntime,
    PortBinding, ResourceLimits,
};

/// 由 Challenge 身份行解析的运行契约（单版本）。
#[derive(Debug, Clone)]
pub struct ChallengeRuntimeSpec {
    /// Pinned image: `image_repo_digest` > `image_id` (ready challenges always pinned).
    pub image_ref: String,
    /// Single exposed TCP port (`[docker].port`).
    pub container_port: u16,
    /// `Some(flag)` → dynamic: inject `FLAG=<flag>` (entrypoint writes /flag, then unsets).
    /// `None` → static: image has the baked flag; NO `FLAG` env injected.
    pub flag: Option<String>,
    /// Resolved resource limits (revision recommendation; no event override in v1).
    pub cpu_millis: i64,
    pub memory_bytes: i64,
    pub pids_limit: i64,
}

#[async_trait]
pub trait InstanceRuntime: Send + Sync {
    async fn launch(&self, spec: &ChallengeRuntimeSpec, identifier: &str) -> anyhow::Result<u16>;
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
    spec: &ChallengeRuntimeSpec,
    identifier: &str,
) -> (ContainerSpec, String) {
    let mut env = Vec::new();
    if let Some(flag) = &spec.flag {
        // FloatCTF internal runtime contract: dynamic flag env is always FLAG.
        env.push(format!("FLAG={flag}"));
    }

    let container_port = format!("{}/tcp", spec.container_port);
    (
        ContainerSpec {
            name: identifier.to_string(),
            image: spec.image_ref.clone(),
            env,
            labels: Default::default(),
            network_name: None,
            fixed_ip: None,
            port_bindings: vec![PortBinding {
                container_port: container_port.clone(),
                host_ip: Some("0.0.0.0".into()),
                host_port: None,
            }],
            auto_remove: true,
            resources: ResourceLimits {
                cpu_millis: Some(spec.cpu_millis),
                memory_bytes: Some(spec.memory_bytes),
                pids_limit: Some(spec.pids_limit),
                ..Default::default()
            },
            network_mode: None,
            healthcheck: None,
        },
        container_port,
    )
}

#[async_trait]
impl InstanceRuntime for DockerInstanceRuntime {
    async fn launch(&self, spec: &ChallengeRuntimeSpec, identifier: &str) -> anyhow::Result<u16> {
        // Ensure the pinned image exists locally (pull exact RepoDigest if missing);
        // never rebuild from package and never trust a mutable tag.
        ImageRuntime::ensure_image(&self.runtime, &spec.image_ref, None)
            .await
            .map_err(|e| anyhow::anyhow!("ensure image {}: {e}", spec.image_ref))?;

        let (container_spec, container_port) = challenge_container_spec(spec, identifier);
        let handle = self.runtime.create_and_start(container_spec).await?;
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
        let spec = ChallengeRuntimeSpec {
            image_ref: "floatctf/challenges/web:1.0.0".into(),
            container_port: 8080,
            flag: Some("flag{test}".into()),
            cpu_millis: 500,
            memory_bytes: 268_435_456,
            pids_limit: 100,
        };

        let (container_spec, container_port) = challenge_container_spec(&spec, "instance-1");

        assert_eq!(container_spec.name, "instance-1");
        assert_eq!(container_spec.image, "floatctf/challenges/web:1.0.0");
        assert_eq!(container_spec.env, vec!["FLAG=flag{test}"]);
        assert_eq!(container_port, "8080/tcp");
        assert_eq!(container_spec.port_bindings.len(), 1);
        assert_eq!(container_spec.port_bindings[0].container_port, "8080/tcp");
        assert_eq!(
            container_spec.port_bindings[0].host_ip.as_deref(),
            Some("0.0.0.0")
        );
        assert_eq!(container_spec.port_bindings[0].host_port, None);
        assert!(container_spec.auto_remove);
        assert_eq!(container_spec.resources.cpu_millis, Some(500));
        assert_eq!(container_spec.resources.memory_bytes, Some(268_435_456));
        assert_eq!(container_spec.resources.pids_limit, Some(100));
    }

    #[test]
    fn static_flag_injects_no_env() {
        let spec = ChallengeRuntimeSpec {
            image_ref: "floatctf/challenges/static:1.0.0".into(),
            container_port: 80,
            flag: None,
            cpu_millis: 500,
            memory_bytes: 268_435_456,
            pids_limit: 100,
        };
        let (container_spec, _) = challenge_container_spec(&spec, "i");
        assert!(container_spec.env.is_empty());
    }
}
