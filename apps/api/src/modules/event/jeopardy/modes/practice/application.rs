//! Practice-mode entry points (thin; engine lives in `event::jeopardy`).

use anyhow::{Result, anyhow};
use bollard::Docker;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{instances, sea_orm_active_enums::InstanceStatus, users};
use crate::modules::event::jeopardy::{
    application::{
        common,
        context::{EventContext, ModeInstanceResult, SubmitFlagRequest},
        instance_service::InstanceService,
    },
    domain::{policy::JeopardyModePolicy, scoreboard::ScoreboardItem, trend::TrendItem},
    submit_practice,
};
use crate::{entity::events, infrastructure::WebDb};

use super::policy::JeopardyPracticePolicy;

#[derive(Clone, Default)]
pub struct JeopardyPracticeServices;

impl JeopardyPracticeServices {
    pub fn policy(&self) -> JeopardyPracticePolicy {
        JeopardyPracticePolicy
    }

    pub async fn submit_flag(
        &self,
        db: &DatabaseConnection,
        docker: &Docker,
        user: &users::Model,
        instance_id: Uuid,
        flag: &str,
    ) -> Result<()> {
        submit_practice(db, docker, user, instance_id, flag).await
    }

    pub async fn submit_from_context(
        &self,
        ctx: &EventContext,
        sfr: &SubmitFlagRequest,
    ) -> Result<()> {
        let instance_id = sfr.instance_id.ok_or(anyhow!("no instance_id"))?;
        self.submit_flag(
            ctx.db.get_ref(),
            ctx.docker.get_ref(),
            &ctx.user,
            instance_id,
            &sfr.flag,
        )
        .await
    }

    pub async fn launch_instance(
        &self,
        db: &WebDb,
        docker: &crate::infrastructure::WebDocker,
        challenge_id: Uuid,
        user_id: Uuid,
    ) -> Result<instances::Model> {
        let policy = self.policy();
        let running_instances_count = instances::Entity::find()
            .filter(
                instances::Column::Status
                    .eq(InstanceStatus::Running)
                    .and(instances::Column::UserId.eq(user_id))
                    .and(instances::Column::Ref.eq("JeopardyPractice")),
            )
            .count(db.get_ref())
            .await?;

        if running_instances_count >= 1 {
            return Err(anyhow!(
                "you can only launch {} instances at the same time in practice mode",
                1
            ));
        }

        if let Some(running_instance) = instances::Entity::find()
            .filter(
                instances::Column::Status
                    .eq(InstanceStatus::Running)
                    .and(instances::Column::ChallengeId.eq(challenge_id))
                    .and(instances::Column::UserId.eq(user_id)),
            )
            .one(db.get_ref())
            .await?
        {
            return Ok(running_instance);
        }

        let identifier = {
            let user_id_prefix = common::get_uuid_prefix(&user_id);
            let challenge_id_prefix = common::get_uuid_prefix(&challenge_id);
            format!("JP-{}-{}", user_id_prefix, challenge_id_prefix)
        };
        // Practice 无 Event pin：钉住 challenge 的 latest ready revision
        let revision = crate::modules::challenge::build::revision_repo::find_latest_ready(
            db.get_ref(),
            challenge_id,
        )
        .await?
        .ok_or_else(|| anyhow!("challenge has no ready revision; import a package first"))?;
        common::launch_instance(
            db,
            docker,
            challenge_id,
            revision.id,
            identifier,
            user_id,
            policy.instance_ref_label().into(),
            None,
        )
        .await
    }

    pub async fn launch_from_context(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<instances::Model> {
        self.launch_instance(&ctx.db, &ctx.docker, challenge_id, ctx.user.id)
            .await
    }

    pub fn instance_service(&self, db: DatabaseConnection, docker: Docker) -> InstanceService {
        InstanceService::with_docker(db, docker)
    }

    pub async fn get_instance_by_challenge_id(
        &self,
        _ctx: &EventContext,
        _challenge_id: Uuid,
    ) -> Result<instances::Model> {
        Err(anyhow!("no need to implement"))
    }

    pub async fn get_instances(&self, _ctx: &EventContext) -> Result<Vec<ModeInstanceResult>> {
        Err(anyhow!("no need to implement"))
    }

    pub async fn get_scoreboard(
        &self,
        _db: &WebDb,
        _event: &events::Model,
    ) -> Result<Vec<ScoreboardItem>> {
        Err(anyhow!("event type not supported"))
    }

    pub async fn get_trend(&self, _db: &WebDb, _event: &events::Model) -> Result<Vec<TrendItem>> {
        Err(anyhow!("event type not supported"))
    }

    pub async fn challenge_solve_status(
        &self,
        _db: &sea_orm::DatabaseConnection,
        _event_id: Uuid,
        _challenge_id: Uuid,
        _user_id: Uuid,
    ) -> Result<(bool, u64)> {
        Err(anyhow!("event type not supported"))
    }

    pub async fn own_writeup_file_url(
        &self,
        _db: &WebDb,
        _event: &events::Model,
        _user: &users::Model,
    ) -> Result<Option<String>> {
        Ok(None)
    }
}
