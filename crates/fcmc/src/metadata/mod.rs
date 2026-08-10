//! Challenge / GameBox metadata parsing (TOML, config shapes).

mod challenge;
mod gamebox;
pub mod template;

pub use challenge::{ChallengeMeta, DockerMeta, FlagMeta};
pub use gamebox::{
    GameBoxConfig, GameBoxHealthcheck, GameBoxManifest, GameBoxMeta, GameBoxMetaError,
    GameBoxSection, JudgeManifest, NormalizedGameBoxSpec, NormalizedHealthcheck,
    RecommendedResources, build_gamebox_image_ref, derive_safe_name, validate_judge_path,
    validate_safe_name, validate_version,
};
