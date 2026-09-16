//! 包元数据：Challenge / GameBox / 模板 / 身份规范。
//!
//! **FloatCTF Content Contract 的 source of truth 是
//! <https://github.com/FloatCTF/floatctf-content>（`scripts/content.py`）**。
//! 本 crate 不定义第二套公共 metadata contract：`name` / `version` / `author` /
//! `category` / `difficulty` / `tags` / `description` / `safe_name` / `[flag]` /
//! `[docker]` 的规则全部对齐该仓库；FCMC 只额外拥有 `[gamebox]` / `[judge]` /
//! `[awdp]` 这些 AWD 运行时扩展。

mod challenge;
pub mod content;
mod gamebox;
pub mod identity;
pub mod template;

pub use content::{
    ContentFieldError, Difficulty, DockerConfig, RecommendedResources, RecommendedResourcesInput,
    validate_content_fields,
};
pub use identity::{
    ArtifactKind, CONTENT_IMAGE_NAMESPACE, SafeNameError, canonical_content_image_ref,
    content_image_ref, derive_safe_name, is_valid_safe_name, is_valid_version, resolve_safe_name,
    validate_safe_name, validate_version,
};

pub use challenge::{
    ChallengeFlagConfig, ChallengeManifest, ChallengeMeta, ChallengeMetaError,
    NormalizedChallengeSpec,
};
pub use gamebox::{
    AwdpManifest, GameBoxConfig, GameBoxHealthcheck, GameBoxManifest, GameBoxMeta,
    GameBoxMetaError, GameBoxSection, JudgeManifest, NormalizedGameBoxSpec, NormalizedHealthcheck,
    validate_awdp_path, validate_judge_path, validate_source_code_dir,
};
