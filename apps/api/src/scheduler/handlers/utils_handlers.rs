use async_trait::async_trait;
use tracing::info;

use crate::{
    entity::scheduled_tasks,
    infrastructure::{WebDb, WebRustfs},
    scheduler::{TaskHandler, TaskKey},
};

pub struct CleanUnusedRustFSFilesHandler {
    pub db: WebDb,
    pub rustfs: WebRustfs,
}

#[async_trait]
impl TaskHandler for CleanUnusedRustFSFilesHandler {
    fn trigger_type(&self) -> &'static str {
        "cron"
    }
    fn task_key(&self) -> TaskKey {
        TaskKey::CleanRustfs
    }
    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()> {
        info!("{} CleanRunningInstancesHandler", self.task_key());
        // check images mainly
        info!("{} task is running : {:?}", self.task_key(), &task);
        Ok(())
    }
}
