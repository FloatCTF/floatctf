//! 镜像/包身份与钉扎相关类型。

/// 制品命名空间：镜像名中的 `challenges/` 与 `gameboxes/`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    Challenge,
    GameBox,
}

impl ArtifactKind {
    /// Image namespace directory used in canonical image refs.
    pub fn dir(self) -> &'static str {
        match self {
            ArtifactKind::Challenge => "challenges",
            ArtifactKind::GameBox => "gameboxes",
        }
    }
}

/// 由人类可读 `name` 派生 Docker 仓库安全的 slug。
///
/// 规则：
/// - 小写 ASCII
/// - 空白 → `-`
/// - 不支持的标点 → `-`
/// - 合并重复分隔符
/// - 去掉首尾分隔符
/// - 空 / 纯非 ASCII → `None`（调用方须要求显式 `safe_name`）
pub fn derive_safe_name(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut last_sep = false;
    let mut saw_alnum = false;

    for c in name.chars() {
        let mapped = if c.is_ascii_alphanumeric() {
            saw_alnum = true;
            Some(c.to_ascii_lowercase())
        } else if c.is_ascii_whitespace() || c == '_' || c == '-' || c == '.' {
            Some('-')
        } else if c.is_ascii() {
            // other ASCII punctuation → separator
            Some('-')
        } else {
            // non-ASCII: drop (no pinyin); may leave empty
            None
        };

        if let Some(ch) = mapped {
            if ch == '-' {
                if !out.is_empty() && !last_sep {
                    out.push('-');
                    last_sep = true;
                }
            } else {
                out.push(ch);
                last_sep = false;
            }
        }
    }

    while out.ends_with('-') || out.ends_with('_') {
        out.pop();
    }

    if !saw_alnum || out.is_empty() {
        return None;
    }

    // Must start with [a-z0-9]
    if validate_safe_name(&out).is_ok() {
        Some(out)
    } else {
        None
    }
}

/// 校验显式 `safe_name`：`^[a-z0-9][a-z0-9_-]*$`。
pub fn validate_safe_name(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err(format!("empty safe_name: '{s}'"));
    }
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return Err(format!("empty safe_name: '{s}'"));
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(format!(
            "invalid safe_name '{s}': must match ^[a-z0-9][a-z0-9_-]*$"
        ));
    }
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
            return Err(format!(
                "invalid safe_name '{s}': must match ^[a-z0-9][a-z0-9_-]*$"
            ));
        }
    }
    Ok(())
}

/// 将包版本解析为 SemVer，**不含** build metadata（拒绝 `+…`）。
/// 允许预发布（`1.0.0-rc.1`）。
pub fn validate_version(version: &str) -> Result<semver::Version, String> {
    match semver::Version::parse(version) {
        Ok(v) => {
            // Double-check: semver crate accepts build metadata after `+`.
            if !v.build.is_empty() {
                Err(format!("build metadata is not allowed: '{version}'"))
            } else {
                Ok(v)
            }
        }
        Err(e) => Err(format!("invalid SemVer '{version}': {e}")),
    }
}

/// `{registry_prefix}/{challenges|gameboxes}/{safe_name}:{version}` ——唯一权威
/// 镜像命名实现。
///
/// `registry_prefix` 来自**平台配置**，绝不来自 `meta.toml`。
/// CLI 未提供时的默认值：`"floatctf"`。
pub fn build_artifact_image_ref(
    kind: ArtifactKind,
    registry_prefix: &str,
    safe_name: &str,
    version: &str,
) -> String {
    let prefix = registry_prefix.trim_end_matches('/');
    format!("{prefix}/{}/{safe_name}:{version}", kind.dir())
}
