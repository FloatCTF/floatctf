//! Challenge metadata — pure data structures and TOML parsing.

use serde::Deserialize;

/// Top-level challenge configuration parsed from `meta.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ChallengeMeta {
    pub name: String,
    pub author: String,
    pub category: String,
    pub description: String,

    pub flag: FlagMeta,
    pub docker: Option<DockerMeta>,

    pub attachment: Option<String>,
}

/// Flag configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct FlagMeta {
    pub value: String,
    pub env_var: String,
}

/// Docker configuration for a challenge.
#[derive(Debug, Clone, Deserialize)]
pub struct DockerMeta {
    pub image_tag: String,
    pub port: String, // e.g. "80/tcp"
    pub is_nc: Option<bool>,
}

impl ChallengeMeta {
    /// Parse a ChallengeMeta from a TOML string.
    pub fn from_toml_str(toml: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml)
    }
}
