//! 练习赛事确保与实例清理相关调度处理器。

use crate::{
    entity::scheduled_tasks,
    infrastructure::{WebDb, WebDocker},
    modules::event::common::domain::practice_event::ensure_practice_jeopardy_event,
    modules::event::jeopardy::InstanceService,
    scheduler::{TaskHandler, TaskKey},
};
use async_trait::async_trait;

use tracing::{error, info};

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
        Ok(())
    }
}
