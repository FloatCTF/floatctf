//! 官方 Content Contract 的公共部分：`difficulty` / `tags` / `[docker]`。
//!
//! Challenge 与 GameBox **共用**这些定义，不允许各域各写一份。
//! 规则来源：`floatctf-content/scripts/content.py`
//! （`REQUIRED_FIELDS` / `TEXT_FIELDS` / `DIFFICULTIES` /
//! `POSITIVE_RESOURCE_FIELDS` / `validate_meta`）。

use serde::{Deserialize, Serialize};

use crate::metadata::identity;

// ---------------------------------------------------------------------------
// difficulty
// ---------------------------------------------------------------------------

/// 官方 `DIFFICULTIES` 的唯一合法值定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Unknown,
    Beginner,
    Easy,
    Medium,
    Hard,
    Expert,
}

impl Difficulty {
    pub const ALL: [Difficulty; 6] = [
        Difficulty::Unknown,
        Difficulty::Beginner,
        Difficulty::Easy,
        Difficulty::Medium,
        Difficulty::Hard,
        Difficulty::Expert,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Difficulty::Unknown => "unknown",
            Difficulty::Beginner => "beginner",
            Difficulty::Easy => "easy",
            Difficulty::Medium => "medium",
            Difficulty::Hard => "hard",
            Difficulty::Expert => "expert",
        }
    }
}

impl std::fmt::Display for Difficulty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// resources
// ---------------------------------------------------------------------------

/// 完全物化的资源建议（normalize 后的产物；平台落库用）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecommendedResources {
    pub cpu_millis: i64,
    pub memory_bytes: i64,
    pub pids_limit: i64,
}

impl RecommendedResources {
    /// Challenge 缺省建议：500m / 256 MiB / 100。
    pub const CHALLENGE_DEFAULTS: RecommendedResources = RecommendedResources {
        cpu_millis: 500,
        memory_bytes: 268_435_456,
        pids_limit: 100,
    };

    /// GameBox 缺省建议：1000m / 512 MiB / 100。
    pub const GAMEBOX_DEFAULTS: RecommendedResources = RecommendedResources {
        cpu_millis: 1000,
        memory_bytes: 536_870_912,
        pids_limit: 100,
    };
}

impl Default for RecommendedResources {
    fn default() -> Self {
        Self::GAMEBOX_DEFAULTS
    }
}

/// `[docker.recommended_resources]` 的 **partial** 表示。
///
/// 官方 validator 只要求“出现即 positive integer”，不要求三个字段齐全
/// （`content.py::validate_meta`）。缺省值在 normalize 阶段物化。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecommendedResourcesInput {
    #[serde(default)]
    pub cpu_millis: Option<i64>,
    #[serde(default)]
    pub memory_bytes: Option<i64>,
    #[serde(default)]
    pub pids_limit: Option<i64>,
}

impl RecommendedResourcesInput {
    /// 用 *defaults* 补齐未出现的字段。
    pub fn materialize(&self, defaults: RecommendedResources) -> RecommendedResources {
        RecommendedResources {
            cpu_millis: self.cpu_millis.unwrap_or(defaults.cpu_millis),
            memory_bytes: self.memory_bytes.unwrap_or(defaults.memory_bytes),
            pids_limit: self.pids_limit.unwrap_or(defaults.pids_limit),
        }
    }
}

// ---------------------------------------------------------------------------
// [docker]
// ---------------------------------------------------------------------------

/// 官方公共 `[docker]` 段（Challenge / GameBox 共用）。
///
/// `port` 是 **可选**：`[docker]` 出现但只有 `recommended_resources` 也合法。
/// 是否为容器由 `src/Dockerfile` 是否存在决定，**不是**由 `[docker]` 决定。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DockerConfig {
    /// 唯一暴露的 TCP 端口；出现时必须位于 1..=65535。
    #[serde(default)]
    pub port: Option<u16>,
    /// 作者建议资源（partial；platform 会与上限比对）。
    #[serde(default)]
    pub recommended_resources: Option<RecommendedResourcesInput>,
}

impl DockerConfig {
    /// 物化资源建议（未配置 → *defaults*）。
    pub fn materialize_resources(&self, defaults: RecommendedResources) -> RecommendedResources {
        match &self.recommended_resources {
            Some(input) => input.materialize(defaults),
            None => defaults,
        }
    }
}

// ---------------------------------------------------------------------------
// 公共字段校验
// ---------------------------------------------------------------------------

/// 官方 Content Contract 公共字段的校验错误。
///
/// 各 meta 类型的错误枚举通过 `From` 映射，保持对外错误类型稳定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentFieldError {
    EmptyName,
    EmptyAuthor,
    EmptyCategory,
    EmptyDescription,
    InvalidVersion(String),
    /// `tags` 元素 strip 后为空。
    InvalidTag(String),
    InvalidPort(u16),
    InvalidResource(&'static str),
}

/// 校验 Challenge / GameBox **完全相同**的公共字段。
///
/// 覆盖：`name` / `version` / `author` / `category` / `description` 非空、
/// `version` 匹配 `^\d+\.\d+\.\d+$`、`tags` 每项 strip 后非空、
/// `[docker].port` ∈ 1..=65535、`[docker.recommended_resources]` 出现的字段 > 0。
pub fn validate_content_fields(
    name: &str,
    version: &str,
    author: &str,
    category: &str,
    description: &str,
    tags: &[String],
    docker: Option<&DockerConfig>,
) -> Result<(), ContentFieldError> {
    if name.trim().is_empty() {
        return Err(ContentFieldError::EmptyName);
    }
    if author.trim().is_empty() {
        return Err(ContentFieldError::EmptyAuthor);
    }
    if category.trim().is_empty() {
        return Err(ContentFieldError::EmptyCategory);
    }
    if description.trim().is_empty() {
        return Err(ContentFieldError::EmptyDescription);
    }

    identity::validate_version(version).map_err(ContentFieldError::InvalidVersion)?;

    // tags: array of string，每项 strip 后必须非空（允许 []）
    for tag in tags {
        if tag.trim().is_empty() {
            return Err(ContentFieldError::InvalidTag(tag.clone()));
        }
    }

    if let Some(docker) = docker {
        if let Some(port) = docker.port.filter(|port| *port == 0) {
            return Err(ContentFieldError::InvalidPort(port));
        }
        if let Some(res) = &docker.recommended_resources {
            if res.cpu_millis.is_some_and(|v| v <= 0) {
                return Err(ContentFieldError::InvalidResource("cpu_millis"));
            }
            if res.memory_bytes.is_some_and(|v| v <= 0) {
                return Err(ContentFieldError::InvalidResource("memory_bytes"));
            }
            if res.pids_limit.is_some_and(|v| v <= 0) {
                return Err(ContentFieldError::InvalidResource("pids_limit"));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[derive(Deserialize)]
    struct DifficultyWrapper {
        d: Difficulty,
    }

    #[test]
    fn difficulty_serde_roundtrip_lowercase() {
        assert_eq!(Difficulty::ALL.len(), 6);
        for difficulty in Difficulty::ALL {
            let raw = format!("d = \"{}\"", difficulty.as_str());
            let parsed: DifficultyWrapper = toml::from_str(&raw).unwrap();
            assert_eq!(parsed.d, difficulty);
            // serde 也以小写序列化（spec_digest 稳定）
            assert_eq!(
                serde_json::to_string(&difficulty).unwrap(),
                format!("\"{}\"", difficulty.as_str())
            );
        }
        assert!(
            toml::from_str::<DifficultyWrapper>("d = \"impossible\"").is_err(),
            "invalid difficulty must be rejected"
        );
    }

    #[test]
    fn resources_partial_is_valid_and_materializes_defaults() {
        let docker: DockerConfig = toml::from_str(
            r#"
            [recommended_resources]
            cpu_millis = 500
            "#,
        )
        .unwrap();
        assert!(docker.port.is_none());
        assert!(validate_content_fields("n", "1.0.0", "a", "c", "d", &[], Some(&docker)).is_ok());
        let res = docker.materialize_resources(RecommendedResources::CHALLENGE_DEFAULTS);
        assert_eq!(res.cpu_millis, 500);
        assert_eq!(res.memory_bytes, 268_435_456);
        assert_eq!(res.pids_limit, 100);
    }

    #[test]
    fn empty_docker_table_is_valid() {
        let docker: DockerConfig = toml::from_str("").unwrap();
        assert_eq!(docker, DockerConfig::default());
        assert!(validate_content_fields("n", "1.0.0", "a", "c", "d", &[], Some(&docker)).is_ok());
    }

    #[test]
    fn tags_and_description_rules() {
        assert!(validate_content_fields("n", "1.0.0", "a", "c", "d", &tags(&[]), None).is_ok());
        assert_eq!(
            validate_content_fields("n", "1.0.0", "a", "c", "d", &tags(&[""]), None),
            Err(ContentFieldError::InvalidTag(String::new()))
        );
        assert_eq!(
            validate_content_fields("n", "1.0.0", "a", "c", "d", &tags(&["web", "   "]), None),
            Err(ContentFieldError::InvalidTag("   ".to_string()))
        );
        assert_eq!(
            validate_content_fields("n", "1.0.0", "a", "c", "   ", &[], None),
            Err(ContentFieldError::EmptyDescription)
        );
    }

    #[test]
    fn resource_must_be_positive_when_present() {
        let docker: DockerConfig =
            toml::from_str("[recommended_resources]\npids_limit = 0\n").unwrap();
        assert_eq!(
            validate_content_fields("n", "1.0.0", "a", "c", "d", &[], Some(&docker)),
            Err(ContentFieldError::InvalidResource("pids_limit"))
        );
    }

    #[test]
    fn port_zero_rejected() {
        let docker: DockerConfig = toml::from_str("port = 0\n").unwrap();
        assert_eq!(
            validate_content_fields("n", "1.0.0", "a", "c", "d", &[], Some(&docker)),
            Err(ContentFieldError::InvalidPort(0))
        );
    }
}
