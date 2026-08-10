//! GameBox portable package manifest (`meta.toml`).
//!
//! Contract (strict `deny_unknown_fields`):
//!
//! ```toml
//! name = "TTT1"
//! version = "1.0.0"
//! author = "your_email"
//! category = "web"
//! description = "hello floatctf"
//! # optional safe_name = "ttt1"
//!
//! [gamebox]
//! username = "floatctf"
//!
//! [[gamebox.healthchecks]]
//! type = "http"
//! port = 80
//! path = "/"
//! expected_status = 200
//!
//! [[gamebox.healthchecks]]
//! type = "tcp"
//! port = 3306
//!
//! [judge]
//! script = "judge/check.py"
//!
//! [gamebox.recommended_resources]
//! cpu_millis = 1000
//! memory_bytes = 536870912
//! pids_limit = 100
//! ```
//!
//! Removed from the portable manifest (platform / event concerns):
//! `image_tag`, `image_ref`, scoring fields, Docker-style healthcheck, `services`,
//! `schema_version`, and the old `[gamebox.resources]` key.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameBoxMetaError {
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

    #[error("username must be non-empty")]
    EmptyUsername,

    #[error("invalid version '{version}': {reason}")]
    InvalidVersion { version: String, reason: String },

    #[error("SemVer build metadata is not allowed in package version: '{0}'")]
    VersionBuildMetadata(String),

    #[error("invalid safe_name '{0}': must match ^[a-z0-9][a-z0-9_-]*$")]
    InvalidSafeName(String),

    #[error("safe_name is required (could not derive a valid slug from name)")]
    SafeNameRequired,

    #[error("invalid healthcheck port {0}: must be 1..=65535")]
    InvalidHealthcheckPort(u16),

    #[error("HTTP healthcheck path must start with '/': '{0}'")]
    InvalidHealthcheckPath(String),

    #[error("invalid HTTP expected_status {0}: must be 100..=599")]
    InvalidExpectedStatus(u16),

    #[error("duplicate healthcheck entry")]
    DuplicateHealthcheck,

    #[error("invalid judge script path '{0}': {1}")]
    InvalidJudgePath(String, String),

    #[error("recommended_resources.{0} must be > 0")]
    InvalidResource(String),
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Top-level GameBox package manifest (`meta.toml`).
///
/// Also exported as [`GameBoxManifest`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GameBoxMeta {
    pub name: String,
    /// Author package version (SemVer, no build metadata). Becomes the image tag suffix.
    pub version: String,
    pub author: String,
    pub category: String,
    pub description: String,
    /// Optional stable slug. When omitted, derived via [`derive_safe_name`].
    #[serde(default)]
    pub safe_name: Option<String>,
    pub gamebox: GameBoxSection,
    /// Optional trusted judge script reference (never part of Docker build context).
    #[serde(default)]
    pub judge: Option<JudgeManifest>,
}

/// Alias preferred by some callers / plan docs.
pub type GameBoxManifest = GameBoxMeta;

/// `[gamebox]` section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GameBoxSection {
    /// Unprivileged user inside the container.
    pub username: String,
    /// Readiness probes (HTTP / TCP). Not Docker CMD healthchecks.
    #[serde(default)]
    pub healthchecks: Vec<GameBoxHealthcheck>,
    /// Soft resource recommendation (EventGameBox may override).
    #[serde(default)]
    pub recommended_resources: Option<RecommendedResources>,
}

/// Back-compat alias — prefer [`GameBoxSection`].
pub type GameBoxConfig = GameBoxSection;

/// Tagged healthcheck union (`type = "http" | "tcp"`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum GameBoxHealthcheck {
    Http {
        port: u16,
        path: String,
        /// Defaults to 200; materialised in [`GameBoxMeta::normalize`].
        #[serde(default = "default_expected_status")]
        expected_status: u16,
    },
    Tcp {
        port: u16,
    },
}

fn default_expected_status() -> u16 {
    200
}

/// `[judge]` section — path to a trusted check script under the package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JudgeManifest {
    /// Relative path that must start with `judge/` (e.g. `judge/check.py`).
    pub script: String,
}

/// Shared soft resource recommendation (see [`crate::metadata::RecommendedResources`]).
pub use super::RecommendedResources;

// ---------------------------------------------------------------------------
// Canonical normalized spec (stable JSON for spec_digest)
// ---------------------------------------------------------------------------

/// Canonical, fully-materialised view used for `spec_json` / digest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedGameBoxSpec {
    pub name: String,
    pub version: String,
    pub author: String,
    pub category: String,
    pub description: String,
    pub safe_name: String,
    pub username: String,
    pub healthchecks: Vec<NormalizedHealthcheck>,
    pub recommended_resources: RecommendedResources,
    pub judge_script: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NormalizedHealthcheck {
    Http {
        port: u16,
        path: String,
        expected_status: u16,
    },
    Tcp {
        port: u16,
    },
}

// ---------------------------------------------------------------------------
// Shared identity helpers (logic lives in crate::metadata::identity)
// ---------------------------------------------------------------------------

pub use crate::metadata::identity::{ArtifactKind, build_artifact_image_ref, derive_safe_name};

/// Validate an explicit `safe_name` (identity rules) mapped to
/// [`GameBoxMetaError`] so the public error type stays stable.
pub fn validate_safe_name(s: &str) -> Result<(), GameBoxMetaError> {
    crate::metadata::identity::validate_safe_name(s)
        .map_err(|_| GameBoxMetaError::InvalidSafeName(s.to_string()))
}

/// Parse a package version as SemVer **without** build metadata (`+…` rejected),
/// mapped to [`GameBoxMetaError`]. Prerelease (`1.0.0-rc.1`) is allowed.
pub fn validate_version(version: &str) -> Result<semver::Version, GameBoxMetaError> {
    if version.contains('+') {
        return Err(GameBoxMetaError::VersionBuildMetadata(version.to_string()));
    }
    crate::metadata::identity::validate_version(version).map_err(|reason| {
        GameBoxMetaError::InvalidVersion {
            version: version.to_string(),
            reason,
        }
    })
}

// ---------------------------------------------------------------------------
// Judge path helper
// ---------------------------------------------------------------------------

/// Validate a judge script path: relative, starts with `judge/`, no `..`, not absolute.
pub fn validate_judge_path(path: &str) -> Result<(), GameBoxMetaError> {
    if path.is_empty() {
        return Err(GameBoxMetaError::InvalidJudgePath(
            path.to_string(),
            "empty path".into(),
        ));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(GameBoxMetaError::InvalidJudgePath(
            path.to_string(),
            "must be relative".into(),
        ));
    }
    // Windows drive / UNC
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return Err(GameBoxMetaError::InvalidJudgePath(
            path.to_string(),
            "must be relative".into(),
        ));
    }
    if !path.starts_with("judge/") {
        return Err(GameBoxMetaError::InvalidJudgePath(
            path.to_string(),
            "must start with 'judge/'".into(),
        ));
    }
    if path.contains("..") {
        return Err(GameBoxMetaError::InvalidJudgePath(
            path.to_string(),
            "must not contain '..'".into(),
        ));
    }
    if path.ends_with('/') || path == "judge/" {
        return Err(GameBoxMetaError::InvalidJudgePath(
            path.to_string(),
            "must point to a file".into(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Image ref helper (platform prefix + identity)
// ---------------------------------------------------------------------------

/// Build the canonical GameBox image reference (delegates to the shared
/// [`build_artifact_image_ref`] implementation).
///
/// ```text
/// {registry_prefix}/gameboxes/{safe_name}:{version}
/// ```
///
/// `registry_prefix` comes from **platform config**, never from `meta.toml`.
/// CLI default when none is supplied: `"floatctf"`.
pub fn build_gamebox_image_ref(registry_prefix: &str, safe_name: &str, version: &str) -> String {
    build_artifact_image_ref(ArtifactKind::GameBox, registry_prefix, safe_name, version)
}

// ---------------------------------------------------------------------------
// GameBoxMeta impl
// ---------------------------------------------------------------------------

impl GameBoxMeta {
    /// Parse TOML only (no semantic validation). Unknown fields fail via
    /// `deny_unknown_fields`.
    pub fn from_toml_str(toml_str: &str) -> Result<Self, GameBoxMetaError> {
        toml::from_str(toml_str).map_err(|e| {
            let msg = e.to_string();
            // Surface unknown-field errors with a clearer variant when possible.
            if msg.contains("unknown field") {
                GameBoxMetaError::UnknownField(msg)
            } else {
                GameBoxMetaError::Parse(msg)
            }
        })
    }

    /// Parse + semantic validation.
    pub fn parse_and_validate(toml_str: &str) -> Result<Self, GameBoxMetaError> {
        let meta = Self::from_toml_str(toml_str)?;
        meta.validate()?;
        Ok(meta)
    }

    /// Resolve `safe_name`: explicit value, or derived from `name`.
    pub fn resolved_safe_name(&self) -> Result<String, GameBoxMetaError> {
        if let Some(ref s) = self.safe_name {
            validate_safe_name(s)?;
            return Ok(s.clone());
        }
        derive_safe_name(&self.name).ok_or(GameBoxMetaError::SafeNameRequired)
    }

    /// Semantic validation (ports, paths, version, safe_name, judge, resources, dupes).
    pub fn validate(&self) -> Result<(), GameBoxMetaError> {
        if self.name.trim().is_empty() {
            return Err(GameBoxMetaError::EmptyName);
        }
        if self.author.trim().is_empty() {
            return Err(GameBoxMetaError::EmptyAuthor);
        }
        if self.category.trim().is_empty() {
            return Err(GameBoxMetaError::EmptyCategory);
        }
        if self.gamebox.username.trim().is_empty() {
            return Err(GameBoxMetaError::EmptyUsername);
        }

        validate_version(&self.version)?;
        let _ = self.resolved_safe_name()?;

        // Healthchecks
        let mut seen = std::collections::HashSet::new();
        for hc in &self.gamebox.healthchecks {
            match hc {
                GameBoxHealthcheck::Http {
                    port,
                    path,
                    expected_status,
                } => {
                    if *port == 0 {
                        return Err(GameBoxMetaError::InvalidHealthcheckPort(*port));
                    }
                    if !path.starts_with('/') {
                        return Err(GameBoxMetaError::InvalidHealthcheckPath(path.clone()));
                    }
                    if !(100..=599).contains(expected_status) {
                        return Err(GameBoxMetaError::InvalidExpectedStatus(*expected_status));
                    }
                    let key = NormalizedHealthcheck::Http {
                        port: *port,
                        path: path.clone(),
                        expected_status: *expected_status,
                    };
                    if !seen.insert(format!("{key:?}")) {
                        return Err(GameBoxMetaError::DuplicateHealthcheck);
                    }
                }
                GameBoxHealthcheck::Tcp { port } => {
                    if *port == 0 {
                        return Err(GameBoxMetaError::InvalidHealthcheckPort(*port));
                    }
                    let key = format!("Tcp({port})");
                    if !seen.insert(key) {
                        return Err(GameBoxMetaError::DuplicateHealthcheck);
                    }
                }
            }
        }

        if let Some(ref judge) = self.judge {
            validate_judge_path(&judge.script)?;
        }

        if let Some(ref res) = self.gamebox.recommended_resources {
            if res.cpu_millis <= 0 {
                return Err(GameBoxMetaError::InvalidResource("cpu_millis".into()));
            }
            if res.memory_bytes <= 0 {
                return Err(GameBoxMetaError::InvalidResource("memory_bytes".into()));
            }
            if res.pids_limit <= 0 {
                return Err(GameBoxMetaError::InvalidResource("pids_limit".into()));
            }
        }

        Ok(())
    }

    /// Produce a canonical, fully-materialised spec (sorted healthchecks, defaults filled).
    ///
    /// Callers should `validate()` first; this method also validates.
    pub fn normalize(&self) -> Result<NormalizedGameBoxSpec, GameBoxMetaError> {
        self.validate()?;
        let safe_name = self.resolved_safe_name()?;

        let mut healthchecks: Vec<NormalizedHealthcheck> = self
            .gamebox
            .healthchecks
            .iter()
            .map(|hc| match hc {
                GameBoxHealthcheck::Http {
                    port,
                    path,
                    expected_status,
                } => NormalizedHealthcheck::Http {
                    port: *port,
                    path: path.clone(),
                    expected_status: *expected_status,
                },
                GameBoxHealthcheck::Tcp { port } => NormalizedHealthcheck::Tcp { port: *port },
            })
            .collect();

        // Canonical order: type (Http < Tcp via Ord on tag… we implement via sort_by_key)
        healthchecks.sort_by(|a, b| {
            use std::cmp::Ordering;
            match (a, b) {
                (
                    NormalizedHealthcheck::Http {
                        port: pa,
                        path: a_path,
                        ..
                    },
                    NormalizedHealthcheck::Http {
                        port: pb,
                        path: b_path,
                        ..
                    },
                ) => pa.cmp(pb).then_with(|| a_path.cmp(b_path)),
                (
                    NormalizedHealthcheck::Tcp { port: pa },
                    NormalizedHealthcheck::Tcp { port: pb },
                ) => pa.cmp(pb),
                (NormalizedHealthcheck::Http { .. }, NormalizedHealthcheck::Tcp { .. }) => {
                    Ordering::Less
                }
                (NormalizedHealthcheck::Tcp { .. }, NormalizedHealthcheck::Http { .. }) => {
                    Ordering::Greater
                }
            }
        });

        let recommended_resources = self
            .gamebox
            .recommended_resources
            .clone()
            .unwrap_or_default();

        Ok(NormalizedGameBoxSpec {
            name: self.name.clone(),
            version: self.version.clone(),
            author: self.author.clone(),
            category: self.category.clone(),
            description: self.description.clone(),
            safe_name,
            username: self.gamebox.username.clone(),
            healthchecks,
            recommended_resources,
            judge_script: self.judge.as_ref().map(|j| j.script.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
name = "TTT1"
version = "1.0.0"
author = "you@example.com"
category = "web"
description = "hello"

[gamebox]
username = "floatctf"
"#;

    #[test]
    fn parse_minimal() {
        let meta = GameBoxMeta::parse_and_validate(MINIMAL).unwrap();
        assert_eq!(meta.name, "TTT1");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.resolved_safe_name().unwrap(), "ttt1");
        assert!(meta.gamebox.healthchecks.is_empty());
        assert!(meta.judge.is_none());
    }

    #[test]
    fn reject_image_tag() {
        let toml = format!("{MINIMAL}\nimage_tag = \"x\"\n");
        // image_tag at top level
        let err = GameBoxMeta::from_toml_str(&toml).unwrap_err();
        assert!(matches!(
            err,
            GameBoxMetaError::UnknownField(_) | GameBoxMetaError::Parse(_)
        ));
    }

    #[test]
    fn reject_legacy_scoring_in_gamebox() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[gamebox]
username = "u"
break_points = 100
"#;
        assert!(GameBoxMeta::from_toml_str(toml).is_err());
    }

    #[test]
    fn reject_old_resources_key() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[gamebox]
username = "u"

[gamebox.resources]
cpu_millis = 1
"#;
        assert!(GameBoxMeta::from_toml_str(toml).is_err());
    }

    #[test]
    fn reject_build_metadata_version() {
        let toml = r#"
name = "t"
version = "1.0.0+build.1"
author = "a"
category = "web"
description = "d"

[gamebox]
username = "u"
"#;
        let err = GameBoxMeta::parse_and_validate(toml).unwrap_err();
        assert!(matches!(err, GameBoxMetaError::VersionBuildMetadata(_)));
    }

    #[test]
    fn allow_prerelease_version() {
        let toml = r#"
name = "t"
version = "1.0.0-rc.1"
author = "a"
category = "web"
description = "d"

[gamebox]
username = "u"
"#;
        GameBoxMeta::parse_and_validate(toml).unwrap();
    }

    #[test]
    fn healthcheck_http_default_status_materialized() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[gamebox]
username = "u"

[[gamebox.healthchecks]]
type = "http"
port = 80
path = "/"
"#;
        let meta = GameBoxMeta::parse_and_validate(toml).unwrap();
        let norm = meta.normalize().unwrap();
        match &norm.healthchecks[0] {
            NormalizedHealthcheck::Http {
                expected_status, ..
            } => assert_eq!(*expected_status, 200),
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn tcp_rejects_path_field() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[gamebox]
username = "u"

[[gamebox.healthchecks]]
type = "tcp"
port = 3306
path = "/"
"#;
        assert!(GameBoxMeta::from_toml_str(toml).is_err());
    }

    #[test]
    fn normalize_sorts_healthchecks() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[gamebox]
username = "u"

[[gamebox.healthchecks]]
type = "tcp"
port = 3306

[[gamebox.healthchecks]]
type = "http"
port = 80
path = "/"
expected_status = 200

[[gamebox.healthchecks]]
type = "http"
port = 8080
path = "/health"
expected_status = 200
"#;
        let a = GameBoxMeta::parse_and_validate(toml)
            .unwrap()
            .normalize()
            .unwrap();
        // reverse order input should normalize identically
        let toml_rev = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
description = "d"

[gamebox]
username = "u"

[[gamebox.healthchecks]]
type = "http"
port = 8080
path = "/health"
expected_status = 200

[[gamebox.healthchecks]]
type = "http"
port = 80
path = "/"
expected_status = 200

[[gamebox.healthchecks]]
type = "tcp"
port = 3306
"#;
        let b = GameBoxMeta::parse_and_validate(toml_rev)
            .unwrap()
            .normalize()
            .unwrap();
        let ja = serde_json::to_string(&a).unwrap();
        let jb = serde_json::to_string(&b).unwrap();
        assert_eq!(ja, jb);
        assert!(matches!(
            a.healthchecks[0],
            NormalizedHealthcheck::Http { port: 80, .. }
        ));
        assert!(matches!(
            a.healthchecks[2],
            NormalizedHealthcheck::Tcp { port: 3306 }
        ));
    }

    #[test]
    fn derive_safe_name_cases() {
        assert_eq!(
            derive_safe_name("Easy Web 01").as_deref(),
            Some("easy-web-01")
        );
        assert_eq!(derive_safe_name("easy---web").as_deref(), Some("easy-web"));
        assert_eq!(derive_safe_name("  Hello  ").as_deref(), Some("hello"));
        // Mixed ASCII + CJK keeps the ASCII slug; pure non-ASCII → None.
        assert_eq!(derive_safe_name("SQL注入").as_deref(), Some("sql"));
        assert_eq!(derive_safe_name("注入题目"), None);
        assert_eq!(derive_safe_name("!!!"), None);
    }

    #[test]
    fn image_ref_helper() {
        assert_eq!(
            build_gamebox_image_ref("floatctf", "ttt1", "1.0.0"),
            "floatctf/gameboxes/ttt1:1.0.0"
        );
        assert_eq!(
            build_gamebox_image_ref("registry.example.com", "easy-web", "2.1.0"),
            "registry.example.com/gameboxes/easy-web:2.1.0"
        );
    }

    #[test]
    fn judge_path_rules() {
        assert!(validate_judge_path("judge/check.py").is_ok());
        assert!(validate_judge_path("/judge/check.py").is_err());
        assert!(validate_judge_path("judge/../x.py").is_err());
        assert!(validate_judge_path("scripts/check.py").is_err());
        assert!(validate_judge_path("judge/").is_err());
    }
}
