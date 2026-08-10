//! Application service coordinating instance persistence and container lifecycle.
//!
//! v2: instances are pinned to an immutable `challenge_revisions` row. Launch
//! resolves the image pin (RepoDigest > image_id), port and flag semantics from
//! the revision — never from a mutable `challenges.toml_str`.

use std::sync::Arc;

use anyhow::{Context, anyhow};
use bollard::Docker;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait, ModelTrait};
use tracing::warn;
use uuid::Uuid;

use crate::{
    entity::{
        challenge_revisions, challenges, instances, sea_orm_active_enums::InstanceStatus, users,
    },
    infrastructure::settings::get_setting,
};

use crate::modules::challenge::build::revision_repo;
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

    /// Launch a challenge instance pinned to `revision_id` for `user_id`.
    ///
    /// Order: ensure pinned image → start container (if docker challenge) →
    /// insert Running row (with `challenge_revision_id`) → schedule auto-destroy.
    pub async fn launch(
        &self,
        challenge_id: Uuid,
        revision_id: Uuid,
        identifier: String,
        user_id: Uuid,
        r#ref: String,
        flag_prefix: Option<String>,
    ) -> anyhow::Result<instances::Model> {
        let challenge = challenges::Entity::find_by_id(challenge_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow!("no such challenge: {}", challenge_id))?;

        // 钉住的 Revision 决定一切 runtime 契约（§91：Instance 不得自行查 latest）
        let revision = challenge_revisions::Entity::find_by_id(revision_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow!("no such revision: {}", revision_id))?;
        if revision.challenge_id != challenge_id {
            return Err(anyhow!(
                "revision {} does not belong to challenge {}",
                revision_id,
                challenge_id
            ));
        }
        if revision.build_status != revision_repo::BUILD_STATUS_READY {
            return Err(anyhow!(
                "revision {} is not ready (build_status={})",
                revision_id,
                revision.build_status
            ));
        }

        // Flag semantics（explicit tagged union）
        let flag = match revision.flag_type.as_str() {
            "dynamic" => Self::gen_flag(&self.db, flag_prefix).await,
            "static" => revision
                .static_flag_value
                .clone()
                .ok_or_else(|| anyhow!("static revision {} has no flag value", revision_id))?,
            other => {
                return Err(anyhow!(
                    "unknown flag_type '{other}' on revision {revision_id}"
                ));
            }
        };

        // Docker runtime spec（non-docker 题目无容器）
        let runtime_spec = match revision.container_port {
            Some(port) => {
                let pin =
                    revision_repo::effective_image_ref(&revision).map_err(|e| anyhow!("{e}"))?;
                Some(ChallengeRuntimeSpec {
                    image_ref: pin,
                    container_port: port as u16,
                    // dynamic → FLAG env；static → 镜像内置，不注入
                    flag: if revision.flag_type == "dynamic" {
                        Some(flag.clone())
                    } else {
                        None
                    },
                    cpu_millis: revision.recommended_cpu_millis,
                    memory_bytes: revision.recommended_memory_bytes,
                    pids_limit: revision.recommended_pids_limit,
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
        let new_instance = instances::ActiveModel {
            status: Set(InstanceStatus::Running),
            flag: Set(flag),
            content: Set(content.into()),
            user_id: Set(user_id),
            challenge_id: Set(challenge_id.into()),
            challenge_revision_id: Set(Some(revision_id.into())),
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

    /// Decide whether the instance had a docker runtime from its pinned revision.
    async fn remove_runtime_if_needed(&self, instance: &instances::Model) -> anyhow::Result<()> {
        let is_docker = match instance.challenge_revision_id {
            Some(rev_id) => {
                let rev = challenge_revisions::Entity::find_by_id(rev_id)
                    .one(&self.db)
                    .await?
                    .ok_or_else(|| {
                        anyhow!("revision {} not found for instance {}", rev_id, instance.id)
                    })?;
                rev.container_port.is_some()
            }
            None => {
                // 历史行：回退到 challenge 最新 ready revision（旧数据迁移路径）
                let Some(challenge_id) = instance.challenge_id else {
                    return Ok(());
                };
                match revision_repo::find_latest_ready(&self.db, challenge_id).await? {
                    Some(rev) => rev.container_port.is_some(),
                    None => false,
                }
            }
        };

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
