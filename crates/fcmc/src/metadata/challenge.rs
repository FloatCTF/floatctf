//! Challenge package manifest (`meta.toml`) — v1.
//!
//! Contract (strict `deny_unknown_fields`):
//!
//! ```toml
//! name = "Easy Web 01"
//! version = "1.0.0"
//! author = "your_email@example.com"
//! category = "web"
//! description = "Challenge description"
//! # optional: safe_name = "easy-web-01"
//! # optional: attachment = "attachment/src.zip"
//!
//! [flag]
//! type = "dynamic"          # "dynamic" | "static"
//!
//! [docker]
//! port = 80
//!
//! [docker.recommended_resources]
//! cpu_millis = 500
//! memory_bytes = 268435456
//! pids_limit = 100
//! ```
//!
//! Removed from the portable manifest (legacy / platform concerns):
//! `image_tag`, `env_var`, `is_nc`, `schema_version`, string ports (`"80/tcp"`),
//! and `[flag] value = ""` (empty-string dynamic markers).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::metadata::{RecommendedResources, identity};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ChallengeMetaError {
    #[error("manifest parse error: {0}")]
    Parse(String),

    #[error("unknown or legacy field in manifest: {0}")]
    UnknownField(String),

    #[error("name must be non-empty")]
    EmptyName,

    #[error("author must be non-empty")]
    EmptyAuthor,

    #[error("category must be non-empty")]
    EmptyCategory,

    #[error("invalid version '{version}': {reason}")]
    InvalidVersion { version: String, reason: String },

    #[error("SemVer build metadata is not allowed in package version: '{0}'")]
    VersionBuildMetadata(String),

    #[error("invalid safe_name '{0}': must match ^[a-z0-9][a-z0-9_-]*$")]
    InvalidSafeName(String),

    #[error("safe_name is required (could not derive a valid slug from name)")]
    SafeNameRequired,

    #[error("invalid flag config: {0}")]
    InvalidFlagConfig(String),

    #[error("static flag requires [flag] value")]
    StaticFlagRequired,

    #[error("invalid container port {0}: must be 1..=65535")]
    InvalidPort(u16),

    #[error("recommended_resources.{0} must be > 0")]
    InvalidResource(String),

    #[error("invalid attachment path '{0}': {1}")]
    InvalidAttachmentPath(String, String),
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Top-level Challenge package manifest (`meta.toml`).
///
/// Also exported as [`ChallengeManifest`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengeMeta {
    pub name: String,
    /// Author package version (SemVer, no build metadata). Becomes the image tag suffix.
    pub version: String,
    pub author: String,
    pub category: String,
    pub description: String,
    /// Optional stable slug; derived from name when absent.
    #[serde(default)]
    pub safe_name: Option<String>,
    /// Optional attachment path, must be under `attachment/`.
    #[serde(default)]
    pub attachment: Option<String>,
    pub flag: ChallengeFlagConfig,
    #[serde(default)]
    pub docker: Option<ChallengeDockerConfig>,
}

/// Alias preferred by some callers / plan docs.
pub type ChallengeManifest = ChallengeMeta;

/// `[flag]` section — flag type contract.
///
/// serde's internally-tagged enums silently ignore `deny_unknown_fields`, so
/// deserialization goes through a strict intermediate struct (see the manual
/// [`Deserialize`] impl) that rejects legacy/unknown keys such as `env_var`
/// and `value` on a dynamic flag.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum ChallengeFlagConfig {
    /// Platform generates a per-instance flag, injected as FLAG env, written to /flag at entrypoint.
    Dynamic,
    /// Fixed flag; `value` required. Stored separately (secret) — never in logs/DTOs.
    Static { value: Option<String> },
}

impl<'de> Deserialize<'de> for ChallengeFlagConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct FlagRepr {
            r#type: String,
            #[serde(default)]
            value: Option<String>,
        }

        let repr = FlagRepr::deserialize(deserializer)?;
        match repr.r#type.as_str() {
            "dynamic" => {
                if repr.value.is_some() {
                    return Err(D::Error::custom(
                        "unknown field `value` for flag type dynamic (only [flag] type = \"static\" accepts value)",
                    ));
                }
                Ok(ChallengeFlagConfig::Dynamic)
            }
            "static" => Ok(ChallengeFlagConfig::Static { value: repr.value }),
            other => Err(D::Error::custom(format!("unknown flag type: `{other}`"))),
        }
    }
}

/// `[docker]` section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengeDockerConfig {
    /// Single exposed TCP port (runtime contract source of truth; EXPOSE in Dockerfile is cosmetic).
    pub port: u16,
    /// Soft resource recommendation (EventChallenge may override).
    #[serde(default)]
    pub recommended_resources: Option<RecommendedResources>,
}

// ---------------------------------------------------------------------------
// Canonical normalized spec (stable JSON for spec_digest)
// ---------------------------------------------------------------------------

/// Canonical, fully-materialised view used for `spec_json` / digest.
///
/// MUST NOT contain the static flag value (secret).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedChallengeSpec {
    pub name: String,
    pub version: String,
    pub author: String,
    pub category: String,
    pub description: String,
    pub safe_name: String,
    /// `"dynamic"` | `"static"`
    pub flag_type: String,
    /// None for non-docker challenges.
    pub container_port: Option<u16>,
    pub recommended_resources: RecommendedResources,
    pub attachment: Option<String>,
}

// ---------------------------------------------------------------------------
// Attachment path helper
// ---------------------------------------------------------------------------

/// Validate an attachment path: non-empty, relative, under `attachment/`,
/// no `..`, not a directory.
fn validate_attachment_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("empty path".into());
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err("must be relative".into());
    }
    // Windows drive / UNC
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return Err("must be relative".into());
    }
    if path.contains("..") {
        return Err("must not contain '..'".into());
    }
    if !path.starts_with("attachment/") {
        return Err("must start with 'attachment/'".into());
    }
    if path.ends_with('/') {
        return Err("must point to a file".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ChallengeMeta impl
// ---------------------------------------------------------------------------

impl ChallengeMeta {
    /// Parse TOML only (no semantic validation). Unknown fields fail via
    /// `deny_unknown_fields`; a static flag without `value` maps to
    /// [`ChallengeMetaError::StaticFlagRequired`].
    pub fn from_toml_str(toml_str: &str) -> Result<Self, ChallengeMetaError> {
        toml::from_str(toml_str).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("unknown field") {
                ChallengeMetaError::UnknownField(msg)
            } else if msg.contains("missing field `value`") {
                ChallengeMetaError::StaticFlagRequired
            } else {
                ChallengeMetaError::Parse(msg)
            }
        })
    }

    /// Parse + semantic validation.
    pub fn parse_and_validate(toml_str: &str) -> Result<Self, ChallengeMetaError> {
        let meta = Self::from_toml_str(toml_str)?;
        meta.validate()?;
        Ok(meta)
    }

    /// Resolve `safe_name`: explicit value, or derived from `name`.
    pub fn resolved_safe_name(&self) -> Result<String, ChallengeMetaError> {
        if let Some(ref s) = self.safe_name {
            identity::validate_safe_name(s)
                .map_err(|_| ChallengeMetaError::InvalidSafeName(s.clone()))?;
            return Ok(s.clone());
        }
        identity::derive_safe_name(&self.name).ok_or(ChallengeMetaError::SafeNameRequired)
    }

    /// Semantic validation (identity fields, flag config, docker port/resources, attachment).
    pub fn validate(&self) -> Result<(), ChallengeMetaError> {
        if self.name.trim().is_empty() {
            return Err(ChallengeMetaError::EmptyName);
        }
        if self.author.trim().is_empty() {
            return Err(ChallengeMetaError::EmptyAuthor);
        }
        if self.category.trim().is_empty() {
            return Err(ChallengeMetaError::EmptyCategory);
        }

        if self.version.contains('+') {
            return Err(ChallengeMetaError::VersionBuildMetadata(
                self.version.clone(),
            ));
        }
        identity::validate_version(&self.version).map_err(|reason| {
            ChallengeMetaError::InvalidVersion {
                version: self.version.clone(),
                reason,
            }
        })?;

        let _ = self.resolved_safe_name()?;

        match &self.flag {
            ChallengeFlagConfig::Dynamic => {}
            ChallengeFlagConfig::Static { value } => {
                if value.as_deref().map(str::trim).map_or(true, str::is_empty) {
                    return Err(ChallengeMetaError::StaticFlagRequired);
                }
            }
        }

        if let Some(ref docker) = self.docker {
            if docker.port == 0 {
                return Err(ChallengeMetaError::InvalidPort(docker.port));
            }
            if let Some(ref res) = docker.recommended_resources {
                if res.cpu_millis <= 0 {
                    return Err(ChallengeMetaError::InvalidResource("cpu_millis".into()));
                }
                if res.memory_bytes <= 0 {
                    return Err(ChallengeMetaError::InvalidResource("memory_bytes".into()));
                }
                if res.pids_limit <= 0 {
                    return Err(ChallengeMetaError::InvalidResource("pids_limit".into()));
                }
            }
        }

        if let Some(ref attachment) = self.attachment {
            validate_attachment_path(attachment).map_err(|reason| {
                ChallengeMetaError::InvalidAttachmentPath(attachment.clone(), reason)
            })?;
        }

        Ok(())
    }

    /// Static flag value (secret). `None` for dynamic flags / missing value.
    /// The platform stores it in a secret column — never in logs/DTOs.
    pub fn static_flag_value(&self) -> Option<&str> {
        match &self.flag {
            ChallengeFlagConfig::Static { value: Some(v) } => Some(v.as_str()),
            _ => None,
        }
    }

    /// Produce a canonical, fully-materialised spec (defaults filled).
    ///
    /// Callers should `validate()` first; this method also validates.
    pub fn normalize(&self) -> Result<NormalizedChallengeSpec, ChallengeMetaError> {
        self.validate()?;
        let safe_name = self.resolved_safe_name()?;

        let flag_type = match &self.flag {
            ChallengeFlagConfig::Dynamic => "dynamic",
            ChallengeFlagConfig::Static { .. } => "static",
        };

        let container_port = self.docker.as_ref().map(|d| d.port);

        // Challenge default recommendations (500m CPU / 256MiB / 100 pids) differ
        // from the gamebox default (1000 / 512MiB / 100); fill inline when absent.
        // Non-docker challenges get the same default (they have no resources anyway).
        let recommended_resources = self
            .docker
            .as_ref()
            .and_then(|d| d.recommended_resources.clone())
            .unwrap_or(RecommendedResources {
                cpu_millis: 500,
                memory_bytes: 268_435_456,
                pids_limit: 100,
            });

        Ok(NormalizedChallengeSpec {
            name: self.name.clone(),
            version: self.version.clone(),
            author: self.author.clone(),
            category: self.category.clone(),
            description: self.description.clone(),
            safe_name,
            flag_type: flag_type.to_string(),
            container_port,
            recommended_resources,
            attachment: self.attachment.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
name = "Easy Web 01"
version = "1.0.0"
author = "you@example.com"
category = "web"
description = "hello"

[flag]
type = "dynamic"

[docker]
port = 80
"#;

    #[test]
    fn parse_minimal_dynamic() {
        let meta = ChallengeMeta::parse_and_validate(MINIMAL).unwrap();
        assert_eq!(meta.name, "Easy Web 01");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.resolved_safe_name().unwrap(), "easy-web-01");
        assert!(matches!(meta.flag, ChallengeFlagConfig::Dynamic));
        let docker = meta.docker.as_ref().unwrap();
        assert_eq!(docker.port, 80);
        assert!(docker.recommended_resources.is_none());
    }

    #[test]
    fn parse_static_with_value() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "static"
value = "flag{secret}"
"#;
        let meta = ChallengeMeta::parse_and_validate(toml).unwrap();
        assert_eq!(meta.static_flag_value(), Some("flag{secret}"));
        let norm = meta.normalize().unwrap();
        assert_eq!(norm.flag_type, "static");
    }

    #[test]
    fn static_without_value_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "static"
"#;
        let err = ChallengeMeta::parse_and_validate(toml).unwrap_err();
        assert!(matches!(err, ChallengeMetaError::StaticFlagRequired));
    }

    #[test]
    fn static_empty_value_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "static"
value = ""
"#;
        let err = ChallengeMeta::parse_and_validate(toml).unwrap_err();
        assert!(matches!(err, ChallengeMetaError::StaticFlagRequired));
    }

    #[test]
    fn dynamic_with_value_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
value = "flag{secret}"
"#;
        let err = ChallengeMeta::from_toml_str(toml).unwrap_err();
        assert!(matches!(
            err,
            ChallengeMetaError::UnknownField(_) | ChallengeMetaError::Parse(_)
        ));
    }

    #[test]
    fn legacy_top_level_fields_rejected() {
        for line in [
            "image_tag = \"x:v1\"",
            "env_var = \"FLAG\"",
            "schema_version = 1",
        ] {
            let toml = format!(
                r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
{line}

[flag]
type = "dynamic"
"#
            );
            let err = ChallengeMeta::from_toml_str(&toml).unwrap_err();
            assert!(
                matches!(
                    err,
                    ChallengeMetaError::UnknownField(_) | ChallengeMetaError::Parse(_)
                ),
                "legacy field must be rejected: {line}"
            );
        }
    }

    #[test]
    fn legacy_flag_env_var_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
env_var = "FLAG"
"#;
        let err = ChallengeMeta::from_toml_str(toml).unwrap_err();
        assert!(matches!(
            err,
            ChallengeMetaError::UnknownField(_) | ChallengeMetaError::Parse(_)
        ));
    }

    #[test]
    fn string_port_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"

[docker]
port = "80/tcp"
"#;
        let err = ChallengeMeta::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ChallengeMetaError::Parse(_)));
    }

    #[test]
    fn zero_port_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"

[docker]
port = 0
"#;
        let err = ChallengeMeta::parse_and_validate(toml).unwrap_err();
        assert!(matches!(err, ChallengeMetaError::InvalidPort(0)));
    }

    #[test]
    fn safe_name_derivation() {
        assert_eq!(
            identity::derive_safe_name("Easy Web 01").as_deref(),
            Some("easy-web-01")
        );
        assert_eq!(
            identity::derive_safe_name("easy---web").as_deref(),
            Some("easy-web")
        );
        // non-ASCII-only name without explicit safe_name → SafeNameRequired
        let toml = r#"
name = "注入题目"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
"#;
        let err = ChallengeMeta::parse_and_validate(toml).unwrap_err();
        assert!(matches!(err, ChallengeMetaError::SafeNameRequired));
    }

    #[test]
    fn explicit_safe_name_valid_and_invalid() {
        let valid = r#"
name = "注入题目"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
safe_name = "zhu-ru"

[flag]
type = "dynamic"
"#;
        let meta = ChallengeMeta::parse_and_validate(valid).unwrap();
        assert_eq!(meta.resolved_safe_name().unwrap(), "zhu-ru");

        let invalid = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
safe_name = "Easy Web"

[flag]
type = "dynamic"
"#;
        let err = ChallengeMeta::parse_and_validate(invalid).unwrap_err();
        assert!(matches!(err, ChallengeMetaError::InvalidSafeName(_)));
    }

    #[test]
    fn version_rules() {
        for v in ["1.0.0", "1.0.0-rc.1"] {
            let toml = format!(
                r#"
name = "t"
version = "{v}"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
"#
            );
            ChallengeMeta::parse_and_validate(&toml).unwrap();
        }

        let build_meta = r#"
name = "t"
version = "1.0.0+build.1"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
"#;
        let err = ChallengeMeta::parse_and_validate(build_meta).unwrap_err();
        assert!(matches!(err, ChallengeMetaError::VersionBuildMetadata(_)));

        let bad = r#"
name = "t"
version = "abc"
author = "a"
category = "web"
description = "d"

[flag]
type = "dynamic"
"#;
        let err = ChallengeMeta::parse_and_validate(bad).unwrap_err();
        assert!(matches!(err, ChallengeMetaError::InvalidVersion { .. }));
    }

    #[test]
    fn attachment_rules() {
        let ok = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
attachment = "attachment/src.zip"

[flag]
type = "dynamic"
"#;
        let meta = ChallengeMeta::parse_and_validate(ok).unwrap();
        assert_eq!(meta.attachment.as_deref(), Some("attachment/src.zip"));

        for bad in ["../x", "/x", "src/x"] {
            let toml = format!(
                r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"
attachment = "{bad}"

[flag]
type = "dynamic"
"#
            );
            let err = ChallengeMeta::parse_and_validate(&toml).unwrap_err();
            assert!(
                matches!(err, ChallengeMetaError::InvalidAttachmentPath(_, _)),
                "attachment path must be rejected: {bad}"
            );
        }
    }

    #[test]
    fn normalize_fills_defaults() {
        let meta = ChallengeMeta::parse_and_validate(MINIMAL).unwrap();
        let norm = meta.normalize().unwrap();
        assert_eq!(norm.safe_name, "easy-web-01");
        assert_eq!(norm.flag_type, "dynamic");
        assert_eq!(norm.container_port, Some(80));
        assert_eq!(norm.recommended_resources.cpu_millis, 500);
        assert_eq!(norm.recommended_resources.memory_bytes, 268_435_456);
        assert_eq!(norm.recommended_resources.pids_limit, 100);
        assert!(norm.attachment.is_none());
    }

    #[test]
    fn shared_artifact_image_ref() {
        assert_eq!(
            identity::build_artifact_image_ref(
                identity::ArtifactKind::Challenge,
                "registry.example",
                "easy-web",
                "1.0.0"
            ),
            "registry.example/challenges/easy-web:1.0.0"
        );
        assert_eq!(
            identity::build_artifact_image_ref(
                identity::ArtifactKind::GameBox,
                "registry.example",
                "easy-web",
                "1.0.0"
            ),
            "registry.example/gameboxes/easy-web:1.0.0"
        );
    }
}
