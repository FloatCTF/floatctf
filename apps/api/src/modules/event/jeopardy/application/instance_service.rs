//! Application service coordinating instance persistence and container lifecycle.
//!
//! 单版本模型：instance 直接引用 Challenge 当前版本（flag/port/镜像 pin 均来自 identity 行），
//! 不存在 revision 钉住。

use std::sync::Arc;

use anyhow::anyhow;
use bollard::Docker;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use tracing::warn;
use uuid::Uuid;

use crate::{
    entity::{challenge_instances, challenges, sea_orm_active_enums::InstanceStatus, users},
    infrastructure::settings::get_setting,
};

use crate::modules::event::jeopardy::{
    domain::{CleanupFailure, CleanupReport},
    infrastructure::{
        container_runtime::{ChallengeRuntimeSpec, DockerInstanceRuntime, InstanceRuntime},
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

    /// Launch a challenge instance for `user_id` under `event_id`（单版本：使用 Challenge 当前版本字段）。
    ///
    /// Order: ensure pinned image → start container (if docker challenge) →
    /// insert Running row → schedule auto-destroy.
    pub async fn launch(
        &self,
        event_id: Uuid,
        challenge_id: Uuid,
        identifier: String,
        user_id: Uuid,
        team_id: Option<Uuid>,
        flag_prefix: Option<String>,
    ) -> anyhow::Result<challenge_instances::Model> {
        let challenge = challenges::Entity::find_by_id(challenge_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow!("no such challenge: {}", challenge_id))?;

        // 单版本模型：Challenge 当前版本决定一切 runtime 契约
        if challenge.build_status.as_deref() != Some("ready") {
            return Err(anyhow!(
                "challenge {} is not ready (build_status={:?}); import a package first",
                challenge_id,
                challenge.build_status
            ));
        }

        // Flag semantics（explicit tagged union）
        let flag = match challenge.flag_type.as_deref() {
            Some("dynamic") => Self::gen_flag(&self.db, flag_prefix).await,
            Some("static") => challenge
                .static_flag_value
                .clone()
                .ok_or_else(|| anyhow!("static challenge {} has no flag value", challenge_id))?,
            other => {
                return Err(anyhow!(
                    "unknown flag_type '{other:?}' on challenge {challenge_id}"
                ));
            }
        };

        // Docker runtime spec（non-docker 题目无容器）
        let runtime_spec = match challenge.container_port {
            Some(port) => {
                let pin = effective_image_ref(
                    challenge.image_repo_digest.as_deref(),
                    challenge.image_id.as_deref(),
                )
                .map_err(|e| anyhow!("{e}"))?;
                Some(ChallengeRuntimeSpec {
                    image_ref: pin,
                    container_port: port as u16,
                    // dynamic → FLAG env；static → 镜像内置，不注入
                    flag: if challenge.flag_type.as_deref() == Some("dynamic") {
                        Some(flag.clone())
                    } else {
                        None
                    },
                    cpu_millis: challenge.recommended_cpu_millis,
                    memory_bytes: challenge.recommended_memory_bytes,
                    pids_limit: challenge.recommended_pids_limit,
                })
            }
            None => None,
        };

        let node_ip = get_setting(&self.db, "NODE_IP").await?;
        let http_prefix = get_setting(&self.db, "HTTP_PREFIX").await?;

        let content = match &runtime_spec {
            Some(spec) => {
                let port = self.runtime.launch(spec, &identifier).await?;
                let url = format!("{}{}:{}", http_prefix, node_ip, port);
                format!(
                    "<a href=\"{url}\" target=\"_blank\" rel=\"noopener noreferrer\" download >{url}</a>"
                )
            }
            None => "".into(),
        };

        let delay = get_setting(&self.db, "INSTANCE_DESTROY_DELAY")
            .await?
            .parse::<i64>()?;

        let destroy_at = Utc::now() + chrono::Duration::minutes(delay);
        let new_instance = challenge_instances::ActiveModel {
            status: Set(InstanceStatus::Running),
            flag: Set(flag),
            content: Set(content.into()),
            user_id: Set(user_id),
            challenge_id: Set(challenge_id),
            event_id: Set(event_id),
            team_id: Set(team_id),
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

    async fn destroy_model(&self, instance: challenge_instances::Model) -> anyhow::Result<()> {
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

    /// Decide whether the instance had a docker runtime from its challenge's current version.
    async fn remove_runtime_if_needed(
        &self,
        instance: &challenge_instances::Model,
    ) -> anyhow::Result<()> {
        let challenge = challenges::Entity::find_by_id(instance.challenge_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| {
                anyhow!(
                    "challenge {} not found for instance {}",
                    instance.challenge_id,
                    instance.id
                )
            })?;
        let is_docker = challenge.container_port.is_some();

        if is_docker {
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

/// Image pin: `image_repo_digest` (repo@sha256:…) > `image_id` (LocalOnly sha256:…).
pub fn effective_image_ref(
    repo_digest: Option<&str>,
    image_id: Option<&str>,
) -> Result<String, String> {
    if let Some(d) = repo_digest.filter(|d| !d.is_empty()) {
        return Ok(d.to_string());
    }
    if let Some(id) = image_id.filter(|id| !id.is_empty()) {
        return Ok(id.to_string());
    }
    Err("no image pin (image_repo_digest/image_id)".to_string())
}
