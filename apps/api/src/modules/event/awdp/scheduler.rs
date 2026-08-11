//! AWDP 调度处理器：`awdp.tick`（阶段推进 + 回合物化）与 `awdp.eval.worker`（评估执行）。
//!
//! 设计（plan §30/§31/§32）：不建 participant×gamebox×round 海量任务；
//! 仅 2 个 recurring cron 任务。全部副作用幂等（CAS / unique / idempotency key）。

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use tracing::info;
use uuid::Uuid;

use crate::{
    core::AppConfig,
    infrastructure::{WebDb, WebDocker},
    scheduler::{TaskHandler, TaskKey},
};

/// `awdp.tick`：每 10s 扫描 due 事件（阶段推进 + round cutoff 物化）。
pub struct AwdpTickHandler {
    pub db: WebDb,
    pub docker: WebDocker,
    pub config: Arc<AppConfig>,
}

#[async_trait]
impl TaskHandler for AwdpTickHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdpTick
    }

    fn trigger_type(&self) -> &'static str {
        "cron"
    }

    async fn run(&self, _task: crate::entity::scheduled_tasks::Model) -> anyhow::Result<()> {
        crate::modules::event::awdp::service::tick_service::tick_once(
            self.db.get_ref(),
            self.docker.get_ref(),
            self.config.auth.jwt_secret.expose().as_bytes(),
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }
}

/// `awdp.eval.worker`：每 3s 领取 pending 评估（SKIP LOCKED）并执行（health→judge→exploit→score）。
pub struct AwdpEvalWorkerHandler {
    pub db: WebDb,
    pub docker: WebDocker,
}

#[async_trait]
impl TaskHandler for AwdpEvalWorkerHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdpEvalWorker
    }

    fn trigger_type(&self) -> &'static str {
        "cron"
    }

    async fn run(&self, _task: crate::entity::scheduled_tasks::Model) -> anyhow::Result<()> {
        crate::modules::event::awdp::service::evaluation::worker_round(
            self.db.get_ref(),
            self.docker.get_ref(),
            4,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }
}

/// 幂等 seed：插入 awdp 的 2 个 recurring cron 任务（若不存在）。
pub async fn seed_awdp_recurring_tasks(db: &sea_orm::DatabaseConnection) -> anyhow::Result<()> {
    use crate::entity::scheduled_tasks;

    let specs = [
        (
            TaskKey::AwdpTick,
            "AWDP 阶段推进/回合物化",
            "*/10 * * * * *",
            10i64,
        ),
        (
            TaskKey::AwdpEvalWorker,
            "AWDP 评估 worker",
            "*/3 * * * * *",
            3i64,
        ),
    ];
    for (key, name, cron_expr, every_secs) in specs {
        let exists = scheduled_tasks::Entity::find()
            .filter(scheduled_tasks::Column::TaskKey.eq(key.as_str()))
            .filter(scheduled_tasks::Column::Status.is_in(["pending", "running"]))
            .one(db)
            .await?;
        if exists.is_some() {
            continue;
        }
        let now = Utc::now();
        scheduled_tasks::ActiveModel {
            id: Set(Uuid::new_v4()),
            group_id: Set(None),
            task_name: Set(name.to_string()),
            description: Set(Some(format!("recurring {} (awdp engine)", key.as_str()))),
            task_key: Set(key.as_str().to_string()),
            trigger_type: Set("cron".to_string()),
            status: Set("pending".to_string()),
            enabled: Set(true),
            protected: Set(true),
            cron_expr: Set(Some(cron_expr.to_string())),
            execute_at: Set(Some((now + chrono::Duration::seconds(every_secs)).into())),
            expires_at: Set(None),
            payload: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(3),
            timeout_secs: Set(Some(120)),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
            ..Default::default()
        }
        .insert(db)
        .await?;
        info!("[Init] Seeded recurring task {}", key.as_str());
    }
    Ok(())
}
