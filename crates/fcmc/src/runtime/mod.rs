//! Unified Docker container runtime.

pub mod awd;
pub mod docker;
pub mod model;

pub use docker::{ContainerRuntime, DockerContainerRuntime};
pub use model::{
    ContainerFilter, ContainerSpec, DEFAULT_STOP_TIMEOUT, HealthcheckSpec, IMMEDIATE_STOP_TIMEOUT,
    NetworkInspect, NetworkSpec, PortBinding, ResourceLimits,
};
// Prefer model handles for generic runtime; re-export model ContainerHandle/State/NetworkHandle
pub use model::{ContainerHandle, ContainerState, NetworkHandle};
