//! fcmc — FloatCTF container / metadata crate.
//!
//! - `metadata` — Challenge / GameBox package manifests
//! - `runtime` — Docker container lifecycle, image build/push/pull, AWD Specs
//! - `application` — CLI orchestration (check / build / gen)

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

// ── Image runtime ──
pub use runtime::{
    ImageBuildRequest, ImageBuildResult, ImageError, ImageInspect, ImageRuntime, RegistryAuth,
    image_repository, pick_repo_digest, split_image_ref,
};

// ── CLI types (re-exported for testing) ──
pub mod cli;
pub use cli::{Args, Commands, GenFormat};

// ── Metadata ──
pub use metadata::{
    ArtifactKind, ChallengeDockerConfig, ChallengeFlagConfig, ChallengeManifest, ChallengeMeta,
    ChallengeMetaError, GameBoxConfig, GameBoxHealthcheck, GameBoxManifest, GameBoxMeta,
    GameBoxMetaError, GameBoxSection, JudgeManifest, NormalizedChallengeSpec,
    NormalizedGameBoxSpec, NormalizedHealthcheck, RecommendedResources, build_artifact_image_ref,
    build_gamebox_image_ref, derive_safe_name, validate_judge_path, validate_safe_name,
    validate_version,
};

// ── Re-export runtime model types for external use ──
pub use runtime::HealthcheckSpec;
