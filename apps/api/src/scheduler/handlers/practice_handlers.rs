use crate::{
    entity::{events, scheduled_tasks, sea_orm_active_enums::EventType},
    infrastructure::{WebDb, WebDocker},
    modules::event::jeopardy::InstanceService,
    scheduler::{TaskHandler, TaskKey},
};
use async_trait::async_trait;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use tracing::{error, info};
use uuid::Uuid;

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
        let practice_event = events::Entity::find_by_id(Uuid::nil())
            .one(self.db.get_ref())
            .await?;

        if practice_event.is_some() {
            info!("{} PraticeEvent already exists", self.task_key());
            return Ok(());
        }

        let practice_event = events::ActiveModel {
            id: Set(Uuid::nil()),
            r#type: Set(EventType::JeopardyPractice),
            title: Set("PraticeEvent".into()),
            description: Set(Some("Practice Event".into())),
            hidden: Set(true),
            start_time: Set(Utc::now().into()),
            end_time: Set((Utc::now() + chrono::Duration::days(36500)).into()),
            rules: Set("".into()),
            allow_join: Set(true),
            flag_prefix: Set(None), // use config settings
            created_at: Set(Utc::now().into()),
            updated_at: Set(Utc::now().into()),
            ..Default::default()
        };

        let practice_event = practice_event.insert(self.db.get_ref()).await?;

        info!(
            "{} Inserting practice_event: {:?}",
            self.task_key(),
            practice_event
        );

        Ok(())
    }
}
