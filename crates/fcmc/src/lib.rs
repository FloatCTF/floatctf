//! fcmc — FloatCTF container / metadata crate.
//!
//! - `metadata` — Challenge / GameBox YAML and config shapes
//! - `runtime` — Docker container lifecycle + AWD Specs
//! - `image` — reserved for image build helpers (see `main` CLI)

pub mod application;
pub mod metadata;
pub mod runtime;

// ── AWD high-level runtime (domain-specific Specs; names preserved) ──
pub use runtime::awd::{
    AwdContainerRuntime, ContainerHandle, ContainerState, DockerRuntime, EventNetworkSpec,
    GameBoxResetSpec, GameBoxSpec, InfrastructureContainerSpec, NetworkHandle, NetworkState,
    awd_labels,
};

// ── Unified low-level runtime ──
pub use runtime::{
    ContainerFilter, ContainerRuntime, ContainerSpec, DEFAULT_STOP_TIMEOUT, DockerContainerRuntime,
    IMMEDIATE_STOP_TIMEOUT, NetworkSpec, PortBinding, ResourceLimits,
};

// ── CLI types (re-exported for testing) ──
pub mod cli;
pub use cli::{Args, Commands, GenFormat};

// ── Metadata ──
pub use metadata::{
    ChallengeMeta, DockerMeta, FlagMeta, GameBoxConfig, GameBoxMeta, HealthcheckConfig,
    JudgeCheckConfig, ResourceConfig,
};

// ── Re-export runtime model types for external use ──
pub use runtime::HealthcheckSpec;
