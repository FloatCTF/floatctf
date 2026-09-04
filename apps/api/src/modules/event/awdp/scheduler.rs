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
    infrastructure::{
        WebDb, WebDocker,
        realtime::{EventPublisher, RealtimeEvent},
    },
    scheduler::{TaskHandler, TaskKey},
};

/// `awdp.tick`：每 10s 扫描 due 事件（阶段推进 + round cutoff 物化）。
pub struct AwdpTickHandler {
    pub db: WebDb,
    pub docker: WebDocker,
    pub config: Arc<AppConfig>,
    pub publisher: Arc<dyn EventPublisher>,
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
        let summary = crate::modules::event::awdp::service::tick_service::tick_once(
            self.db.get_ref(),
            self.docker.get_ref(),
            self.config.auth.jwt_secret.expose().as_bytes(),
            &self.config.awdp,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        // tick 自动推进的阶段变化推 SSE（选手端倒计时到点/回合切换即时刷新，
        // 不依赖 15s poll）。admin 手动 start/break-to-fix/finish 已有推送。
        for t in summary.phase_transitions {
            let event = RealtimeEvent::new(
                t.event_id,
                "awdp.phase_changed",
                serde_json::json!({ "phase": t.phase }),
            );
            let publisher = self.publisher.clone();
            actix_web::rt::spawn(async move {
                let _ = publisher.publish(event).await;
            });
        }
        Ok(())
    }
}

/// `awdp.eval.worker`：每 3s 领取 pending 评估（Pull + Lease，SKIP LOCKED）并执行（health→judge→exploit→score）。
pub struct AwdpEvalWorkerHandler {
    pub db: WebDb,
    pub docker: WebDocker,
    pub config: Arc<AppConfig>,
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
            "floatctf-api-worker",
            4,
            self.config.awdp.eval_lease_duration_secs,
            self.config.awdp.eval_max_attempts,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }
}

/// `awdp.practice.judge`：每 30s 幂等 ensure 练习环境（网络 + JudgeServer）。
///
/// 练习是常驻虚拟赛事，judge/网络只在实例启动时惰性创建，无主动 ensure 会失联
/// （docker 清理/容器被杀后需重启平台才恢复）。本任务承担周期自愈：
/// `ensure_practice_environment` 幂等（网络已存在复用、judge running+env 匹配跳过），
/// 失败走 scheduler 既有重试（30s×n backoff）与重启自动复活机制。
pub struct AwdpPracticeJudgeHandler {
    pub db: WebDb,
    pub docker: WebDocker,
    pub config: Arc<AppConfig>,
}

#[async_trait]
impl TaskHandler for AwdpPracticeJudgeHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdpPracticeJudge
    }

    fn trigger_type(&self) -> &'static str {
        "cron"
    }

    async fn run(&self, _task: crate::entity::scheduled_tasks::Model) -> anyhow::Result<()> {
        crate::modules::event::awdp::service::practice_judge::ensure_practice_environment(
            self.db.get_ref(),
            self.docker.get_ref(),
            &self.config.awdp,
            self.config.auth.jwt_secret.expose().as_bytes(),
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }
}

/// 幂等 seed：插入 awdp 的 3 个 recurring cron 任务（若不存在）。
/// 注意：`awdp.practice.judge` 复用旧 sweep-push 派发任务的 task_key——
/// 旧实现已随 pull 模型移除（plan §61），现承担「周期 ensure 练习环境」职责。
pub async fn seed_awdp_recurring_tasks(db: &sea_orm::DatabaseConnection) -> anyhow::Result<()> {
    use crate::entity::scheduled_tasks;

    let specs = [
        (
            TaskKey::AwdpTick,
            "AWDP Practice 阶段推进/回合物化",
            "*/10 * * * * *",
            10i64,
        ),
        (
            TaskKey::AwdpEvalWorker,
            "AWDP Practice 评估 worker",
            "*/3 * * * * *",
            3i64,
        ),
        (
            TaskKey::AwdpPracticeJudge,
            "AWDP Practice 环境 ensure（网络+JudgeServer）",
            "*/30 * * * * *",
            30i64,
        ),
    ];
    for (key, name, cron_expr, every_secs) in specs {
        let exists = scheduled_tasks::Entity::find()
            .filter(scheduled_tasks::Column::TaskKey.eq(key.as_str()))
            // 任何状态（含 failed，等待 init_and_recover 复活）都算已 seed，避免重复行。
            .filter(scheduled_tasks::Column::Status.is_in(["pending", "running", "failed"]))
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
            description: Set(Some(format!(
                "recurring {}（AWDPlusPractice 引擎）",
                key.as_str()
            ))),
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
