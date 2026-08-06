//! Application service coordinating instance persistence and container lifecycle.

use std::sync::Arc;

use anyhow::{Context, anyhow};
use bollard::Docker;
use chrono::Utc;
use fcmc::ChallengeMeta;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait, ModelTrait};
use tracing::warn;
use uuid::Uuid;

use crate::{
    entity::{challenges, instances, sea_orm_active_enums::InstanceStatus, users},
    infrastructure::settings::get_setting,
};

use crate::modules::event::jeopardy::{
    domain::{CleanupFailure, CleanupReport},
    infrastructure::{
        container_runtime::{DockerInstanceRuntime, InstanceRuntime},
        instance_repository as repo,
    },
};

#[derive(Clone)]
pub struct InstanceService {
    db: DatabaseConnection,
    runtime: Arc<dyn InstanceRuntime>,
}

impl InstanceService {
    pub fn new(db: DatabaseConnection, runtime: Arc<dyn InstanceRuntime>) -> Self {
        Self { db, runtime }
    }

    pub fn with_docker(db: DatabaseConnection, docker: Docker) -> Self {
        let runtime = Arc::new(DockerInstanceRuntime::new(docker));
        Self::new(db, runtime)
    }

    /// Launch a challenge instance for `user_id` (Jeopardy path).
    ///
    /// Order: start container (if docker challenge) → insert Running row → schedule auto-destroy.
    pub async fn launch(
        &self,
        challenge_id: Uuid,
        identifier: String,
        user_id: Uuid,
        r#ref: String,
        flag_prefix: Option<String>,
    ) -> anyhow::Result<instances::Model> {
        let challenge = challenges::Entity::find_by_id(challenge_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow!("no such challenge: {}", challenge_id))?;

        let cm = ChallengeMeta::from_toml_str(&challenge.toml_str)
            .context("failed to parse challenge toml_str")?;

        let flag = if cm.flag.value.is_empty() {
            Self::gen_flag(&self.db, flag_prefix).await
        } else {
            cm.flag.value.clone()
        };

        let node_ip = get_setting(&self.db, "NODE_IP").await?;
        let http_prefix = get_setting(&self.db, "HTTP_PREFIX").await?;

        let content = match &cm.docker {
            Some(d) => {
                let port = self.runtime.launch(&cm, &identifier, &flag).await?;
                match d.is_nc {
                    Some(true) => format!("nc {} {}", node_ip, port),
                    _ => {
                        let url = format!("{}{}:{}", http_prefix, node_ip, port);
                        format!(
                            "<a href=\"{url}\" target=\"_blank\" rel=\"noopener noreferrer\" download >{url}</a>",
                        )
                    }
                }
            }
            None => "".into(),
        };

        let delay = get_setting(&self.db, "INSTANCE_DESTROY_DELAY")
            .await?
            .parse::<i64>()?;

        let destroy_at = Utc::now() + chrono::Duration::minutes(delay);
        let new_instance = instances::ActiveModel {
            status: Set(InstanceStatus::Running),
            flag: Set(flag),
            content: Set(content.into()),
            user_id: Set(user_id),
            challenge_id: Set(challenge_id.into()),
            r#ref: Set(r#ref),
            destroy_at: Set(destroy_at.clone().into()),
            identifier: Set(identifier),
            ..Default::default()
        };

        let mut res = new_instance.insert(&self.db).await?;
        res.flag.clear();

        // Auto-destroy after delay (best-effort background task).
        let service = self.clone();
        let d_id = res.id;
        let d_user = users::Entity::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow!("user not found: {}", user_id))?;

        actix_web::rt::spawn(async move {
            let now = Utc::now();
            let delay = (destroy_at - now).to_std();
            match delay {
                Ok(d) => {
                    actix_web::rt::time::sleep(d).await;
                    if let Err(e) = service.destroy_owned(d_id, d_user.id).await {
                        tracing::error!("[@destroy_auto]{}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("[@destroy_auto]{}", e);
                }
            }
        });

        Ok(res)
    }

    /// Destroy a running instance owned by `user_id`.
    ///
    /// Returns `Ok(false)` when no matching running instance exists (idempotent).
    /// Runtime is removed before the DB status becomes `Completed`.
    pub async fn destroy_owned(&self, instance_id: Uuid, user_id: Uuid) -> anyhow::Result<bool> {
        let Some(instance) = repo::find_owned_running(&self.db, instance_id, user_id).await? else {
            return Ok(false);
        };
        self.destroy_model(instance).await?;
        Ok(true)
    }

    pub async fn cleanup_running(&self) -> anyhow::Result<CleanupReport> {
        let instances = repo::list_cleanup_candidates(&self.db).await?;
        let mut report = CleanupReport::default();

        for instance in instances {
            let instance_id = instance.id;
            match self.destroy_model(instance).await {
                Ok(()) => report.completed.push(instance_id),
                Err(error) => report.failed.push(CleanupFailure {
                    instance_id,
                    message: error.to_string(),
                    retryable: true,
                }),
            }
        }

        Ok(report)
    }

    async fn destroy_model(&self, instance: instances::Model) -> anyhow::Result<()> {
        let previous_status = instance.status.clone();
        let result = self.remove_runtime_if_needed(&instance).await;
        match result {
            Ok(()) => {
                repo::transition_status(
                    &self.db,
                    instance.id,
                    previous_status,
                    InstanceStatus::Completed,
                )
                .await?;
                Ok(())
            }
            Err(error) => {
                let instance_id = instance.id;
                if previous_status == InstanceStatus::Running {
                    if let Err(status_error) = repo::transition_status(
                        &self.db,
                        instance_id,
                        InstanceStatus::Running,
                        InstanceStatus::Failed,
                    )
                    .await
                    {
                        warn!(
                            instance_id = %instance_id,
                            error = %status_error,
                            "failed to persist instance cleanup failure"
                        );
                    }
                }
                Err(error)
            }
        }
    }

    async fn remove_runtime_if_needed(&self, instance: &instances::Model) -> anyhow::Result<()> {
        let challenge = instance
            .find_related(challenges::Entity)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow!("challenge not found for instance {}", instance.id))?;
        let metadata = ChallengeMeta::from_toml_str(&challenge.toml_str)
            .context("failed to parse challenge metadata")?;

        if metadata.docker.is_some() {
            self.runtime.stop_and_remove(&instance.identifier).await?;
        }
        Ok(())
    }

    async fn gen_flag(db: &DatabaseConnection, flag_prefix: Option<String>) -> String {
        let unique_value = Uuid::new_v4();
        let prefix = match flag_prefix {
            Some(prefix) => prefix,
            None => get_setting(db, "FLAG_PREFIX")
                .await
                .unwrap_or_else(|_| "flag".into()),
        };
        format!("{}{{{}}}", prefix, unique_value)
    }
}
