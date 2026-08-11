//! Practice-mode entry points (thin; engine lives in `event::jeopardy`).

use anyhow::{Result, anyhow};
use bollard::Docker;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{challenge_instances, sea_orm_active_enums::InstanceStatus, users};
use crate::modules::event::jeopardy::{
    application::{
        common,
        context::{EventContext, ModeInstanceResult, SubmitFlagRequest},
        instance_service::InstanceService,
    },
    domain::{scoreboard::ScoreboardItem, trend::TrendItem},
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
        event_id: Uuid,
        challenge_id: Uuid,
        user_id: Uuid,
        flag_prefix: Option<String>,
    ) -> Result<challenge_instances::Model> {
        let running_instances_count = challenge_instances::Entity::find()
            .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
            .filter(challenge_instances::Column::UserId.eq(user_id))
            .filter(challenge_instances::Column::EventId.eq(event_id))
            .count(db.get_ref())
            .await?;

        if running_instances_count >= 1 {
            return Err(anyhow!(
                "you can only launch {} instances at the same time in practice mode",
                1
            ));
        }

        if let Some(running_instance) = challenge_instances::Entity::find()
            .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
            .filter(challenge_instances::Column::ChallengeId.eq(challenge_id))
            .filter(challenge_instances::Column::UserId.eq(user_id))
            .filter(challenge_instances::Column::EventId.eq(event_id))
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

        common::launch_instance(
            db,
            docker,
            event_id,
            challenge_id,
            identifier,
            user_id,
            None,
            flag_prefix,
        )
        .await
    }

    pub async fn launch_from_context(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<challenge_instances::Model> {
        self.launch_instance(
            &ctx.db,
            &ctx.docker,
            ctx.event.id,
            challenge_id,
            ctx.user.id,
            ctx.event.flag_prefix.clone(),
        )
        .await
    }

    pub fn instance_service(&self, db: DatabaseConnection, docker: Docker) -> InstanceService {
        InstanceService::with_docker(db, docker)
    }

    pub async fn get_instance_by_challenge_id(
        &self,
        _ctx: &EventContext,
        _challenge_id: Uuid,
    ) -> Result<challenge_instances::Model> {
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
        Err(anyhow!("practice has no official event scoreboard"))
    }

    pub async fn get_trend(&self, _db: &WebDb, _event: &events::Model) -> Result<Vec<TrendItem>> {
        Err(anyhow!("practice has no official event trend"))
    }

    pub async fn challenge_solve_status(
        &self,
        db: &sea_orm::DatabaseConnection,
        event_id: Uuid,
        challenge_id: Uuid,
        user_id: Uuid,
    ) -> Result<(bool, u64)> {
        use crate::entity::jeopardy_challenge_solves;
        let solved = jeopardy_challenge_solves::Entity::find()
            .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
            .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
            .filter(jeopardy_challenge_solves::Column::UserId.eq(user_id))
            .one(db)
            .await?
            .is_some();
        Ok((solved, if solved { 1 } else { 0 }))
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
