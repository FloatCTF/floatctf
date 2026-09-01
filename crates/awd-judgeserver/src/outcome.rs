//! Judge 执行结果映射（纯函数，可单测）。
//!
//! # 退出码契约
//!
//! | 退出码 | 语义 | 映射 outcome |
//! |--------|------|-------------|
//! | 0 | 服务健康 | `up` |
//! | 1 | 服务故障（连接拒绝/HTTP 500/错误输出等） | `down` |
//! | >1 | 非标准退出（配置错误/脚本 bug 等） | `worker_error` |
//!
//! 非退出码场景：
//! - tokio timeout 触发 → `target_timeout`
//! - 子进程 spawn 失败 → `worker_error`
//! - 脚本文件写入失败 → `worker_error`

/// 将子进程退出码映射为 outcome 字符串。
///
/// 纯函数，无副作用。
pub fn exit_code_to_outcome(exit_code: i32) -> &'static str {
    match exit_code {
        0 => "up",
        1 => "down",
        _ => "worker_error",
    }
}

/// 将子进程退出码映射为 outcome 字符串（适用于 `Option<i32>`，-1 表示无退出码）。
pub fn optional_exit_code_to_outcome(exit_code: Option<i32>) -> &'static str {
    match exit_code {
        Some(0) => "up",
        Some(1) => "down",
        _ => "worker_error",
    }
}

/// Judge 脚本执行的最小 env 白名单。
///
/// 只保留 `PATH` / `HOME` / `LANG` / `JUDGE_*`，其余全部清除。
pub fn build_script_env(host_env: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    host_env
        .filter(|(key, _)| {
            key == "PATH" || key == "HOME" || key == "LANG" || key.starts_with("JUDGE_")
        })
        .collect()
}

/// 截断字符串到 max_len 字节，超出时附加截断提示。
pub fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}... (truncated, {} bytes total)", &s[..max_len], s.len())
    }
}

/// 最大 stdout/stderr 截断长度。
pub const OUTPUT_MAX_LEN: usize = 4096;

#[cfg(test)]
mod tests {
    use super::*;

    // ── Outcome mapping ──

    #[test]
    fn exit_0_maps_to_up() {
        assert_eq!(exit_code_to_outcome(0), "up");
    }

    #[test]
    fn exit_1_maps_to_down() {
        assert_eq!(exit_code_to_outcome(1), "down");
    }

    #[test]
    fn exit_other_maps_to_worker_error() {
        assert_eq!(exit_code_to_outcome(2), "worker_error");
        assert_eq!(exit_code_to_outcome(127), "worker_error");
        assert_eq!(exit_code_to_outcome(-1), "worker_error");
        assert_eq!(exit_code_to_outcome(255), "worker_error");
    }

    #[test]
    fn optional_none_maps_to_worker_error() {
        assert_eq!(optional_exit_code_to_outcome(None), "worker_error");
    }

    #[test]
    fn optional_exit_0_maps_to_up() {
        assert_eq!(optional_exit_code_to_outcome(Some(0)), "up");
    }

    #[test]
    fn optional_exit_1_maps_to_down() {
        assert_eq!(optional_exit_code_to_outcome(Some(1)), "down");
    }

    // ── Script env ──

    #[test]
    fn script_env_allowlist_only_keeps_whitelisted_vars() {
        let host_env = vec![
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            ("HOME".to_string(), "/root".to_string()),
            ("LANG".to_string(), "C.UTF-8".to_string()),
            ("JUDGE_TIMEOUT_FACTOR".to_string(), "3".to_string()),
            ("INTERNAL_TOKEN".to_string(), "should-not-leak".to_string()),
            (
                "PLATFORM_INTERNAL_URL".to_string(),
                "http://127.0.0.1".to_string(),
            ),
            ("RUST_LOG".to_string(), "debug".to_string()),
        ];

        let allowlist = build_script_env(host_env.into_iter());
        let keys: Vec<&str> = allowlist.iter().map(|(k, _)| k.as_str()).collect();

        assert!(keys.contains(&"PATH"));
        assert!(keys.contains(&"HOME"));
        assert!(keys.contains(&"LANG"));
        assert!(keys.contains(&"JUDGE_TIMEOUT_FACTOR"));
        assert!(!keys.contains(&"INTERNAL_TOKEN"));
        assert!(!keys.contains(&"PLATFORM_INTERNAL_URL"));
        assert!(!keys.contains(&"RUST_LOG"));
        assert_eq!(allowlist.len(), 4);
    }

    // ── Truncation ──

    #[test]
    fn truncate_short_string_unchanged() {
        let s = "hello";
        assert_eq!(truncate_str(s, 10), "hello");
    }

    #[test]
    fn truncate_at_boundary_unchanged() {
        let s = "a".repeat(4096);
        assert_eq!(truncate_str(&s, 4096), s);
    }

    #[test]
    fn truncate_long_string_adds_truncation_note() {
        let s = "a".repeat(5000);
        let result = truncate_str(&s, 4096);
        assert!(result.starts_with(&"a".repeat(4096)));
        assert!(result.contains("truncated"));
        assert!(result.contains("5000 bytes total"));
    }

    #[test]
    fn truncate_empty_string_unchanged() {
        assert_eq!(truncate_str("", 4096), "");
    }
}
