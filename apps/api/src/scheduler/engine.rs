use crate::entity::scheduled_tasks;
use crate::infrastructure::logging::LogService;
use crate::infrastructure::{WebDb, WebDocker, WebRustfs};
use crate::scheduler::TaskKey;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel,
    QueryFilter, Statement,
};
use serde_json::json;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use uuid::Uuid;

#[async_trait]
pub trait TaskHandler: Send + Sync {
    fn task_key(&self) -> TaskKey;
    fn trigger_type(&self) -> &'static str;
    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()>;
}

#[derive(Clone, Default)]
pub struct TaskRegistry {
    handlers: HashMap<TaskKey, Arc<dyn TaskHandler>>,
}

impl TaskRegistry {
    pub fn register(&mut self, handler: Arc<dyn TaskHandler>) -> Result<()> {
        let key = handler.task_key();
        if self.handlers.contains_key(&key) {
            return Err(anyhow!("duplicate scheduled task handler: {key}"));
        }
        self.handlers.insert(key, handler);
        Ok(())
    }

    pub fn get(&self, key: TaskKey) -> Option<&Arc<dyn TaskHandler>> {
        self.handlers.get(&key)
    }

    pub fn contains(&self, key: TaskKey) -> bool {
        self.handlers.contains_key(&key)
    }
}

pub struct TaskScheduler {
    db: WebDb,
    docker: WebDocker,
    rustfs: WebRustfs,
    logger: LogService,
    registry: TaskRegistry,
}

impl TaskScheduler {
    pub fn new(db: WebDb, docker: WebDocker, rustfs: WebRustfs, logger: LogService) -> Self {
        Self {
            db,
            docker,
            logger,
            rustfs,
            registry: TaskRegistry::default(),
        }
    }

    pub fn register_handler(&mut self, handler: Arc<dyn TaskHandler>) -> Result<()> {
        self.registry.register(handler)
    }

    pub async fn start_polling(self: Arc<Self>) {
        let mut interval = actix_web::rt::time::interval(Duration::from_secs(5));

        loop {
            interval.tick().await;
            if let Err(e) = self.fetch_and_run().await {
                error!("[Scheduler] 执行任务时出错: {}", e);
            }
        }
    }

    async fn fetch_and_run(&self) -> Result<()> {
        // 这里的 SQL 变化：NOW() + INTERVAL '5 seconds'
        // 提前把未来 5 秒内要执行的任务全部锁住并取出来
        // 只取出is_enabled=true的任务
        let sql = r#"
                UPDATE scheduled_tasks
                SET status = 'running',
                    updated_at = NOW(),
                    locked_at = NOW(),
                    heartbeat_at = NOW()
                WHERE id IN (
                    SELECT id FROM scheduled_tasks
                    WHERE status = 'pending' AND enabled = true
                      AND execute_at <= NOW() + INTERVAL '5 seconds'
                    ORDER BY execute_at ASC
                    FOR UPDATE SKIP LOCKED LIMIT 20
                )
                RETURNING *;
            "#;

        let tasks = scheduled_tasks::Entity::find()
            .from_raw_sql(Statement::from_string(
                self.db.get_ref().get_database_backend(),
                sql,
            ))
            .all(self.db.get_ref()) // 这里的返回值会自动推导为 Vec<scheduled_tasks::Model>
            .await?;

        for task in tasks {
            let engine = Arc::new(self.clone_logic()); // 模拟克隆引用
            actix_web::rt::spawn(async move {
                engine.dispatch_with_precision(task).await;
            });
        }

        Ok(())
    }

    async fn dispatch_with_precision(&self, task: scheduled_tasks::Model) {
        if !task.enabled {
            warn!("[{}] task is disabled : {:?}", task.task_key, task);
            return;
        }

        let task_key = task.task_key.clone();

        // --- 精准睡眠阶段 ---
        if let Some(execute_at) = task.execute_at {
            let now = Utc::now();

            // 将 execute_at (FixedOffset) 转为 Utc 进行计算
            let target_time = execute_at.with_timezone(&Utc);

            if target_time > now {
                let duration_to_wait = target_time - now;
                let std_duration = duration_to_wait.to_std().unwrap_or(Duration::from_secs(0));

                info!(
                    "[Precision] 任务 {} 提前命中，等待 {:?} 后准时执行",
                    task_key, std_duration
                );
                actix_web::rt::time::sleep(std_duration).await;
            }
        }

        if is_expired(task.expires_at.as_ref(), Utc::now()) {
            warn!(
                "[Scheduler] 任务 '{}' 已超过 expires_at，跳过执行",
                task_key
            );
            self.mark_done(task, Ok(())).await;
            return;
        }

        // --- 执行阶段 with timeout and panic isolation ---
        info!("[Execute] 时间已到，精准触发: {}", task_key);

        // Per-task timeout from regenerated Entity (`sql/update/01-scheduler-retry.sql`).
        let timeout_secs = task.timeout_secs.map(|s| s.max(1) as u64).unwrap_or(60);

        let parsed_key = TaskKey::from_str(&task_key);
        let result = if let Ok(parsed_key) = parsed_key {
            if let Some(handler) = self.registry.get(parsed_key) {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    handler.run(task.clone()),
                )
                .await
                {
                    Ok(inner_result) => inner_result,
                    Err(_elapsed) => {
                        error!(
                            "[Scheduler] 任务 '{}' 执行超时 ({}s)",
                            task_key, timeout_secs
                        );
                        Err(anyhow!("Task execution timed out after {timeout_secs}s"))
                    }
                }
            } else {
                error!(
                    "[Scheduler] 任务 '{}' 的 task_key '{}' 未注册处理器，标记为失败",
                    task.task_name, task_key
                );
                Err(anyhow!("未注册处理器: {}", task_key))
            }
        } else {
            error!(
                "[Scheduler] 任务 '{}' 的 task_key '{}' 无效，标记为失败",
                task.task_name, task_key
            );
            Err(anyhow!("无效的任务 key: {}", task_key))
        };

        self.mark_done(task, result).await;
    }

    async fn mark_done(&self, task: scheduled_tasks::Model, res: Result<()>) {
        let mut active_item: scheduled_tasks::ActiveModel = task.clone().into_active_model();
        let now = Utc::now();

        // 更新最后运行时间 / 释放 worker lock
        active_item.updated_at = ActiveValue::Set(now.into());
        active_item.last_run_at = ActiveValue::Set(Some(now.into()));
        active_item.locked_at = ActiveValue::Set(None);
        active_item.heartbeat_at = ActiveValue::Set(Some(now.into()));

        if let Err(e) = res {
            let attempts = task.attempt_count.saturating_add(1);
            let max_attempts = task.max_attempts.max(1);
            active_item.attempt_count = ActiveValue::Set(attempts);
            active_item.last_error = ActiveValue::Set(Some(e.to_string()));
            active_item.error_msg = ActiveValue::Set(Some(e.to_string()));
            if attempts < max_attempts {
                // Retry: re-queue as pending (fixed backoff: 30s * attempt)
                active_item.status = ActiveValue::Set("pending".to_string());
                let backoff = chrono::Duration::seconds(30 * attempts as i64);
                active_item.execute_at = ActiveValue::Set(Some((now + backoff).into()));
                warn!(
                    "⚠️ 任务 {} 失败 (attempt {}/{}), 将重试: {}",
                    task.task_key, attempts, max_attempts, e
                );
            } else {
                active_item.status = ActiveValue::Set("failed".to_string());
                error!("❌ 任务执行出错且已达最大重试: {}", e);
            }
            self.logger
                .add_log(
                    "ERROR",
                    "TASK",
                    "EXECUTE",
                    format!("{} 执行失败", task.task_name).as_str(),
                    json!({
                        "task": task.task_name,
                        "error": e.to_string(),
                        "attempt": attempts,
                        "max_attempts": max_attempts,
                    }),
                    None,
                    None,
                    None,
                )
                .await;
        } else {
            // Success: reset attempt counter for cron re-queue path.
            active_item.attempt_count = ActiveValue::Set(0);
            active_item.last_error = ActiveValue::Set(None);
            // ✨ 核心逻辑：区分触发类型
            match task.trigger_type.as_str() {
                "cron" => {
                    if let Some(cron_expr) = &task.cron_expr {
                        match cron::Schedule::from_str(cron_expr) {
                            Ok(schedule) => {
                                let next_tick = schedule.upcoming(Utc).next();
                                if let Some(next_time) = next_tick {
                                    // ✨ 增加：检查是否超过了结束时间 (end_time/expires_at)
                                    let is_expired = if let Some(end_time) = task.expires_at {
                                        next_time > end_time.with_timezone(&Utc)
                                    } else {
                                        false
                                    };

                                    if is_expired {
                                        active_item.status =
                                            ActiveValue::Set("completed".to_string());
                                        info!(
                                            "[Cron] 任务 {} 已到期 (expires_at)，停止循环",
                                            task.task_key
                                        );
                                    } else {
                                        active_item.execute_at =
                                            ActiveValue::Set(Some(next_time.into()));
                                        active_item.status =
                                            ActiveValue::Set("pending".to_string());
                                        info!(
                                            "[Cron] 任务 {} 已重置，下次执行: {:?}",
                                            task.task_key, next_time
                                        );
                                    }
                                } else {
                                    active_item.status = ActiveValue::Set("completed".to_string());
                                }
                            }
                            Err(e) => {
                                // 如果 Cron 表达式写错了，不能让它死循环，设为 failed
                                active_item.status = ActiveValue::Set("failed".to_string());
                                active_item.error_msg =
                                    ActiveValue::Set(Some(format!("Cron 解析失败: {}", e)));
                            }
                        }
                    }
                }
                "startup" => {
                    // startup 任务执行完后，通常设为 completed
                    // 这样在本次运行期间不会再被扫描，直到下次重启被 init_and_recover 重置
                    active_item.status = ActiveValue::Set("completed".to_string());
                }
                _ => {
                    // once 类型执行完直接结束
                    active_item.status = ActiveValue::Set("completed".to_string());
                }
            }
            self.logger
                .add_log(
                    "INFO",
                    "TASK",
                    "EXECUTE",
                    format!("{} 执行成功", task.task_name).as_str(),
                    json!([]),
                    None,
                    None,
                    None,
                )
                .await;
        }

        if let Err(e) = active_item.update(self.db.get_ref()).await {
            error!("❌ 数据库状态更新失败: {}", e);
        }
    }

    pub async fn init_and_recover(&self) -> Result<()> {
        info!("[Scheduler] 正在恢复未完成的任务");
        scheduled_tasks::Entity::update_many()
            .col_expr(scheduled_tasks::Column::Status, Expr::value("pending"))
            .filter(scheduled_tasks::Column::Status.eq("running"))
            .exec(self.db.get_ref())
            .await?;
        info!("[Scheduler] 恢复完成");

        info!("[Scheduler] 正在执行Startup任务");
        let startup_tasks = scheduled_tasks::Entity::find()
            .filter(scheduled_tasks::Column::TriggerType.eq("startup"))
            .all(self.db.get_ref())
            .await?;
        for task in startup_tasks {
            let scheduler_arc = Arc::new(self.clone_logic());
            actix_web::rt::spawn(async move {
                // dispatch_with_precision already calls mark_done internally
                scheduler_arc.dispatch_with_precision(task).await;
            });
        }
        info!("[Scheduler] Startup任务 执行完成");
        self.fetch_and_run().await?;

        Ok(())
    }

    fn clone_logic(&self) -> Self {
        Self {
            db: self.db.clone(),
            docker: self.docker.clone(),
            logger: self.logger.clone(),
            rustfs: self.rustfs.clone(),
            registry: self.registry.clone(),
        }
    }

    pub async fn seed_startup_tasks(&self) -> Result<()> {
        // 固定主键定义在 `core::system_ids`（Rust 为权威源，启动时 seed 入库）
        for &(id, name, task_key, trigger_type) in
            crate::core::system_ids::startup_scheduled_task_seeds()
        {
            let exists = scheduled_tasks::Entity::find_by_id(id)
                .one(self.db.get_ref())
                .await?;

            if exists.is_none() {
                warn!("[Init] 数据库中未发现基础任务 '{}'，正在初始化...", name);

                let startup_model = scheduled_tasks::ActiveModel {
                    id: ActiveValue::Set(id),
                    task_name: ActiveValue::Set(name.to_string()),
                    task_key: ActiveValue::Set(task_key.to_string()),
                    trigger_type: ActiveValue::Set(trigger_type.to_string()),
                    status: ActiveValue::Set("pending".to_string()),
                    created_at: ActiveValue::Set(Utc::now().into()),
                    updated_at: ActiveValue::Set(Utc::now().into()),
                    ..Default::default()
                };

                startup_model.insert(self.db.get_ref()).await?;
                info!("[Init] 任务 '{}' 成功录入数据库 id={}", name, id);
            }
        }
        Ok(())
    }

    pub async fn validate_enabled_task_keys(&self) -> Result<()> {
        let tasks = scheduled_tasks::Entity::find()
            .filter(scheduled_tasks::Column::Enabled.eq(true))
            .all(self.db.get_ref())
            .await?;

        let mut invalid = Vec::new();
        for task in tasks {
            match TaskKey::from_str(&task.task_key) {
                Ok(key) if self.registry.contains(key) => {}
                Ok(_) => invalid.push(format!("{} (not registered)", task.task_key)),
                Err(_) => invalid.push(format!("{} (unknown)", task.task_key)),
            }
        }

        if invalid.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(
                "enabled scheduled tasks have invalid handlers: {}",
                invalid.join(", ")
            ))
        }
    }
}

fn is_expired(
    expires_at: Option<&chrono::DateTime<chrono::FixedOffset>>,
    now: chrono::DateTime<Utc>,
) -> bool {
    expires_at
        .map(|deadline| deadline.with_timezone(&Utc) < now)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler;

    #[async_trait]
    impl TaskHandler for TestHandler {
        fn task_key(&self) -> TaskKey {
            TaskKey::AwdEventStart
        }

        fn trigger_type(&self) -> &'static str {
            "once"
        }

        async fn run(&self, _task: scheduled_tasks::Model) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn registry_rejects_duplicate_task_keys() {
        let mut registry = TaskRegistry::default();
        registry.register(Arc::new(TestHandler)).unwrap();

        let error = registry.register(Arc::new(TestHandler)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate scheduled task handler")
        );
        assert_eq!(registry.handlers.len(), 1);
    }

    #[test]
    fn expired_tasks_are_skipped() {
        let now = Utc::now();
        let past = (now - chrono::Duration::seconds(1)).fixed_offset();
        let future = (now + chrono::Duration::seconds(1)).fixed_offset();

        assert!(is_expired(Some(&past), now));
        assert!(!is_expired(Some(&future), now));
        assert!(!is_expired(None, now));
    }
}
