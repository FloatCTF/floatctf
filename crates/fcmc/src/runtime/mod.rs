//! Unified Docker container + image runtime.

pub mod awd;
pub mod docker;
pub mod image;
pub mod model;

pub use docker::{ContainerRuntime, DockerContainerRuntime};
pub use image::{
    ImageBuildRequest, ImageBuildResult, ImageError, ImageInspect, ImageRuntime, RegistryAuth,
    image_repository, pick_repo_digest, split_image_ref,
};
pub use model::{
    ContainerFilter, ContainerSpec, DEFAULT_STOP_TIMEOUT, HealthcheckSpec, IMMEDIATE_STOP_TIMEOUT,
    NetworkInspect, NetworkSpec, PortBinding, ResourceLimits,
};
// Prefer model handles for generic runtime; re-export model ContainerHandle/State/NetworkHandle
pub use model::{ContainerHandle, ContainerState, NetworkHandle};
