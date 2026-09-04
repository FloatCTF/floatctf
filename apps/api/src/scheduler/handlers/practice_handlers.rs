//! 练习赛事确保与实例清理相关调度处理器。

use std::sync::Arc;

use crate::{
    core::AppConfig,
    entity::scheduled_tasks,
    infrastructure::{WebDb, WebDocker},
    modules::event::common::domain::practice_event::ensure_practice_jeopardy_event,
    modules::event::jeopardy::InstanceService,
    scheduler::{TaskHandler, TaskKey},
};
use async_trait::async_trait;

use tracing::{error, info, warn};

pub struct CleanRunningInstancesHandler {
    pub db: WebDb,
    pub docker: WebDocker,
}

#[async_trait]
impl TaskHandler for CleanRunningInstancesHandler {
    fn trigger_type(&self) -> &'static str {
        "startup"
    }
    fn task_key(&self) -> TaskKey {
        TaskKey::CleanInstances
    }
    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()> {
        info!("{} CleanRunningInstancesHandler", self.task_key());

        info!("{} task is running : {:?}", self.task_key(), &task);

        let service =
            InstanceService::with_docker(self.db.get_ref().clone(), self.docker.get_ref().clone());
        let report = service.cleanup_running().await?;

        for instance_id in report.completed {
            info!("{} Killed instance {}", self.task_key(), instance_id);
        }
        let failed_count = report.failed.len();
        for failure in report.failed {
            error!(
                "{} failed to clean instance {} (retryable={}): {}",
                self.task_key(),
                failure.instance_id,
                failure.retryable,
                failure.message
            );
        }

        if failed_count > 0 {
            return Err(anyhow::anyhow!(
                "{} instance cleanup operation(s) failed",
                failed_count
            ));
        }

        Ok(())
    }
}

pub struct CheckPracticeEventHandler {
    pub db: WebDb,
    pub docker: WebDocker,
    pub config: Arc<AppConfig>,
}

#[async_trait]
impl TaskHandler for CheckPracticeEventHandler {
    fn trigger_type(&self) -> &'static str {
        "startup"
    }
    fn task_key(&self) -> TaskKey {
        TaskKey::CheckPracticeEvent
    }

    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()> {
        let _ = &task;
        // Jeopardy 练习赛事：practice:jeopardy
        info!(
            "{} ensuring practice:jeopardy system event",
            self.task_key()
        );
        let practice_event = ensure_practice_jeopardy_event(self.db.get_ref()).await?;
        info!(
            "{} practice event ready id={} system_key={:?}",
            self.task_key(),
            practice_event.id,
            practice_event.system_key
        );
        // AWDP 练习赛事：AWDPlusPractice（system_key=awdp-practice，练习模块单挂载点）
        let awdp_event_id =
            crate::modules::event::awdp::repo::run_repo::ensure_practice_event(self.db.get_ref())
                .await
                .map_err(|e| anyhow::anyhow!("ensure AWDPlusPractice failed: {e}"))?;
        info!(
            "{} AWDPlusPractice ready id={} system_key={}",
            self.task_key(),
            awdp_event_id,
            crate::core::system_ids::EVENT_PRACTICE_AWDP_SYSTEM_KEY
        );

        // AWDP 练习 docker 环境 ensure（练习 data 网络 + control 网络 + JudgeServer）：
        // best-effort——失败仅告警不阻塞启动（周期任务 awdp.practice.judge 会持续重试自愈）。
        info!(
            "{} ensuring AWDP practice docker environment (network + judge)",
            self.task_key()
        );
        if let Err(e) =
            crate::modules::event::awdp::service::practice_judge::ensure_practice_environment(
                self.db.get_ref(),
                self.docker.get_ref(),
                &self.config.awdp,
                self.config.auth.jwt_secret.expose().as_bytes(),
            )
            .await
        {
            warn!(
                "{} ensure AWDP practice docker environment failed (best-effort, awdp.practice.judge will retry): {e}",
                self.task_key()
            );
        }
        Ok(())
    }
}
