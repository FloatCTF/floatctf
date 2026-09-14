//! 进程级共享 Redis 基础设施。
//!
//! Redis 是 FloatCTF API 的必需依赖。URL 统一来自 TOML `[redis].url`；bootstrap
//! 会在启动阶段建立连接并执行 PING，失败则 API fail-fast，不进入 HTTP serving。
//!
//! 当前业务用途：realtime pub/sub + 全局 sequence、AWD 分布式限流、Web Terminal
//! 一次性 ticket、scheduler 即时唤醒、settings 热点缓存。部分用途在“启动后 Redis
//! 短暂故障”时仍有 DB/本地连续性路径，但这不代表 Redis 是可选部署组件。

use anyhow::{Context, Result, bail};

use crate::core::config::RedisConfig;

static CLIENT: std::sync::OnceLock<::redis::Client> = std::sync::OnceLock::new();

/// 建立并验证 Redis 连接。Redis 是启动门禁，因此这里必须真实 PING。
pub async fn connect(config: &RedisConfig) -> Result<::redis::Client> {
    let client =
        ::redis::Client::open(config.url.expose()).context("invalid Redis URL in [redis].url")?;
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .context("connect Redis")?;
    let pong: String = ::redis::cmd("PING")
        .query_async(&mut conn)
        .await
        .context("PING Redis")?;
    if pong != "PONG" {
        bail!("unexpected Redis PING response: {pong:?}");
    }
    tracing::info!("Redis connected OK");
    Ok(client)
}

/// 注册 bootstrap 已验证的共享客户端。重复注册保持第一次配置。
pub fn configure(client: ::redis::Client) {
    let _ = CLIENT.set(client);
}

/// 获取 bootstrap 注册的共享客户端。
///
/// 正常 API 运行态一定为 `Some`；`None` 仅可能出现在不经过 bootstrap 的隔离单元测试。
pub fn client() -> Option<&'static ::redis::Client> {
    CLIENT.get()
}
