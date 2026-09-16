//! Content 身份规则：content id → safe_name → canonical image ref。
//!
//! 本模块是 `FloatCTF/floatctf-content@main` 中 `scripts/content.py` 的 **逐条 Rust
//! 复刻**（`derive_safe_name` / `SAFE_NAME_PATTERN` / `VERSION_PATTERN` /
//! `image_ref`）。当两者出现分歧时，floatctf-content 是最终 source of truth：
//! 先改 Python，再同步这里。
//!
//! 三个概念必须区分（与 floatctf-content README 一致）：
//!
//! | 概念 | 来源 | 用途 |
//! |------|------|------|
//! | `id`（content id） | **目录名** | 稳定身份；safe_name 缺省从此派生 |
//! | `name` | `meta.toml` | 仅 UI 显示名 |
//! | `safe_name` | `meta.toml`（可选） | Docker repository 名 |

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::canonical_combining_class;

/// canonical image 使用的官方 Docker namespace（`content.py::IMAGE_NAMESPACE`）。
pub const CONTENT_IMAGE_NAMESPACE: &str = "floatctf";

/// 制品类型：决定 content 目录名与 canonical image tag 前缀。
///
/// 类型只编码在 **tag**（`challenge-v*` / `gamebox-v*`）里，不再编码进
/// repository path。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    Challenge,
    GameBox,
}

impl ArtifactKind {
    pub const ALL: [ArtifactKind; 2] = [ArtifactKind::Challenge, ArtifactKind::GameBox];

    /// floatctf-content 仓库中的内容目录名（`challenges` / `gameboxes`）。
    pub fn content_dir(self) -> &'static str {
        match self {
            ArtifactKind::Challenge => "challenges",
            ArtifactKind::GameBox => "gameboxes",
        }
    }

    /// `content.py::Content.type`，同时是 canonical image tag 前缀。
    pub fn content_type(self) -> &'static str {
        match self {
            ArtifactKind::Challenge => "challenge",
            ArtifactKind::GameBox => "gamebox",
        }
    }
}

// ---------------------------------------------------------------------------
// safe_name
// ---------------------------------------------------------------------------

/// 官方 `SAFE_NAME_PATTERN`：`^[a-z0-9]+(?:[._-][a-z0-9]+)*$`。
pub fn is_valid_safe_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let is_alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let is_sep = |b: u8| b == b'.' || b == b'_' || b == b'-';

    if !is_alnum(bytes[0]) || !is_alnum(bytes[bytes.len() - 1]) {
        return false;
    }

    let mut prev_sep = false;
    for &b in bytes {
        if is_alnum(b) {
            prev_sep = false;
        } else if is_sep(b) {
            // 分隔符不可连续（`(?:[._-][a-z0-9]+)*` 要求分隔符后必须跟字母数字）
            if prev_sep {
                return false;
            }
            prev_sep = true;
        } else {
            return false;
        }
    }

    true
}

/// 校验显式 `safe_name`（官方规则），错误为人类可读原因。
pub fn validate_safe_name(s: &str) -> Result<(), String> {
    if is_valid_safe_name(s) {
        Ok(())
    } else {
        Err(format!(
            "invalid safe_name '{s}': must match ^[a-z0-9]+(?:[._-][a-z0-9]+)*$"
        ))
    }
}

/// 由 **content id（目录名）** 派生 Docker repository 名。
///
/// 严格复刻 `content.py::derive_safe_name`：
///
/// 1. lowercase
/// 2. Unicode NFKD 归一化
/// 3. 删除 combining marks（`unicodedata.combining(c) != 0`）
/// 4. 删除撇号 `'` 与 `’`
/// 5. `[^a-z0-9._-]+` → `-`
/// 6. `[._-]{2,}` → `-`
/// 7. strip `.` `_` `-`
/// 8. 必须匹配 [`is_valid_safe_name`]
///
/// 失败返回 `None`（调用方须要求显式 `safe_name`）。绝不从 `meta.name` 派生。
pub fn derive_safe_name(content_id: &str) -> Option<String> {
    // 1 + 2 + 3 + 4
    let normalized: String = content_id
        .to_lowercase()
        .nfkd()
        .filter(|c| canonical_combining_class(*c) == 0)
        .collect();
    let deapostrophized = normalized.replace(['\'', '\u{2019}'], "");

    // 5: 非 [a-z0-9._-] 的连续片段 → 单个 '-'
    let mut step5 = String::with_capacity(deapostrophized.len());
    let mut in_replacement = false;
    for ch in deapostrophized.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.' || ch == '_' || ch == '-' {
            step5.push(ch);
            in_replacement = false;
        } else if !in_replacement {
            step5.push('-');
            in_replacement = true;
        }
    }

    // 6: 连续分隔符 [._-]{2,} → 单个 '-'
    let mut step6 = String::with_capacity(step5.len());
    let mut pending: Vec<char> = Vec::new();
    for ch in step5.chars() {
        if ch == '.' || ch == '_' || ch == '-' {
            pending.push(ch);
            continue;
        }
        flush_separators(&mut step6, &mut pending);
        step6.push(ch);
    }
    flush_separators(&mut step6, &mut pending);

    // 7: strip . _ -
    let trimmed = step6.trim_matches(|c| c == '.' || c == '_' || c == '-');

    // 8
    if is_valid_safe_name(trimmed) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn flush_separators(out: &mut String, pending: &mut Vec<char>) {
    match pending.len() {
        0 => {}
        1 => out.push(pending[0]),
        _ => out.push('-'),
    }
    pending.clear();
}

/// 显式 `safe_name` 的可用值（`content.py::explicit_safe_name`）：
/// 非字符串 / 空 / 纯空白视为“不可用”。
pub fn explicit_safe_name(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|s| !s.is_empty())
}

/// `safe_name` 解析失败的原因（与 `content.py::validate_meta` 的两种报错一一对应）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafeNameError {
    /// `safe_name` **字段存在**但不合法（含空串 / 纯空白 —— 此时**不**回退派生）。
    Invalid(String),
    /// `safe_name` 缺失且无法从 content id 派生。
    Underivable,
}

impl SafeNameError {
    pub fn message(&self) -> String {
        match self {
            SafeNameError::Invalid(raw) => format!("invalid safe_name '{raw}'"),
            SafeNameError::Underivable => {
                "unable to derive Docker safe_name; set safe_name explicitly".to_string()
            }
        }
    }
}

/// 官方 safe_name 解析顺序：显式值优先（trim 后校验），否则从 **content id** 派生。
///
/// 注意语义细节：`safe_name` 字段一旦出现就必须自身合法，空串/空白**不会**
/// 回退到派生（与 `content.py` 一致）。
pub fn resolve_safe_name(
    content_id: &str,
    explicit: Option<&str>,
) -> Result<String, SafeNameError> {
    match explicit {
        Some(raw) => match explicit_safe_name(Some(raw)) {
            Some(trimmed) if is_valid_safe_name(trimmed) => Ok(trimmed.to_string()),
            _ => Err(SafeNameError::Invalid(raw.to_string())),
        },
        None => derive_safe_name(content_id).ok_or(SafeNameError::Underivable),
    }
}

// ---------------------------------------------------------------------------
// version
// ---------------------------------------------------------------------------

/// 官方 `VERSION_PATTERN`：`^\d+\.\d+\.\d+$`。
///
/// 只接受 `x.y.z`：拒绝 prerelease（`1.0.0-rc.1`）、build metadata
/// （`1.0.0+build`）、`v1.0.0`、`1.0`。`01.0.0` 仍然合法（与 Python 一致，
/// 不添加额外语义约束）。
pub fn is_valid_version(version: &str) -> bool {
    let mut parts = version.split('.');
    let (Some(major), Some(minor), Some(patch), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };

    [major, minor, patch]
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// 校验版本，失败时返回 Python 风格原因（`expected x.y.z`）。
pub fn validate_version(version: &str) -> Result<(), String> {
    if is_valid_version(version) {
        Ok(())
    } else {
        Err(format!("invalid version '{version}' (expected x.y.z)"))
    }
}

// ---------------------------------------------------------------------------
// canonical image ref
// ---------------------------------------------------------------------------

/// 官方 canonical image ref（`content.py::image_ref`）：
///
/// ```text
/// Challenge: {namespace}/{safe_name}:challenge-v{version}
/// GameBox:   {namespace}/{safe_name}:gamebox-v{version}
/// ```
///
/// `namespace`（registry prefix）来自**平台配置 / CLI 参数**，绝不来自
/// `meta.toml`；官方 namespace 是 [`CONTENT_IMAGE_NAMESPACE`]。
pub fn content_image_ref(
    kind: ArtifactKind,
    registry_prefix: &str,
    safe_name: &str,
    version: &str,
) -> String {
    let prefix = registry_prefix.trim().trim_end_matches('/');
    let prefix = if prefix.is_empty() {
        CONTENT_IMAGE_NAMESPACE
    } else {
        prefix
    };
    format!("{prefix}/{safe_name}:{}-v{version}", kind.content_type())
}

/// 使用官方 namespace（`floatctf`）的 canonical image ref。
pub fn canonical_content_image_ref(kind: ArtifactKind, safe_name: &str, version: &str) -> String {
    content_image_ref(kind, CONTENT_IMAGE_NAMESPACE, safe_name, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_matches_python_examples() {
        for (input, expected) in [
            ("comment", Some("comment")),
            ("Android_reverse", Some("android_reverse")),
            ("FloatCTF-qidong", Some("floatctf-qidong")),
            (
                "Cirno's perfect math class",
                Some("cirnos-perfect-math-class"),
            ),
            ("Cirno\u{2019}s book", Some("cirnos-book")),
            ("foo bar", Some("foo-bar")),
            ("foo   bar", Some("foo-bar")),
            ("foo__bar", Some("foo-bar")),
            ("--Foo..Bar--", Some("foo-bar")),
            ("foo.bar", Some("foo.bar")),
            ("题目", None),
            ("!!!", None),
            ("", None),
        ] {
            assert_eq!(
                derive_safe_name(input).as_deref(),
                expected,
                "derive_safe_name({input:?})"
            );
        }
    }

    #[test]
    fn safe_name_pattern_matches_python() {
        for ok in [
            "a",
            "9x",
            "easy-web-01",
            "foo.bar",
            "android_reverse",
            "a.b-c_d",
        ] {
            assert!(is_valid_safe_name(ok), "{ok} must be valid");
        }
        for bad in [
            "",
            "Easy",
            "-bad",
            "bad-",
            "foo..bar",
            "foo__bar",
            "foo-_bar",
            "a.b.",
            "has space",
            "题目",
        ] {
            assert!(!is_valid_safe_name(bad), "{bad} must be invalid");
        }
    }

    #[test]
    fn resolve_prefers_explicit_and_never_falls_back_on_empty() {
        assert_eq!(
            resolve_safe_name("题目", None),
            Err(SafeNameError::Underivable)
        );
        assert_eq!(
            resolve_safe_name("题目", Some("challenge-001")).unwrap(),
            "challenge-001"
        );
        assert_eq!(
            resolve_safe_name("comment", Some("  custom-name  ")).unwrap(),
            "custom-name"
        );
        // 字段存在但空 → invalid，绝不回退派生
        assert_eq!(
            resolve_safe_name("comment", Some("")),
            Err(SafeNameError::Invalid(String::new()))
        );
        assert_eq!(
            resolve_safe_name("comment", Some("   ")),
            Err(SafeNameError::Invalid("   ".to_string()))
        );
        assert_eq!(
            resolve_safe_name("comment", Some("Foo Bar")),
            Err(SafeNameError::Invalid("Foo Bar".to_string()))
        );
    }

    #[test]
    fn version_pattern_matches_python() {
        for ok in ["1.0.0", "0.0.1", "12.34.56", "01.0.0"] {
            assert!(is_valid_version(ok), "{ok} must be accepted");
        }
        for bad in [
            "1.0",
            "v1.0.0",
            "1.0.0-rc.1",
            "1.0.0+build",
            "abc",
            "",
            "1.0.0.",
        ] {
            assert!(!is_valid_version(bad), "{bad} must be rejected");
        }
    }

    #[test]
    fn canonical_image_refs() {
        assert_eq!(
            canonical_content_image_ref(ArtifactKind::Challenge, "comment", "1.0.0"),
            "floatctf/comment:challenge-v1.0.0"
        );
        assert_eq!(
            canonical_content_image_ref(ArtifactKind::GameBox, "comment", "1.0.0"),
            "floatctf/comment:gamebox-v1.0.0"
        );
        assert_eq!(
            canonical_content_image_ref(ArtifactKind::Challenge, "floatctf-qidong", "1.0.0"),
            "floatctf/floatctf-qidong:challenge-v1.0.0"
        );
        // 平台自定义 prefix 仍然生效
        assert_eq!(
            content_image_ref(
                ArtifactKind::GameBox,
                "registry.example.com:5000/",
                "ttt1",
                "2.1.0"
            ),
            "registry.example.com:5000/ttt1:gamebox-v2.1.0"
        );
    }
}
