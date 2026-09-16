//! GameBox 包元数据模型与解析。
//!
//! 公共字段与 [`crate::metadata::ChallengeMeta`] **完全一致**（见
//! `floatctf-content/scripts/content.py`）。FCMC 额外拥有的 AWD 运行时扩展是
//! `[gamebox]` / `[judge]` / `[awdp]`：这些 **不是** 官方必需字段，
//! 官方 canonical GameBox fixture 可以完全没有 `[gamebox]`。

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::metadata::content::{
    ContentFieldError, Difficulty, DockerConfig, RecommendedResources, validate_content_fields,
};
use crate::metadata::identity::{self, SafeNameError};

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

    #[error("description must be non-empty")]
    EmptyDescription,

    #[error("{0}")]
    InvalidVersion(String),

    #[error("invalid safe_name '{0}': must match ^[a-z0-9]+(?:[._-][a-z0-9]+)*$")]
    InvalidSafeName(String),

    #[error("unable to derive Docker safe_name from content id; set safe_name explicitly")]
    SafeNameRequired,

    #[error("invalid tag in `tags`: every entry must be a non-empty string: '{0}'")]
    InvalidTag(String),

    #[error("GameBox runtime metadata [gamebox] is required for this operation")]
    GameBoxSectionRequired,

    #[error("username must be non-empty")]
    EmptyUsername,

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

    #[error("invalid awdp exploit script path '{0}': {1}")]
    InvalidExploitPath(String, String),

    #[error("invalid awdp source_code_dir '{0}': {1}")]
    InvalidSourceCodeDir(String, String),

    #[error("docker.recommended_resources.{0} must be > 0")]
    InvalidResource(String),

    #[error("invalid container port {0}: must be 1..=65535")]
    InvalidPort(u16),
}

impl From<ContentFieldError> for GameBoxMetaError {
    fn from(e: ContentFieldError) -> Self {
        match e {
            ContentFieldError::EmptyName => GameBoxMetaError::EmptyName,
            ContentFieldError::EmptyAuthor => GameBoxMetaError::EmptyAuthor,
            ContentFieldError::EmptyCategory => GameBoxMetaError::EmptyCategory,
            ContentFieldError::EmptyDescription => GameBoxMetaError::EmptyDescription,
            ContentFieldError::InvalidVersion(reason) => GameBoxMetaError::InvalidVersion(reason),
            ContentFieldError::InvalidTag(tag) => GameBoxMetaError::InvalidTag(tag),
            ContentFieldError::InvalidPort(port) => GameBoxMetaError::InvalidPort(port),
            ContentFieldError::InvalidResource(field) => {
                GameBoxMetaError::InvalidResource(field.to_string())
            }
        }
    }
}

impl From<SafeNameError> for GameBoxMetaError {
    fn from(e: SafeNameError) -> Self {
        match e {
            SafeNameError::Invalid(raw) => GameBoxMetaError::InvalidSafeName(raw),
            SafeNameError::Underivable => GameBoxMetaError::SafeNameRequired,
        }
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// GameBox 包顶层清单（`meta.toml`）。
///
/// 亦导出为 [`GameBoxManifest`]。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameBoxMeta {
    pub name: String,
    /// 官方 version：`^\d+\.\d+\.\d+$`。成为 canonical image tag 的一部分。
    pub version: String,
    pub author: String,
    pub category: String,
    pub difficulty: Difficulty,
    pub tags: Vec<String>,
    pub description: String,
    /// 可选显式 slug；缺省从 **content id（目录名）** 派生。
    #[serde(default)]
    pub safe_name: Option<String>,
    /// 官方公共 `[docker]` 段。
    #[serde(default)]
    pub docker: Option<DockerConfig>,
    /// FCMC AWD 运行时扩展：登录用户名 + readiness 探针。
    ///
    /// **可选** —— 官方 Content Contract 不要求它；缺失时 metadata 依然合法，
    /// 只有真正需要运行时能力的操作（`check --runtime` / AWD 部署）才会报错。
    #[serde(default)]
    pub gamebox: Option<GameBoxSection>,
    /// Optional trusted judge script reference (never part of Docker build context).
    #[serde(default)]
    pub judge: Option<JudgeManifest>,
    /// Optional AWD-P exploit script reference (never part of Docker build context).
    #[serde(default)]
    pub awdp: Option<AwdpManifest>,
}

/// 部分调用方/计划文档偏好的别名。
pub type GameBoxManifest = GameBoxMeta;

/// `[gamebox]` section —— FCMC/AWD 运行时扩展（严格）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GameBoxSection {
    /// Unprivileged user inside the container.
    pub username: String,
    /// Readiness probes (HTTP / TCP). Not Docker CMD healthchecks.
    #[serde(default)]
    pub healthchecks: Vec<GameBoxHealthcheck>,
}

/// 向后兼容别名——优先使用 [`GameBoxSection`]。
pub type GameBoxConfig = GameBoxSection;

/// 带标签的健康检查联合体（`type = "http" | "tcp"`）。
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
    /// 兼容键名：`script`（规范）与 `check_script`（用户偏好）均可解析。
    #[serde(alias = "check_script")]
    pub script: String,
}

/// `[awdp]` section — AWD-P capability（可选 section，出现则内部字段全部必填）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AwdpManifest {
    /// Relative path that must start with `awdp/` (e.g. `awdp/exploit.py`).
    pub exploit_script: String,
    /// Container-internal source directory (absolute path, e.g. `/var/www/html`)
    /// that the platform packages into a source zip provided to players.
    pub source_code_dir: String,
}

// ---------------------------------------------------------------------------
// Canonical normalized spec (stable JSON for spec_digest)
// ---------------------------------------------------------------------------

/// 用于 `spec_json` / digest 的规范、完全物化视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedGameBoxSpec {
    pub name: String,
    pub version: String,
    pub author: String,
    pub category: String,
    pub difficulty: Difficulty,
    pub tags: Vec<String>,
    pub description: String,
    pub safe_name: String,
    /// `[gamebox].username`；未声明 `[gamebox]` 时为 `None`。
    pub username: Option<String>,
    pub healthchecks: Vec<NormalizedHealthcheck>,
    pub recommended_resources: RecommendedResources,
    pub judge_script: Option<String>,
    pub exploit_script: Option<String>,
    pub source_code_dir: Option<String>,
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
// Judge / AWDP path helpers
// ---------------------------------------------------------------------------

/// 校验裁判脚本路径：相对路径、以 `judge/` 开头、无 `..`、非绝对路径。
pub fn validate_judge_path(path: &str) -> Result<(), GameBoxMetaError> {
    validate_script_path(path, "judge/", |p, msg| {
        GameBoxMetaError::InvalidJudgePath(p.to_string(), msg.into())
    })
}

/// 校验 AWD-P 攻击脚本路径：相对路径、以 `awdp/` 开头、无 `..`、非绝对路径。
pub fn validate_awdp_path(path: &str) -> Result<(), GameBoxMetaError> {
    validate_script_path(path, "awdp/", |p, msg| {
        GameBoxMetaError::InvalidExploitPath(p.to_string(), msg.into())
    })
}

/// 校验 AWD-P 源码目录：容器内绝对路径（以 `/` 开头）、无 `..`、不以 `/` 结尾（除根）。
pub fn validate_source_code_dir(dir: &str) -> Result<(), GameBoxMetaError> {
    if dir.is_empty() {
        return Err(GameBoxMetaError::InvalidSourceCodeDir(
            dir.to_string(),
            "empty path".into(),
        ));
    }
    if !dir.starts_with('/') {
        return Err(GameBoxMetaError::InvalidSourceCodeDir(
            dir.to_string(),
            "must be an absolute container path (start with '/')".into(),
        ));
    }
    if dir.contains("..") {
        return Err(GameBoxMetaError::InvalidSourceCodeDir(
            dir.to_string(),
            "must not contain '..'".into(),
        ));
    }
    if dir.len() > 1 && dir.ends_with('/') {
        return Err(GameBoxMetaError::InvalidSourceCodeDir(
            dir.to_string(),
            "must not end with '/'".into(),
        ));
    }
    Ok(())
}

fn validate_script_path(
    path: &str,
    prefix: &str,
    err: impl Fn(&str, &str) -> GameBoxMetaError,
) -> Result<(), GameBoxMetaError> {
    if path.is_empty() {
        return Err(err(path, "empty path"));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(err(path, "must be relative"));
    }
    // Windows drive / UNC
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return Err(err(path, "must be relative"));
    }
    if !path.starts_with(prefix) {
        return Err(err(path, "must start with the expected directory"));
    }
    if path.contains("..") {
        return Err(err(path, "must not contain '..'"));
    }
    if path.ends_with('/') || path == prefix.trim_end_matches('/') {
        return Err(err(path, "must point to a file"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// GameBoxMeta impl
// ---------------------------------------------------------------------------

impl GameBoxMeta {
    /// Parse TOML only (no semantic validation).
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

    /// Parse + semantic validation for a package whose **content id 是目录名**。
    pub fn parse_and_validate(toml_str: &str, content_id: &str) -> Result<Self, GameBoxMetaError> {
        let meta = Self::from_toml_str(toml_str)?;
        meta.validate(content_id)?;
        Ok(meta)
    }

    /// 解析 `safe_name`：显式值优先（trim 后校验），否则从 *content_id* 派生。
    pub fn resolved_safe_name(&self, content_id: &str) -> Result<String, GameBoxMetaError> {
        identity::resolve_safe_name(content_id, self.safe_name.as_deref())
            .map_err(GameBoxMetaError::from)
    }

    /// 语义校验：官方公共字段 + `[gamebox]` / `[judge]` / `[awdp]` 扩展（存在才校验）。
    pub fn validate(&self, content_id: &str) -> Result<(), GameBoxMetaError> {
        validate_content_fields(
            &self.name,
            &self.version,
            &self.author,
            &self.category,
            &self.description,
            &self.tags,
            self.docker.as_ref(),
        )?;

        let _ = self.resolved_safe_name(content_id)?;

        if let Some(ref gamebox) = self.gamebox {
            if gamebox.username.trim().is_empty() {
                return Err(GameBoxMetaError::EmptyUsername);
            }

            let mut seen = std::collections::HashSet::new();
            for hc in &gamebox.healthchecks {
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
        }

        if let Some(ref judge) = self.judge {
            validate_judge_path(&judge.script)?;
        }

        if let Some(ref awdp) = self.awdp {
            validate_awdp_path(&awdp.exploit_script)?;
            validate_source_code_dir(&awdp.source_code_dir)?;
        }

        Ok(())
    }

    /// 运行时/部署操作要求的 `[gamebox]` 扩展（operational，不是 content contract）。
    pub fn require_gamebox_section(&self) -> Result<&GameBoxSection, GameBoxMetaError> {
        self.gamebox
            .as_ref()
            .ok_or(GameBoxMetaError::GameBoxSectionRequired)
    }

    /// Produce a canonical, fully-materialised spec (sorted healthchecks, defaults filled).
    ///
    /// Callers should `validate(content_id)` first; this method also validates.
    pub fn normalize(&self, content_id: &str) -> Result<NormalizedGameBoxSpec, GameBoxMetaError> {
        self.validate(content_id)?;
        let safe_name = self.resolved_safe_name(content_id)?;

        let mut healthchecks: Vec<NormalizedHealthcheck> = self
            .gamebox
            .as_ref()
            .map(|gb| {
                gb.healthchecks
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
                        GameBoxHealthcheck::Tcp { port } => {
                            NormalizedHealthcheck::Tcp { port: *port }
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Canonical order: HTTP before TCP, then by port/path.
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

        // 资源唯一来源：`[docker.recommended_resources]`（GameBox 专属默认值）。
        let recommended_resources = self
            .docker
            .as_ref()
            .map(|d| d.materialize_resources(RecommendedResources::GAMEBOX_DEFAULTS))
            .unwrap_or(RecommendedResources::GAMEBOX_DEFAULTS);

        Ok(NormalizedGameBoxSpec {
            name: self.name.clone(),
            version: self.version.clone(),
            author: self.author.clone(),
            category: self.category.clone(),
            difficulty: self.difficulty,
            tags: self.tags.clone(),
            description: self.description.clone(),
            safe_name,
            username: self.gamebox.as_ref().map(|gb| gb.username.clone()),
            healthchecks,
            recommended_resources,
            judge_script: self.judge.as_ref().map(|j| j.script.clone()),
            exploit_script: self.awdp.as_ref().map(|a| a.exploit_script.clone()),
            source_code_dir: self.awdp.as_ref().map(|a| a.source_code_dir.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 官方 canonical minimal GameBox（无 `[gamebox]`）。
    const CANONICAL_MINIMAL: &str = r#"
name = "comment"
version = "1.0.0"
author = "dev@floatctf.local"
category = "misc"
difficulty = "medium"
tags = ["box"]
description = "GameBox fixture"

[docker]
port = 8080
"#;

    #[test]
    fn canonical_minimal_gamebox_without_gamebox_section_is_valid() {
        let meta = GameBoxMeta::parse_and_validate(CANONICAL_MINIMAL, "comment").unwrap();
        assert!(meta.gamebox.is_none());
        assert_eq!(meta.resolved_safe_name("comment").unwrap(), "comment");
        assert_eq!(meta.docker.as_ref().unwrap().port, Some(8080));
        assert_eq!(meta.difficulty, Difficulty::Medium);

        // metadata 合法，但运行时要 [gamebox] → 明确的 operational 错误
        assert_eq!(
            meta.require_gamebox_section().unwrap_err(),
            GameBoxMetaError::GameBoxSectionRequired
        );

        let norm = meta.normalize("comment").unwrap();
        assert!(norm.username.is_none());
        assert!(norm.healthchecks.is_empty());
        assert_eq!(norm.recommended_resources.cpu_millis, 1000);
        assert_eq!(norm.recommended_resources.memory_bytes, 536_870_912);
    }

    const MINIMAL: &str = r#"
name = "TTT1"
version = "1.0.0"
author = "you@example.com"
category = "web"
difficulty = "easy"
tags = []
description = "hello"

[gamebox]
username = "floatctf"
"#;

    #[test]
    fn parse_minimal() {
        let meta = GameBoxMeta::parse_and_validate(MINIMAL, "TTT1").unwrap();
        assert_eq!(meta.name, "TTT1");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.resolved_safe_name("TTT1").unwrap(), "ttt1");
        assert!(meta.gamebox.as_ref().unwrap().healthchecks.is_empty());
        assert!(meta.judge.is_none());
    }

    #[test]
    fn reject_legacy_scoring_in_gamebox() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
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
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[gamebox.resources]
cpu_millis = 1
"#;
        assert!(GameBoxMeta::from_toml_str(toml).is_err());
    }

    #[test]
    fn reject_gamebox_recommended_resources() {
        // 资源唯一来源是 [docker.recommended_resources]（plan §19）。
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[gamebox.recommended_resources]
cpu_millis = 1000
"#;
        assert!(GameBoxMeta::from_toml_str(toml).is_err());
    }

    #[test]
    fn unknown_top_level_fields_are_ignored() {
        let toml = MINIMAL.replacen("\n[gamebox]", "\npoints = 42\n\n[gamebox]", 1);
        GameBoxMeta::parse_and_validate(&toml, "TTT1").unwrap();
    }

    #[test]
    fn version_rules() {
        for v in ["1.0.0", "1.2.3", "01.0.0"] {
            let toml = MINIMAL.replace("1.0.0", v);
            GameBoxMeta::parse_and_validate(&toml, "TTT1").unwrap();
        }
        for v in ["1.0.0-rc.1", "1.0.0+build.1", "not-a-version"] {
            let toml = MINIMAL.replace("1.0.0", v);
            let err = GameBoxMeta::parse_and_validate(&toml, "TTT1").unwrap_err();
            assert!(
                matches!(err, GameBoxMetaError::InvalidVersion(_)),
                "{v} must be rejected: {err}"
            );
        }
    }

    #[test]
    fn healthcheck_http_default_status_materialized() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[[gamebox.healthchecks]]
type = "http"
port = 80
path = "/"
"#;
        let meta = GameBoxMeta::parse_and_validate(toml, "t").unwrap();
        let norm = meta.normalize("t").unwrap();
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
difficulty = "easy"
tags = []
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
    fn empty_username_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "  "
"#;
        let err = GameBoxMeta::parse_and_validate(toml, "t").unwrap_err();
        assert!(matches!(err, GameBoxMetaError::EmptyUsername));
    }

    #[test]
    fn normalize_sorts_healthchecks() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
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
        let a = GameBoxMeta::parse_and_validate(toml, "t")
            .unwrap()
            .normalize("t")
            .unwrap();
        let toml_rev = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
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
        let b = GameBoxMeta::parse_and_validate(toml_rev, "t")
            .unwrap()
            .normalize("t")
            .unwrap();
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
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
    fn unique_healthcheck_duplicate_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[[gamebox.healthchecks]]
type = "tcp"
port = 3306

[[gamebox.healthchecks]]
type = "tcp"
port = 3306
"#;
        let err = GameBoxMeta::parse_and_validate(toml, "t").unwrap_err();
        assert!(matches!(err, GameBoxMetaError::DuplicateHealthcheck));
    }

    #[test]
    fn healthcheck_path_and_status_rules() {
        let base = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[[gamebox.healthchecks]]
type = "http"
port = 80
"#;
        let err = GameBoxMeta::parse_and_validate(&format!("{base}path = \"no-slash\"\n"), "t")
            .unwrap_err();
        assert!(matches!(err, GameBoxMetaError::InvalidHealthcheckPath(_)));

        let err = GameBoxMeta::parse_and_validate(
            &format!("{base}path = \"/\"\nexpected_status = 99\n"),
            "t",
        )
        .unwrap_err();
        assert!(matches!(err, GameBoxMetaError::InvalidExpectedStatus(99)));

        let err = GameBoxMeta::parse_and_validate(
            &format!("{base}path = \"/\"\nexpected_status = 600\n"),
            "t",
        )
        .unwrap_err();
        assert!(matches!(err, GameBoxMetaError::InvalidExpectedStatus(600)));
    }

    #[test]
    fn judge_and_awdp_paths() {
        assert!(validate_judge_path("judge/check.py").is_ok());
        assert!(validate_judge_path("/judge/check.py").is_err());
        assert!(validate_judge_path("judge/../x.py").is_err());
        assert!(validate_judge_path("scripts/check.py").is_err());
        assert!(validate_judge_path("judge/").is_err());

        assert!(validate_awdp_path("awdp/exploit.py").is_ok());
        assert!(validate_awdp_path("/awdp/exploit.py").is_err());
        assert!(validate_awdp_path("awdp/../x.py").is_err());
        assert!(validate_awdp_path("scripts/exploit.py").is_err());

        assert!(validate_source_code_dir("/var/www/html").is_ok());
        assert!(validate_source_code_dir("/").is_ok());
        assert!(validate_source_code_dir("var/www").is_err());
        assert!(validate_source_code_dir("/var/www/../html").is_err());
        assert!(validate_source_code_dir("/var/www/html/").is_err());
        assert!(validate_source_code_dir("").is_err());
    }

    #[test]
    fn judge_accepts_check_script_alias() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[judge]
check_script = "judge/check.py"
"#;
        let meta = GameBoxMeta::parse_and_validate(toml, "t").unwrap();
        assert_eq!(meta.judge.as_ref().unwrap().script, "judge/check.py");
    }

    #[test]
    fn parse_with_awdp() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[judge]
script = "judge/check.py"

[awdp]
exploit_script = "awdp/exploit.py"
source_code_dir = "/var/www/html"
"#;
        let meta = GameBoxMeta::parse_and_validate(toml, "t").unwrap();
        let awdp = meta.awdp.as_ref().unwrap();
        assert_eq!(awdp.exploit_script, "awdp/exploit.py");
        assert_eq!(awdp.source_code_dir, "/var/www/html");
        let norm = meta.normalize("t").unwrap();
        assert_eq!(norm.judge_script.as_deref(), Some("judge/check.py"));
        assert_eq!(norm.exploit_script.as_deref(), Some("awdp/exploit.py"));
        assert_eq!(norm.source_code_dir.as_deref(), Some("/var/www/html"));
        assert_eq!(norm.username.as_deref(), Some("u"));
    }

    #[test]
    fn awdp_requires_source_code_dir() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[awdp]
exploit_script = "awdp/exploit.py"
"#;
        assert!(GameBoxMeta::parse_and_validate(toml, "t").is_err());
    }

    #[test]
    fn reject_bad_awdp_path_and_source_dir() {
        let bad_path = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[awdp]
exploit_script = "judge/exploit.py"
source_code_dir = "/var/www/html"
"#;
        let err = GameBoxMeta::parse_and_validate(bad_path, "t").unwrap_err();
        assert!(matches!(err, GameBoxMetaError::InvalidExploitPath(_, _)));

        let bad_dir = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[awdp]
exploit_script = "awdp/exploit.py"
source_code_dir = "var/www/html"
"#;
        let err = GameBoxMeta::parse_and_validate(bad_dir, "t").unwrap_err();
        assert!(matches!(err, GameBoxMetaError::InvalidSourceCodeDir(_, _)));
    }

    #[test]
    fn resources_come_from_docker_section() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[gamebox]
username = "u"

[docker]
port = 80

[docker.recommended_resources]
cpu_millis = 250
"#;
        let norm = GameBoxMeta::parse_and_validate(toml, "t")
            .unwrap()
            .normalize("t")
            .unwrap();
        assert_eq!(norm.recommended_resources.cpu_millis, 250);
        assert_eq!(norm.recommended_resources.memory_bytes, 536_870_912);
        assert_eq!(norm.recommended_resources.pids_limit, 100);
    }

    #[test]
    fn safe_name_comes_from_content_id() {
        let toml = r#"
name = "琪露诺的完美数学教室"
version = "1.0.0"
author = "a"
category = "misc"
difficulty = "easy"
tags = []
description = "d"
"#;
        let meta = GameBoxMeta::parse_and_validate(toml, "Cirno's perfect math class").unwrap();
        assert_eq!(
            meta.resolved_safe_name("Cirno's perfect math class")
                .unwrap(),
            "cirnos-perfect-math-class"
        );
    }

    #[test]
    fn empty_description_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = ""
"#;
        let err = GameBoxMeta::parse_and_validate(toml, "t").unwrap_err();
        assert!(matches!(err, GameBoxMetaError::EmptyDescription));
    }
}
