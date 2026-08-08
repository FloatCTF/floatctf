//! AWD event orchestration.
//!
//! High-level operations for managing AWD event networks, infrastructure
//! containers, and GameBox instances.

use anyhow::{Context, Result};
use bollard::Docker;
use uuid::Uuid;

use crate::metadata::GameBoxMeta;
use crate::runtime::awd::{
    AwdContainerRuntime, DockerRuntime, EventNetworkSpec, GameBoxResetSpec, GameBoxSpec,
};

/// AWD application context.
pub struct AwdApp {
    runtime: DockerRuntime,
}

impl AwdApp {
    /// Create a new AWD application with a Docker connection.
    pub fn new(docker: Docker) -> Self {
        Self {
            runtime: DockerRuntime::new(docker),
        }
    }

    /// Get a reference to the underlying runtime.
    pub fn runtime(&self) -> &impl AwdContainerRuntime {
        &self.runtime
    }

    /// Create an event network.
    pub async fn create_event_network(
        &self,
        event_id: Uuid,
        network_name: String,
        subnet_cidr: String,
        internal: bool,
    ) -> Result<String> {
        let handle = self
            .runtime
            .create_event_network(EventNetworkSpec {
                event_id,
                network_name,
                subnet_cidr,
                internal,
            })
            .await
            .context("Failed to create event network")?;

        Ok(handle.network_id)
    }

    /// Create a GameBox from metadata.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_gamebox(
        &self,
        event_id: Uuid,
        team_id: Uuid,
        event_gamebox_id: Uuid,
        instance_id: Uuid,
        runtime_generation: i64,
        meta: &GameBoxMeta,
        container_name: String,
        network_name: String,
        fixed_ip: String,
        password: String,
    ) -> Result<String> {
        let spec = GameBoxSpec {
            event_id,
            team_id,
            event_gamebox_id,
            instance_id,
            runtime_generation,
            container_name,
            image_ref: meta.gamebox.image_tag.clone(),
            network_name,
            fixed_ip,
            username: meta.gamebox.username.clone(),
            password,
            cpu_millis: meta.gamebox.resources.cpu_millis,
            memory_bytes: meta.gamebox.resources.memory_bytes,
            pids_limit: meta.gamebox.resources.pids_limit,
            healthcheck: meta.gamebox.healthcheck.clone(),
            extra_hosts: vec![],
            labels: crate::runtime::awd::awd_labels(
                event_id,
                team_id,
                instance_id,
                event_gamebox_id,
                runtime_generation,
                "gamebox",
            ),
        };

        let handle = self
            .runtime
            .create_gamebox(spec)
            .await
            .context("Failed to create gamebox")?;

        Ok(handle.container_id)
    }

    /// Reset a GameBox (stop, remove, recreate).
    pub async fn reset_gamebox(
        &self,
        event_id: Uuid,
        team_id: Uuid,
        event_gamebox_id: Uuid,
        instance_id: Uuid,
        container_name: String,
        recreate_spec: GameBoxSpec,
    ) -> Result<String> {
        let handle = self
            .runtime
            .reset_gamebox(GameBoxResetSpec {
                event_id,
                team_id,
                event_gamebox_id,
                instance_id,
                container_name,
                recreate_spec,
            })
            .await
            .context("Failed to reset gamebox")?;

        Ok(handle.container_id)
    }

    /// Stop a container.
    pub async fn stop_container(&self, container_id: &str) -> Result<()> {
        self.runtime
            .stop_container(container_id)
            .await
            .context("Failed to stop container")
    }

    /// Remove a container.
    pub async fn remove_container(&self, container_id: &str) -> Result<()> {
        self.runtime
            .remove_container(container_id)
            .await
            .context("Failed to remove container")
    }

    /// List all containers for an event.
    pub async fn list_event_containers(&self, event_id: Uuid) -> Result<Vec<String>> {
        let states = self
            .runtime
            .list_event_containers(event_id)
            .await
            .context("Failed to list event containers")?;

        Ok(states.into_iter().map(|s| s.container_id).collect())
    }
}
