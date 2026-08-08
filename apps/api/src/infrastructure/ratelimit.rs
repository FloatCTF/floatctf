//! 轻量进程内滑动窗口限流（Phase 5 P5-10）。
//!
//! 单 API 进程场景够用（scheduler 与 HTTP 共享进程）。多实例部署需换共享存储
//! （Redis/DB），此处接口预留 scope 维度便于未来替换。
//!
//! 限流配置走 settings 表（AGENTS.md 铁律 1）：
//! - `AWD_RATE_SUBMIT_PER_MIN`（默认 30）：提交 flag（每用户）
//! - `AWD_RATE_RESET_PER_HOUR`（默认 5）：重置 GameBox（每队伍）
//! - `AWD_RATE_INTERNAL_PER_MIN`（默认 120）：internal 端点（每 event）

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::modules::event::awd_team::AwdResult;

/// 限流 scope 类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateScope {
    Submit,
    Reset,
    Internal,
}

impl RateScope {
    fn settings_key(&self) -> (&'static str, u32, u64) {
        match self {
            RateScope::Submit => ("AWD_RATE_SUBMIT_PER_MIN", 30, 60),
            RateScope::Reset => ("AWD_RATE_RESET_PER_HOUR", 5, 3600),
            RateScope::Internal => ("AWD_RATE_INTERNAL_PER_MIN", 120, 60),
        }
    }
}

#[derive(Default)]
struct Bucket {
    timestamps: Vec<Instant>,
}

/// 进程内滑动窗口限流器。
pub struct RateLimiter {
    inner: Mutex<HashMap<(RateScope, String), Bucket>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// 检查并记录一次访问。超限返回错误。
    ///
    /// `key` 语义：Submit=user_id，Reset=team_id，Internal=event_id。
    pub async fn check(
        &self,
        db: &sea_orm::DatabaseConnection,
        scope: RateScope,
        key: &str,
    ) -> AwdResult<()> {
        let (settings_key, default_limit, window_secs) = scope.settings_key();
        let limit = crate::infrastructure::settings::get_setting(db, settings_key)
            .await
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(default_limit);

        let now = Instant::now();
        let window = std::time::Duration::from_secs(window_secs);
        let mut map = self.inner.lock().unwrap();
        let bucket = map.entry((scope, key.to_string())).or_default();
        bucket
            .timestamps
            .retain(|t| now.duration_since(*t) < window);
        if bucket.timestamps.len() >= limit as usize {
            return Err(crate::modules::event::awd_team::AwdError::Forbidden(
                format!(
                    "rate limit exceeded for {scope:?} ({} per {}s)",
                    limit, window_secs
                ),
            ));
        }
        bucket.timestamps.push(now);
        Ok(())
    }

    /// 清理（测试用）。
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_settings_have_sane_defaults() {
        assert_eq!(RateScope::Submit.settings_key().1, 30);
        assert_eq!(RateScope::Reset.settings_key().1, 5);
        assert_eq!(RateScope::Internal.settings_key().1, 120);
    }
}
