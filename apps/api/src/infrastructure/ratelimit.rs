//! Redis 分布式滑动窗口限流（AWD）。
//!
//! Redis 是 FloatCTF API 的必需基础设施。限流使用 Redis ZSET + Lua 原子脚本，
//! 所有 API 节点共享同一配额；Redis 运行期故障时 fail-closed，避免退化成各节点
//! 独立计数导致绕过。
//!
//! 限流配置走 settings 表：
//! - `AWD_RATE_SUBMIT_PER_MIN`（默认 30）：提交 flag（每用户）
//! - `AWD_RATE_RESET_PER_HOUR`（默认 5）：重置 GameBox（每队伍）
//! - `AWD_RATE_INTERNAL_PER_MIN`（默认 120）：internal 端点（每 event）

use crate::modules::event::awd::AwdResult;

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

/// Redis-backed 滑动窗口限流器。
pub struct RateLimiter {
    redis: redis::Client,
}

impl RateLimiter {
    pub fn new(redis: redis::Client) -> Self {
        Self { redis }
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

        self.check_redis(scope, key, limit, window_secs).await
    }

    async fn check_redis(
        &self,
        scope: RateScope,
        key: &str,
        limit: u32,
        window_secs: u64,
    ) -> AwdResult<()> {
        // 单条 Lua 脚本完成 prune/count/add/expire，保证跨进程原子滑动窗口语义。
        const SCRIPT: &str = r#"
local key = KEYS[1]
local now = tonumber(ARGV[1])
local cutoff = tonumber(ARGV[2])
local limit = tonumber(ARGV[3])
local member = ARGV[4]
local ttl = tonumber(ARGV[5])
redis.call('ZREMRANGEBYSCORE', key, '-inf', cutoff)
local count = redis.call('ZCARD', key)
if count >= limit then
  return 0
end
redis.call('ZADD', key, now, member)
redis.call('PEXPIRE', key, ttl)
return 1
"#;

        let now_ms = chrono::Utc::now().timestamp_millis();
        let window_ms = (window_secs as i64).saturating_mul(1000);
        let cutoff_ms = now_ms.saturating_sub(window_ms);
        let redis_key = format!("floatctf:ratelimit:{scope:?}:{key}");
        let member = uuid::Uuid::new_v4().to_string();
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| {
                crate::modules::event::awd::AwdError::Internal(format!(
                    "distributed rate limiter unavailable: {e}"
                ))
            })?;
        let allowed: i64 = redis::cmd("EVAL")
            .arg(SCRIPT)
            .arg(1)
            .arg(redis_key)
            .arg(now_ms)
            .arg(cutoff_ms)
            .arg(limit)
            .arg(member)
            .arg(window_ms)
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                crate::modules::event::awd::AwdError::Internal(format!(
                    "distributed rate limiter failed: {e}"
                ))
            })?;

        if allowed == 1 {
            Ok(())
        } else {
            Err(crate::modules::event::awd::AwdError::Forbidden(format!(
                "rate limit exceeded for {scope:?} ({} per {}s)",
                limit, window_secs
            )))
        }
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

#[cfg(test)]
mod redis_tests {
    use super::*;

    /// 测试文件级串行：共享同一 Redis DB。
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn redis_url() -> Option<String> {
        match std::env::var("TEST_REDIS_URL") {
            Ok(url) if !url.trim().is_empty() => Some(url),
            _ => {
                eprintln!("skip: TEST_REDIS_URL not set (ratelimit redis tests)");
                None
            }
        }
    }

    async fn test_db() -> Option<sea_orm::DatabaseConnection> {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into());
        match sea_orm::Database::connect(&url).await {
            Ok(db) => Some(db),
            Err(e) => {
                eprintln!("skip: DB unreachable ({e})");
                None
            }
        }
    }

    async fn flush(client: &redis::Client) {
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .expect("conn");
        let _: () = redis::cmd("FLUSHDB")
            .query_async(&mut conn)
            .await
            .expect("flush");
    }

    /// 单节点限流：limit=10 时第 11 次拒绝；不同 scope / 不同 key 独立。
    #[tokio::test]
    async fn redis_limiter_enforces_shared_window() {
        let _serial = SERIAL.lock().unwrap();
        let Some(url) = redis_url() else { return };
        let Some(db) = test_db().await else { return };
        let client = redis::Client::open(url.as_str()).unwrap();
        let limiter = RateLimiter::new(client.clone());
        flush(&client).await;

        // 设置低配额便于验证（settings 表）
        crate::infrastructure::settings::upsert_setting(&db, "AWD_RATE_RESET_PER_HOUR", "10")
            .await
            .unwrap();

        let key = format!("rl-team-{}", uuid::Uuid::new_v4());
        for i in 0..10 {
            limiter
                .check(&db, RateScope::Reset, &key)
                .await
                .unwrap_or_else(|e| panic!("attempt {i} must pass: {e:?}"));
        }
        let denied = limiter.check(&db, RateScope::Reset, &key).await;
        assert!(denied.is_err(), "第 11 次必须被拒绝");

        // 不同 key 独立
        limiter
            .check(&db, RateScope::Reset, &format!("{key}-other"))
            .await
            .expect("不同 key 必须独立计数");

        // 不同 scope 独立（Submit 默认 30）
        limiter
            .check(&db, RateScope::Submit, &key)
            .await
            .expect("不同 scope 必须独立计数");

        crate::infrastructure::settings::upsert_setting(&db, "AWD_RATE_RESET_PER_HOUR", "5")
            .await
            .unwrap();
    }

    /// 双实例共享配额：节点 A 用 5 次、节点 B 用 5 次（共 10）后，两边都拒绝。
    #[tokio::test]
    async fn redis_limiter_shared_across_instances() {
        let _serial = SERIAL.lock().unwrap();
        let Some(url) = redis_url() else { return };
        let Some(db) = test_db().await else { return };
        let client = redis::Client::open(url.as_str()).unwrap();
        let node_a = RateLimiter::new(client.clone());
        let node_b = RateLimiter::new(client.clone());
        flush(&client).await;

        crate::infrastructure::settings::upsert_setting(&db, "AWD_RATE_RESET_PER_HOUR", "10")
            .await
            .unwrap();

        let key = format!("rl-x-node-{}", uuid::Uuid::new_v4());
        for _ in 0..5 {
            node_a.check(&db, RateScope::Reset, &key).await.expect("A");
        }
        for _ in 0..5 {
            node_b.check(&db, RateScope::Reset, &key).await.expect("B");
        }
        // 共享配额耗尽：两边都拒绝
        assert!(node_a.check(&db, RateScope::Reset, &key).await.is_err());
        assert!(node_b.check(&db, RateScope::Reset, &key).await.is_err());

        crate::infrastructure::settings::upsert_setting(&db, "AWD_RATE_RESET_PER_HOUR", "5")
            .await
            .unwrap();
    }

    /// 100 并发、limit=10：恰好 10 个成功。
    #[tokio::test]
    async fn redis_limiter_concurrent_exactly_limit_allowed() {
        let _serial = SERIAL.lock().unwrap();
        let Some(url) = redis_url() else { return };
        let Some(db) = test_db().await else { return };
        let client = redis::Client::open(url.as_str()).unwrap();
        flush(&client).await;

        crate::infrastructure::settings::upsert_setting(&db, "AWD_RATE_RESET_PER_HOUR", "10")
            .await
            .unwrap();

        let limiter = std::sync::Arc::new(RateLimiter::new(client.clone()));
        let key = format!("rl-c-{}", uuid::Uuid::new_v4());
        let mut handles = Vec::new();
        for _ in 0..100 {
            let l = limiter.clone();
            let db = db.clone();
            let key = key.clone();
            handles.push(tokio::spawn(async move {
                l.check(&db, RateScope::Reset, &key).await.is_ok()
            }));
        }
        let mut allowed = 0usize;
        for h in handles {
            if h.await.expect("join") {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 10, "100 并发下恰好 10 个成功（原子性）");

        crate::infrastructure::settings::upsert_setting(&db, "AWD_RATE_RESET_PER_HOUR", "5")
            .await
            .unwrap();
    }

    /// Redis 故障 → fail-closed（Internal 错误，不静默放行）。
    #[tokio::test]
    async fn redis_limiter_outage_fails_closed() {
        let _serial = SERIAL.lock().unwrap();
        let Some(db) = test_db().await else { return };
        let limiter = RateLimiter::new(
            redis::Client::open("redis://127.0.0.1:6390/").expect("valid dead Redis URL"),
        );
        let err = limiter
            .check(&db, RateScope::Submit, "any-user")
            .await
            .expect_err("Redis 不可达必须报错");
        assert!(
            matches!(err, crate::modules::event::awd::AwdError::Internal(_)),
            "fail-closed 应为 Internal 错误，实际 {err:?}"
        );
    }
}
