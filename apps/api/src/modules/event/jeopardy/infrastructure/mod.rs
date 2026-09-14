//! Jeopardy 共享基础设施（仓储与容器运行时适配）。

pub mod container_runtime;
pub mod instance_repository;
pub mod solve_repository;

pub use container_runtime::{DockerInstanceRuntime, InstanceRuntime};
pub use instance_repository as instance_repo;
pub use solve_repository as solve_repo;
