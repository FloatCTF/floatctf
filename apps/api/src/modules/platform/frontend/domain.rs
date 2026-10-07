//! 前端标识的纯逻辑校验（无 I/O，可单测）。
//!
//! 与 `packages/frontend-runtime` 的 `isSafeFrontendId` **同一契约**：
//! 小写字母/数字开头，随后 `[a-z0-9._-]`，最长 64 字符。
//!
//! 为什么后端也要校验：`FRONTEND_ACTIVE` 是管理员可编辑的动态设置，而它会出现在
//! **未认证**的 `GET /api/frontend` 响应里。后端绝不把未经校验的设置值原样回显 ——
//! 非法值一律回落 `default`，避免把一个被写坏的值当成"前端 ID"传播出去。

/// 前端 ID 最大长度（同时是注册表目录名上限）。
pub const FRONTEND_ID_MAX_LENGTH: usize = 64;

/// 平台内置、始终可用的前端 ID。
pub const DEFAULT_FRONTEND_ID: &str = "default";

/// 校验前端 ID 是否满足受限形式。
pub fn is_safe_frontend_id(value: &str) -> bool {
    if value.is_empty() || value.len() > FRONTEND_ID_MAX_LENGTH {
        return false;
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

/// 把任意设置值归一化为可安全公开的前端 ID：非法/缺失一律取 `default`。
pub fn normalize_frontend_id(value: Option<&str>) -> String {
    match value {
        Some(v) if is_safe_frontend_id(v) => v.to_string(),
        _ => DEFAULT_FRONTEND_ID.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_well_formed_ids() {
        for id in ["default", "cyberpunk", "my.frontend_2", "a", "0abc"] {
            assert!(is_safe_frontend_id(id), "should accept {id}");
        }
    }

    #[test]
    fn rejects_unsafe_ids() {
        for id in [
            "",
            "Default",
            "../evil",
            "a/b",
            "-leading",
            ".leading",
            "with space",
            "路径",
            "a\\b",
        ] {
            assert!(!is_safe_frontend_id(id), "should reject {id:?}");
        }
        let too_long = "a".repeat(FRONTEND_ID_MAX_LENGTH + 1);
        assert!(!is_safe_frontend_id(&too_long));
    }

    #[test]
    fn normalizes_unsafe_or_missing_values_to_default() {
        assert_eq!(normalize_frontend_id(Some("cyberpunk")), "cyberpunk");
        assert_eq!(normalize_frontend_id(Some("../../etc/passwd")), "default");
        assert_eq!(normalize_frontend_id(Some("")), "default");
        assert_eq!(normalize_frontend_id(None), "default");
    }
}
