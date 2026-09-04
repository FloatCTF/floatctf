//! 包元数据：Challenge / GameBox / 模板 / 身份规范。

mod challenge;
mod gamebox;
pub mod identity;
pub mod template;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

/// 给运营/赛事预填用的软资源建议。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecommendedResources {
    pub cpu_millis: i64,
    pub memory_bytes: i64,
    pub pids_limit: i64,
}

impl Default for RecommendedResources {
    fn default() -> Self {
        Self {
            cpu_millis: 1000,
            memory_bytes: 536_870_912,
            pids_limit: 100,
        }
    }
}

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use challenge::{
    ChallengeDockerConfig, ChallengeFlagConfig, ChallengeManifest, ChallengeMeta,
    ChallengeMetaError, NormalizedChallengeSpec,
};
pub use gamebox::{
    AwdpManifest, GameBoxConfig, GameBoxHealthcheck, GameBoxManifest, GameBoxMeta,
    GameBoxMetaError, GameBoxSection, JudgeManifest, NormalizedGameBoxSpec, NormalizedHealthcheck,
    build_gamebox_image_ref, validate_awdp_path, validate_judge_path, validate_safe_name,
    validate_source_code_dir, validate_version,
};
pub use identity::{ArtifactKind, build_artifact_image_ref, derive_safe_name};
