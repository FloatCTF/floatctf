//! Challenge 包元数据模型与解析。
//!
//! 公共字段（`name` / `version` / `author` / `category` / `difficulty` / `tags` /
//! `description` / `safe_name` / `[flag]` / `[docker]`）严格对齐
//! `floatctf-content/scripts/content.py`；见 [`crate::metadata`] 顶部说明。

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::metadata::content::{
    ContentFieldError, Difficulty, DockerConfig, validate_content_fields,
};
use crate::metadata::identity::{self, SafeNameError};

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

    #[error("invalid flag config: {0}")]
    InvalidFlagConfig(String),

    #[error("static flag requires [flag] value")]
    StaticFlagRequired,

    #[error("invalid container port {0}: must be 1..=65535")]
    InvalidPort(u16),

    #[error("docker.recommended_resources.{0} must be > 0")]
    InvalidResource(String),

    #[error("invalid attachment path '{0}': {1}")]
    InvalidAttachmentPath(String, String),
}

impl From<ContentFieldError> for ChallengeMetaError {
    fn from(e: ContentFieldError) -> Self {
        match e {
            ContentFieldError::EmptyName => ChallengeMetaError::EmptyName,
            ContentFieldError::EmptyAuthor => ChallengeMetaError::EmptyAuthor,
            ContentFieldError::EmptyCategory => ChallengeMetaError::EmptyCategory,
            ContentFieldError::EmptyDescription => ChallengeMetaError::EmptyDescription,
            ContentFieldError::InvalidVersion(reason) => ChallengeMetaError::InvalidVersion(reason),
            ContentFieldError::InvalidTag(tag) => ChallengeMetaError::InvalidTag(tag),
            ContentFieldError::InvalidPort(port) => ChallengeMetaError::InvalidPort(port),
            ContentFieldError::InvalidResource(field) => {
                ChallengeMetaError::InvalidResource(field.to_string())
            }
        }
    }
}

impl From<SafeNameError> for ChallengeMetaError {
    fn from(e: SafeNameError) -> Self {
        match e {
            SafeNameError::Invalid(raw) => ChallengeMetaError::InvalidSafeName(raw),
            SafeNameError::Underivable => ChallengeMetaError::SafeNameRequired,
        }
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Challenge 包顶层清单（`meta.toml`）。
///
/// 亦导出为 [`ChallengeManifest`]。
///
/// 公共 Content Contract 字段**不使用** `deny_unknown_fields`：floatctf-content
/// 的 validator 不会因为无关扩展字段拒绝 metadata，FCMC 也不应制造第二套更严
/// 的公共 contract。（`[flag]` 是 FCMC 拥有的运行时契约，仍然严格。）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChallengeMeta {
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
    /// 可选附件路径，必须位于 `attachment/` 下。
    #[serde(default)]
    pub attachment: Option<String>,
    /// 官方 Content Contract **不要求** `[flag]`；缺失即“运行时不注入 FLAG”。
    #[serde(default)]
    pub flag: Option<ChallengeFlagConfig>,
    /// 官方公共 `[docker]` 段（`port` 与 `recommended_resources` 均可选）。
    #[serde(default)]
    pub docker: Option<DockerConfig>,
}

/// 部分调用方/计划文档偏好的别名。
pub type ChallengeManifest = ChallengeMeta;

/// `[flag]` 段——FCMC 拥有的 flag 运行时契约。
///
/// serde 内部标签枚举会静默忽略 `deny_unknown_fields`，故
/// 反序列化经严格中间结构体（见手工 [`Deserialize`] 实现），拒绝历史/未知键
/// 如 `env_var` 以及动态 flag 上的 `value`。
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

// ---------------------------------------------------------------------------
// Canonical normalized spec (stable JSON for spec_digest)
// ---------------------------------------------------------------------------

/// 用于 `spec_json` / digest 的规范、完全物化视图。
///
/// **不得**包含静态 flag 明文（密钥）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedChallengeSpec {
    pub name: String,
    pub version: String,
    pub author: String,
    pub category: String,
    pub difficulty: Difficulty,
    pub tags: Vec<String>,
    pub description: String,
    pub safe_name: String,
    /// `Some("dynamic" | "static")`；未声明 `[flag]` 时为 `None`。
    pub flag_type: Option<String>,
    /// None for non-docker challenges.
    pub container_port: Option<u16>,
    pub recommended_resources: crate::metadata::RecommendedResources,
    pub attachment: Option<String>,
}

// ---------------------------------------------------------------------------
// Attachment path helper
// ---------------------------------------------------------------------------

/// 校验附件路径：非空、相对路径、位于 `attachment/` 下，
/// 不含 `..`，且不是目录。
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
    /// Parse TOML only (no semantic validation).
    ///
    /// 顶层公共字段宽松（未知字段忽略）；`[flag]` 严格。
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

    /// Parse + semantic validation for a package whose **content id 是目录名**。
    pub fn parse_and_validate(
        toml_str: &str,
        content_id: &str,
    ) -> Result<Self, ChallengeMetaError> {
        let meta = Self::from_toml_str(toml_str)?;
        meta.validate(content_id)?;
        Ok(meta)
    }

    /// 解析 `safe_name`：显式值优先（trim 后校验），否则从 *content_id* 派生。
    pub fn resolved_safe_name(&self, content_id: &str) -> Result<String, ChallengeMetaError> {
        identity::resolve_safe_name(content_id, self.safe_name.as_deref())
            .map_err(ChallengeMetaError::from)
    }

    /// Semantic validation（官方公共字段 + flag + 附件路径）。
    pub fn validate(&self, content_id: &str) -> Result<(), ChallengeMetaError> {
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

        let static_flag_without_value = matches!(
            &self.flag,
            Some(ChallengeFlagConfig::Static { value })
                if value.as_deref().map(str::trim).is_none_or(str::is_empty)
        );
        if static_flag_without_value {
            return Err(ChallengeMetaError::StaticFlagRequired);
        }

        if let Some(ref attachment) = self.attachment {
            validate_attachment_path(attachment).map_err(|reason| {
                ChallengeMetaError::InvalidAttachmentPath(attachment.clone(), reason)
            })?;
        }

        Ok(())
    }

    /// Static flag value (secret). `None` for dynamic flags / missing value /
    /// missing `[flag]`. The platform stores it in a secret column — never in
    /// logs/DTOs.
    pub fn static_flag_value(&self) -> Option<&str> {
        match &self.flag {
            Some(ChallengeFlagConfig::Static { value: Some(v) }) => Some(v.as_str()),
            _ => None,
        }
    }

    /// `"dynamic"` / `"static"`；未声明 `[flag]` 时为 `None`。
    pub fn flag_type(&self) -> Option<&'static str> {
        match &self.flag {
            Some(ChallengeFlagConfig::Dynamic) => Some("dynamic"),
            Some(ChallengeFlagConfig::Static { .. }) => Some("static"),
            None => None,
        }
    }

    /// Produce a canonical, fully-materialised spec (defaults filled).
    ///
    /// Callers should `validate(content_id)` first; this method also validates.
    pub fn normalize(
        &self,
        content_id: &str,
    ) -> Result<NormalizedChallengeSpec, ChallengeMetaError> {
        self.validate(content_id)?;
        let safe_name = self.resolved_safe_name(content_id)?;

        let container_port = self.docker.as_ref().and_then(|d| d.port);

        // Challenge default recommendations (500m CPU / 256MiB / 100 pids)
        // differ from the gamebox default; fill inline when absent.
        let recommended_resources = self
            .docker
            .as_ref()
            .map(|d| {
                d.materialize_resources(crate::metadata::RecommendedResources::CHALLENGE_DEFAULTS)
            })
            .unwrap_or(crate::metadata::RecommendedResources::CHALLENGE_DEFAULTS);

        Ok(NormalizedChallengeSpec {
            name: self.name.clone(),
            version: self.version.clone(),
            author: self.author.clone(),
            category: self.category.clone(),
            difficulty: self.difficulty,
            tags: self.tags.clone(),
            description: self.description.clone(),
            safe_name,
            flag_type: self.flag_type().map(str::to_string),
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
difficulty = "easy"
tags = ["web"]
description = "hello"

[flag]
type = "dynamic"

[docker]
port = 80
"#;

    #[test]
    fn parse_minimal_dynamic() {
        let meta = ChallengeMeta::parse_and_validate(MINIMAL, "easy-web-01").unwrap();
        assert_eq!(meta.name, "Easy Web 01");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.difficulty, Difficulty::Easy);
        assert_eq!(meta.tags, vec!["web".to_string()]);
        assert_eq!(
            meta.resolved_safe_name("easy-web-01").unwrap(),
            "easy-web-01"
        );
        assert!(matches!(meta.flag, Some(ChallengeFlagConfig::Dynamic)));
        let docker = meta.docker.as_ref().unwrap();
        assert_eq!(docker.port, Some(80));
        assert!(docker.recommended_resources.is_none());
    }

    #[test]
    fn missing_flag_is_contract_valid() {
        let toml = r#"
name = "cookie"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "unknown"
tags = []
description = "d"
"#;
        let meta = ChallengeMeta::parse_and_validate(toml, "cookie").unwrap();
        assert!(meta.flag.is_none());
        assert!(meta.static_flag_value().is_none());
        let norm = meta.normalize("cookie").unwrap();
        assert!(norm.flag_type.is_none());
    }

    #[test]
    fn missing_difficulty_or_tags_rejected() {
        for missing in [
            (
                "difficulty",
                "name = \"t\"\nversion = \"1.0.0\"\nauthor = \"a\"\ncategory = \"c\"\ntags = []\ndescription = \"d\"\n",
            ),
            (
                "tags",
                "name = \"t\"\nversion = \"1.0.0\"\nauthor = \"a\"\ncategory = \"c\"\ndifficulty = \"easy\"\ndescription = \"d\"\n",
            ),
        ] {
            let err = ChallengeMeta::from_toml_str(missing.1).unwrap_err();
            assert!(
                matches!(err, ChallengeMetaError::Parse(_)),
                "missing {} must be rejected: {err}",
                missing.0
            );
        }
    }

    #[test]
    fn invalid_difficulty_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "impossible"
tags = []
description = "d"
"#;
        assert!(ChallengeMeta::from_toml_str(toml).is_err());
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
description = "   "
"#;
        let err = ChallengeMeta::parse_and_validate(toml, "t").unwrap_err();
        assert!(matches!(err, ChallengeMetaError::EmptyDescription));
    }

    #[test]
    fn static_with_value() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[flag]
type = "static"
value = "flag{secret}"
"#;
        let meta = ChallengeMeta::parse_and_validate(toml, "t").unwrap();
        assert_eq!(meta.static_flag_value(), Some("flag{secret}"));
        let norm = meta.normalize("t").unwrap();
        assert_eq!(norm.flag_type.as_deref(), Some("static"));
    }

    #[test]
    fn static_without_value_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[flag]
type = "static"
"#;
        let err = ChallengeMeta::parse_and_validate(toml, "t").unwrap_err();
        assert!(matches!(err, ChallengeMetaError::StaticFlagRequired));
    }

    #[test]
    fn static_empty_value_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[flag]
type = "static"
value = ""
"#;
        let err = ChallengeMeta::parse_and_validate(toml, "t").unwrap_err();
        assert!(matches!(err, ChallengeMetaError::StaticFlagRequired));
    }

    #[test]
    fn dynamic_with_value_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
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
    fn legacy_flag_env_var_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
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
    fn unknown_top_level_fields_are_ignored() {
        // 公共 Content Contract 不拒绝无关扩展字段（与 content.py 一致）。
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
points = 500
custom_extension = "x"
"#;
        ChallengeMeta::parse_and_validate(toml, "t").unwrap();
    }

    #[test]
    fn string_port_rejected() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

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
difficulty = "easy"
tags = []
description = "d"

[docker]
port = 0
"#;
        let err = ChallengeMeta::parse_and_validate(toml, "t").unwrap_err();
        assert!(matches!(err, ChallengeMetaError::InvalidPort(0)));
    }

    #[test]
    fn partial_recommended_resources_valid() {
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"

[docker.recommended_resources]
cpu_millis = 500
"#;
        let norm = ChallengeMeta::parse_and_validate(toml, "t")
            .unwrap()
            .normalize("t")
            .unwrap();
        assert_eq!(norm.recommended_resources.cpu_millis, 500);
        assert_eq!(norm.recommended_resources.memory_bytes, 268_435_456);
        assert_eq!(norm.recommended_resources.pids_limit, 100);
    }

    #[test]
    fn safe_name_comes_from_content_id_not_name() {
        let toml = r#"
name = "琪露诺的完美数学教室"
version = "1.0.0"
author = "a"
category = "misc"
difficulty = "easy"
tags = []
description = "d"
"#;
        let meta = ChallengeMeta::parse_and_validate(toml, "Cirno's perfect math class").unwrap();
        assert_eq!(
            meta.resolved_safe_name("Cirno's perfect math class")
                .unwrap(),
            "cirnos-perfect-math-class"
        );
        // 从 name 派生会失败（纯中文）——证明没有走 name
        assert!(meta.resolved_safe_name("题目").is_err());
    }

    #[test]
    fn safe_name_derivation() {
        assert_eq!(
            identity::derive_safe_name("Easy Web 01").as_deref(),
            Some("easy-web-01")
        );
        // 非 ASCII-only content id 且无显式 safe_name → SafeNameRequired
        let toml = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#;
        let err = ChallengeMeta::parse_and_validate(toml, "注入题目").unwrap_err();
        assert!(matches!(err, ChallengeMetaError::SafeNameRequired));
    }

    #[test]
    fn explicit_safe_name_valid_and_invalid() {
        let valid = r#"
name = "注入题目"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
safe_name = "zhu-ru"
"#;
        let meta = ChallengeMeta::parse_and_validate(valid, "题目").unwrap();
        assert_eq!(meta.resolved_safe_name("题目").unwrap(), "zhu-ru");

        let invalid = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
safe_name = "Easy Web"
"#;
        let err = ChallengeMeta::parse_and_validate(invalid, "t").unwrap_err();
        assert!(matches!(err, ChallengeMetaError::InvalidSafeName(_)));
    }

    #[test]
    fn version_rules() {
        for v in ["1.0.0", "01.0.0", "12.34.56"] {
            let toml = format!(
                r#"
name = "t"
version = "{v}"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#
            );
            ChallengeMeta::parse_and_validate(&toml, "t").unwrap();
        }

        for v in ["1.0", "v1.0.0", "1.0.0-rc.1", "1.0.0+build.1", "abc"] {
            let toml = format!(
                r#"
name = "t"
version = "{v}"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
"#
            );
            let err = ChallengeMeta::parse_and_validate(&toml, "t").unwrap_err();
            assert!(
                matches!(err, ChallengeMetaError::InvalidVersion(_)),
                "version {v} must be rejected: {err}"
            );
        }
    }

    #[test]
    fn attachment_rules() {
        let ok = r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
attachment = "attachment/src.zip"
"#;
        let meta = ChallengeMeta::parse_and_validate(ok, "t").unwrap();
        assert_eq!(meta.attachment.as_deref(), Some("attachment/src.zip"));

        for bad in ["../x", "/x", "src/x"] {
            let toml = format!(
                r#"
name = "t"
version = "1.0.0"
author = "a"
category = "web"
difficulty = "easy"
tags = []
description = "d"
attachment = "{bad}"
"#
            );
            let err = ChallengeMeta::parse_and_validate(&toml, "t").unwrap_err();
            assert!(
                matches!(err, ChallengeMetaError::InvalidAttachmentPath(_, _)),
                "attachment path must be rejected: {bad}"
            );
        }
    }

    #[test]
    fn normalize_fills_defaults() {
        let meta = ChallengeMeta::parse_and_validate(MINIMAL, "easy-web-01").unwrap();
        let norm = meta.normalize("easy-web-01").unwrap();
        assert_eq!(norm.safe_name, "easy-web-01");
        assert_eq!(norm.flag_type.as_deref(), Some("dynamic"));
        assert_eq!(norm.container_port, Some(80));
        assert_eq!(norm.recommended_resources.cpu_millis, 500);
        assert_eq!(norm.recommended_resources.memory_bytes, 268_435_456);
        assert_eq!(norm.recommended_resources.pids_limit, 100);
        assert!(norm.attachment.is_none());
        assert_eq!(norm.difficulty, Difficulty::Easy);
        assert_eq!(norm.tags, vec!["web".to_string()]);
    }

    #[test]
    fn canonical_image_ref() {
        assert_eq!(
            identity::content_image_ref(
                identity::ArtifactKind::Challenge,
                "registry.example",
                "easy-web",
                "1.0.0"
            ),
            "registry.example/easy-web:challenge-v1.0.0"
        );
        assert_eq!(
            identity::content_image_ref(
                identity::ArtifactKind::GameBox,
                "registry.example",
                "easy-web",
                "1.0.0"
            ),
            "registry.example/easy-web:gamebox-v1.0.0"
        );
    }
}
