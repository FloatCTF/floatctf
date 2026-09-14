//! 实例级 PostgreSQL advisory lock（跨进程互斥，plan §23/§43）。
//!
//! Patch / Official evaluation / Reset / Manual check 全部使用同一把
//! instance-scoped session lock（`pg_advisory_lock`），避免：
//!   - Round N cutoff 已过、玩家在评估期间改容器；
//!   - 评估与 reset 并发 recreate 竞态。
//!
//! 锁绑定到独占连接；显式 `release()` 或连接关闭都会解锁。

use sea_orm::sqlx::{Acquire, Postgres, pool::PoolConnection};
use uuid::Uuid;

use sea_orm::DatabaseConnection;

use crate::modules::event::awdp::{AwdpError, AwdpResult};

// 锁 key 用 Postgres hashtextextended 派生（int8，稳定跨进程/版本）。
const LOCK_SALT: &str = "floatctf-awdp-instance";

/// 持有的实例锁（Drop 不保证解锁——显式 release；连接关闭自动释放）。
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
        sea_orm::sqlx::query("SELECT pg_advisory_lock(hashtextextended($1::text, 0))")
            .bind(&lock_input)
            .execute(&mut *conn)
            .await
            .map_err(|e| AwdpError::Database(format!("pg_advisory_lock: {e}")))?;
        Ok(InstanceAdvisoryLock {
            conn: Some(conn),
            key: lock_input,
        })
    }

    pub async fn release(mut self) {
        if let Some(mut conn) = self.conn.take() {
            let key = self.key.clone();
            let _ =
                sea_orm::sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1::text, 0))")
                    .bind(key)
                    .execute(&mut *conn)
                    .await;
        }
    }
}

impl Drop for InstanceAdvisoryLock {
    fn drop(&mut self) {
        // 连接归还/关闭时 advisory lock 自动释放；无需异步解锁。
        self.conn = None;
    }
}
