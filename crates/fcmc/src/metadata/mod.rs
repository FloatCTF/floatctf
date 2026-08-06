//! Challenge / GameBox metadata parsing (YAML, config shapes).

mod challenge;
mod gamebox;
pub mod template;

pub use challenge::{ChallengeMeta, DockerMeta, FlagMeta};
pub use gamebox::{
    GameBoxConfig, GameBoxMeta, HealthcheckConfig, JudgeCheckConfig, ResourceConfig,
};
