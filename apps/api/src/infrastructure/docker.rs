use bollard::{API_DEFAULT_VERSION, Docker};

use crate::core::config::DockerConfig;

pub async fn connect(config: &DockerConfig) -> anyhow::Result<Docker> {
    let docker = Docker::connect_with_unix(&config.socket_path, 120, API_DEFAULT_VERSION)?;
    let ping = docker.ping().await?;
    tracing::info!(socket = %config.socket_path, response = %ping, "Docker connected through floatctf-helper");
    Ok(docker)
}
