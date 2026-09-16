//! `fcmc` — FloatCTF 容器/元数据客户端库与 CLI 核心。
//!
//! 提供 Challenge/GameBox 元数据解析、镜像运行时、构建与检查等能力。
//!
//! **公共 metadata contract 的 source of truth**：
//! <https://github.com/FloatCTF/floatctf-content>（`scripts/content.py`）。
//! 本 crate 只复刻该 contract，不定义第二套。

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
    ContainerFilter, ContainerRuntime, ContainerSpec, DEFAULT_HELPER_DOCKER_SOCKET,
    DEFAULT_STOP_TIMEOUT, DockerConnectionKind, DockerContainerRuntime, ExecOptions, ExecOutcome,
    IMMEDIATE_STOP_TIMEOUT, MAX_COPY_BYTES, NetworkInspect, NetworkSpec, PortBinding,
    ResourceLimits, connect_preferred,
};

// ── Image runtime ──
pub use runtime::{
    ImageBuildRequest, ImageBuildResult, ImageError, ImageInspect, ImageRuntime, RegistryAuth,
    image_repository, pick_repo_digest, split_image_ref,
};

// ── CLI types (re-exported for testing) ──
pub mod cli;
pub use cli::{Args, Commands, GenFormat};

// ── Metadata (FloatCTF Content Contract) ──
pub use metadata::{
    ArtifactKind, AwdpManifest, CONTENT_IMAGE_NAMESPACE, ChallengeFlagConfig, ChallengeManifest,
    ChallengeMeta, ChallengeMetaError, ContentFieldError, Difficulty, DockerConfig, GameBoxConfig,
    GameBoxHealthcheck, GameBoxManifest, GameBoxMeta, GameBoxMetaError, GameBoxSection,
    JudgeManifest, NormalizedChallengeSpec, NormalizedGameBoxSpec, NormalizedHealthcheck,
    RecommendedResources, RecommendedResourcesInput, SafeNameError, canonical_content_image_ref,
    content_image_ref, derive_safe_name, is_valid_safe_name, is_valid_version, resolve_safe_name,
    validate_awdp_path, validate_content_fields, validate_judge_path, validate_safe_name,
    validate_version,
};

// ── Re-export runtime model types for external use ──
pub use runtime::HealthcheckSpec;
