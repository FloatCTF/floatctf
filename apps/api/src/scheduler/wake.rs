//! 定时任务 Redis 即时唤醒。
//!
//! 机制：任务排入/重排/手动触发点调用 [`notify_scheduled`]，向
//! `floatctf:scheduler:wake` 频道 PUBLISH 一条轻量消息；引擎侧
//! [`spawn_wake_listener`] 订阅该频道，收到消息立即执行一次
//! `TaskScheduler::poll_once`（底层 `FOR UPDATE SKIP LOCKED`，
//! 与 5s 轮询及多节点并发安全）。
//!
//! 收益：近实时任务（管理员 Run once / 即刻开赛排程等）从
//! 「最坏 5s 后才被轮询发现」变为毫秒级触发；远期任务仍由轮询
//! 提前 5s 锁定 + 精准睡眠执行，原路径不变。
//!
//! Redis 是启动必需依赖；运行期短暂故障时订阅器自动重连，引擎保留 5s DB 轮询兜底。

/// 唤醒频道（与 realtime event channel 独立；内部实现细节，无需配置）。
pub const SCHEDULER_WAKE_CHANNEL: &str = "floatctf:scheduler:wake";

static CLIENT: std::sync::OnceLock<::redis::Client> = std::sync::OnceLock::new();

/// 注册 bootstrap 已验证的 Redis client。
pub fn configure(client: ::redis::Client) {
    let _ = CLIENT.set(client);
    tracing::info!(
        channel = SCHEDULER_WAKE_CHANNEL,
        "定时任务 Redis 即时唤醒已配置"
    );
}

/// 「有任务刚排入/重排」通知：fire-and-forget，绝不阻塞、绝不向调用方报错。
///
/// 在事务内调用也是安全的：唤醒触发的拉取看不到未提交行时等价于一次空拉取，
/// 任务仍会被 5s DB 轮询兜底捕获。
pub fn notify_scheduled() {
    let Some(client) = CLIENT.get().cloned() else {
        // 只允许隔离测试触发；正常 API bootstrap 必定先 configure。
        return;
    };
    tokio::spawn(async move {
        let op = async {
            use ::redis::AsyncCommands;
            let mut conn = client.get_multiplexed_async_connection().await?;
            let _: () = conn.publish(SCHEDULER_WAKE_CHANNEL, "wake").await?;
            Ok::<(), ::redis::RedisError>(())
        };
        if let Err(error) = op.await {
            tracing::debug!(%error, "[Scheduler] wake 发布失败（由 5s DB 轮询兜底）");
        }
    });
}

/// 启动唤醒订阅者。Redis 是部署必需基础设施；启动后的短暂故障会自动重连，
/// 同时保留 5s DB 轮询维持任务正确性。
///
/// 必须用 `actix_web::rt::spawn`（LocalSet）而非 `tokio::spawn`：引擎 dispatch /
/// LogService 内部存在 `spawn_local` 路径。
pub fn spawn_wake_listener(engine: std::sync::Arc<crate::scheduler::engine::TaskScheduler>) {
    let Some(client) = CLIENT.get().cloned() else {
        tracing::error!("[Scheduler] Redis client 未由 bootstrap 配置；wake listener 未启动");
        return;
    };
    actix_web::rt::spawn(async move {
        loop {
            let attempt = futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(
                run_wake_loop(&client, &engine),
            ))
            .await;
            match attempt {
                Ok(Ok(())) => tracing::warn!("[Scheduler] wake 订阅结束，2s 后重连"),
                Ok(Err(error)) => {
                    tracing::error!(%error, "[Scheduler] wake 订阅错误，2s 后重连")
                }
                Err(panic) => {
                    tracing::error!(panic = ?panic, "[Scheduler] wake 循环 panic，2s 后重连")
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });
    tracing::info!(
        channel = SCHEDULER_WAKE_CHANNEL,
        "[Scheduler] Redis wake 监听已启动（5s DB 轮询作为故障兜底）"
    );
}

/// 订阅循环：每条消息触发一次立即拉取。
async fn run_wake_loop(
    client: &::redis::Client,
    engine: &std::sync::Arc<crate::scheduler::engine::TaskScheduler>,
) -> anyhow::Result<()> {
    use futures_util::StreamExt;

    let mut pubsub = client.get_async_pubsub().await?;
    pubsub.subscribe(SCHEDULER_WAKE_CHANNEL).await?;
    let mut stream = pubsub.on_message();

    while stream.next().await.is_some() {
        if let Err(error) = engine.poll_once().await {
            tracing::warn!(%error, "[Scheduler] wake 触发拉取失败（由 5s DB 轮询兜底）");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 publisher / ratelimit 测试同一约定：未设置 TEST_REDIS_URL 时跳过。
    fn redis_url() -> Option<String> {
        match std::env::var("TEST_REDIS_URL") {
            Ok(url) if !url.trim().is_empty() => Some(url),
            _ => {
                eprintln!("skip: TEST_REDIS_URL not set (scheduler wake tests)");
                None
            }
        }
    }

    /// notify → 订阅端收到的端到端验证（真实 Redis，配置进程级 OnceLock）。
    #[tokio::test]
    async fn notify_scheduled_publishes_wake_message() {
        let Some(url) = redis_url() else { return };
        // 独立客户端订阅（订阅连接与发布连接必须分离）。
        let subscriber = ::redis::Client::open(url.as_str()).unwrap();
        let mut pubsub = subscriber.get_async_pubsub().await.unwrap();
        pubsub.subscribe(SCHEDULER_WAKE_CHANNEL).await.unwrap();

        // 配置进程级客户端（幂等；首次生效）后发布。
        configure(::redis::Client::open(url.as_str()).unwrap());
        notify_scheduled();

        use futures_util::StreamExt;
        let received = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            pubsub.on_message().next(),
        )
        .await
        .expect("3s 内应收到唤醒消息");
        assert!(received.is_some(), "唤醒消息不应为空");
    }

    /// 未配置客户端时 notify 必须是无副作用的 no-op（不 panic、不阻塞）。
    #[tokio::test]
    async fn notify_without_configure_is_noop() {
        // 不调用 configure —— CLIENT 可能为 None（或被其他用例配置过，
        // 此时也只应静默发布/跳过），任何路径都不允许 panic。
        notify_scheduled();
    }
}
