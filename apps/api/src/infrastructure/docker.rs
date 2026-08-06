//! Docker client initialization.

use anyhow::Result;
use bollard::Docker;
use tracing::info;

use crate::core::config::DockerConfig;

pub async fn connect(config: &DockerConfig) -> Result<Docker> {
    let _ = config; // reserved for socket/host overrides
    let docker = Docker::connect_with_defaults()?;
    let s = docker.ping().await?;
    info!("Docker connected {}", s);
    Ok(docker)
}
