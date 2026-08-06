//! Challenge metadata helpers (safe names, meta.toml validation helpers).

/// Sanitize a challenge display name into a filesystem-safe directory name.
///
/// Keeps ASCII alphanumerics, space, `.`, `_`, `-`; replaces other ASCII with `_`.
/// Non-ASCII characters (CJK, emoji, …) are preserved.
pub fn generate_safe_name(original: &str) -> String {
    original
        .chars()
        .map(|c| {
            if c.is_ascii() {
                // 保留 ASCII 字母、数字、空格、点号、下划线、连字符
                if c.is_ascii_alphanumeric() || c == ' ' || c == '.' || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            } else {
                // 中文、日文、韩文、emoji 等非 ASCII 字符不处理
                c
            }
        })
        .collect()
}
