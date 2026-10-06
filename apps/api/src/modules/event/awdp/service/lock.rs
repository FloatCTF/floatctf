//! 实例级 PostgreSQL advisory lock（跨进程互斥，plan §23/§43）。
//!
//! Patch / Official evaluation / Reset / Manual check / ALL Check 全部使用同一把
//! instance-scoped session lock（`pg_try_advisory_lock`），避免：
//!   - Round N cutoff 已过、玩家在评估期间改容器；
//!   - 评估与 reset 并发 recreate 竞态。
//!
//! 等待有界：判定（healthcheck→judge→exploit）本身可能跑几十秒，但**绝不能无限等**。
//! 超过 [`LOCK_WAIT_TIMEOUT_SECS`] 未取得 → 明确返回 409 冲突错误，让前端显示
//! 「有判定正在进行」，而不是按钮一直停在 Checking…。
//!
//! 锁绑定到独占连接；显式 `release()` 解锁。`Drop` 走后台任务补解锁——**不能**把
//! 仍持有 session 级锁的连接直接还回连接池（那把锁会留在池里的连接上，后续任何
//! acquire 都会永久阻塞）。

use std::time::Duration;

use sea_orm::sqlx::{Acquire, Postgres, pool::PoolConnection};
use uuid::Uuid;

use sea_orm::DatabaseConnection;

use crate::modules::event::awdp::{AwdpError, AwdpResult};

// 锁 key 用 Postgres hashtextextended 派生（int8，稳定跨进程/版本）。
const LOCK_SALT: &str = "floatctf-awdp-instance";

/// 获取实例锁的最长等待时间（秒）。判定管线本身可能占用数十秒，故留足余量；
/// 超过即视为「另一个判定卡住/异常」并快速失败。
pub const LOCK_WAIT_TIMEOUT_SECS: u64 = 90;

/// 未取得锁时的轮询间隔。
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// 持有的实例锁（Drop 由后台任务补解锁；连接关闭亦会释放）。
pub struct InstanceAdvisoryLock {
    conn: Option<PoolConnection<Postgres>>,
    key: String,
}

impl InstanceAdvisoryLock {
    pub async fn acquire(
        db: &DatabaseConnection,
        instance_id: Uuid,
    ) -> AwdpResult<InstanceAdvisoryLock> {
        let pool = db.get_postgres_connection_pool();
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| AwdpError::Database(format!("acquire conn: {e}")))?;
        let lock_input = format!("{LOCK_SALT}:{instance_id}");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(LOCK_WAIT_TIMEOUT_SECS);
        loop {
            let acquired: bool = sea_orm::sqlx::query_scalar(
                "SELECT pg_try_advisory_lock(hashtextextended($1::text, 0))",
            )
            .bind(&lock_input)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| AwdpError::Database(format!("pg_try_advisory_lock: {e}")))?;
            if acquired {
                return Ok(InstanceAdvisoryLock {
                    conn: Some(conn),
                    key: lock_input,
                });
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(AwdpError::Conflict(format!(
                    "该实例已有判定在进行（等待 {LOCK_WAIT_TIMEOUT_SECS}s 未获得锁），请稍后重试"
                )));
            }
            tokio::time::sleep(LOCK_POLL_INTERVAL).await;
        }
    }

    pub async fn release(mut self) {
        if let Some(mut conn) = self.conn.take() {
            let key = self.key.clone();
            if let Err(e) =
                sea_orm::sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1::text, 0))")
                    .bind(&key)
                    .execute(&mut *conn)
                    .await
            {
                // 解锁失败 → 该连接不能还回池（上面还带着锁），直接关闭会话。
                tracing::error!(error = %e, lock_key = %key, "[AWDP] advisory unlock 执行失败，关闭该连接");
                let _ = conn.close().await;
            }
        }
    }
}

impl Drop for InstanceAdvisoryLock {
    fn drop(&mut self) {
        // 走到这里说明调用方没有显式 release（提前 return / panic / 被取消）。
        // 绝不能直接把连接还回池：session 级 advisory lock 会留在那连接上，后续
        // 任何 acquire 都将永不到手。交给后台任务显式解锁。
        if let Some(mut conn) = self.conn.take() {
            let key = self.key.clone();
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    handle.spawn(async move {
                        if let Err(e) = sea_orm::sqlx::query(
                            "SELECT pg_advisory_unlock(hashtextextended($1::text, 0))",
                        )
                        .bind(&key)
                        .execute(&mut *conn)
                        .await
                        {
                            tracing::error!(
                                error = %e,
                                lock_key = %key,
                                "[AWDP] advisory lock 在 Drop 路径解锁失败"
                            );
                        }
                    });
                }
                Err(_) => {
                    tracing::error!(
                        lock_key = %key,
                        "[AWDP] 无 tokio 运行时可用，advisory lock 未能显式解锁"
                    );
                }
            }
        }
    }
}
