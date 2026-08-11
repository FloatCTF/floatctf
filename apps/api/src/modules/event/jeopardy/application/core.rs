//! Shared Jeopardy launch + solve-status helpers.
//! Formal flag scoring lives in `submission_service`.

use anyhow::{Result, anyhow};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use uuid::Uuid;

use crate::{
    entity::{
        challenge_instances, event_team_members, jeopardy_challenge_solves,
        jeopardy_event_challenges, sea_orm_active_enums::InstanceStatus,
    },
    modules::event::jeopardy::{
        application::{
            common, context::EventContext, submission_service::JeopardySubmissionService,
        },
        domain::solve::{JeopardySubmitRequest, SolveSubject},
    },
};

pub use crate::modules::event::jeopardy::domain::solve::SolveSubject as SolveSubjectReexport;

pub async fn jeopardy_submit(
    ctx: &EventContext,
    instance_id: Uuid,
    flag: &str,
    subject: SolveSubject,
) -> Result<()> {
    let service =
        JeopardySubmissionService::new(ctx.db.get_ref().clone(), ctx.docker.get_ref().clone());
    service
        .submit(JeopardySubmitRequest {
            event_id: ctx.event.id,
            user_id: ctx.user.id,
            instance_id,
            flag: flag.to_string(),
            subject,
        })
        .await
}

pub async fn jeopardy_launch(
    ctx: &EventContext,
    challenge_id: Uuid,
    subject: SolveSubject,
) -> Result<challenge_instances::Model> {
    let db = ctx.db.get_ref();
    let user = ctx.user.clone();
    let event_id = ctx.event.id;

    let (team_id, max_instances, mode_label) = match subject {
        SolveSubject::User => {
            let max: u64 = 2;
            (None, max, "individual")
        }
        SolveSubject::Team => {
            let team_member = event_team_members::Entity::find()
                .filter(
                    event_team_members::Column::EventId
                        .eq(event_id)
                        .and(event_team_members::Column::UserId.eq(user.id)),
                )
                .one(db)
                .await?
                .ok_or(anyhow!("you are not in any team"))?;
            let team_member_count = event_team_members::Entity::find()
                .filter(event_team_members::Column::TeamId.eq(team_member.team_id))
                .count(db)
                .await?;
            (
                Some(team_member.team_id),
                team_member_count.saturating_mul(2).max(2),
                "team",
            )
        }
    };

    let mut running_q = challenge_instances::Entity::find()
        .filter(challenge_instances::Column::EventId.eq(event_id))
        .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running));

    running_q = match subject {
        SolveSubject::User => running_q.filter(challenge_instances::Column::UserId.eq(user.id)),
        SolveSubject::Team => {
            running_q.filter(challenge_instances::Column::TeamId.eq(team_id.unwrap()))
        }
    };

    let running_instances_count = running_q.count(db).await?;
    if running_instances_count >= max_instances {
        return Err(anyhow!(
            "you can only launch {} instances at the same time in {} mode",
            max_instances,
            mode_label
        ));
    }

    // Existing running instance for this challenge
    let mut existing_q = challenge_instances::Entity::find()
        .filter(challenge_instances::Column::EventId.eq(event_id))
        .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
        .filter(challenge_instances::Column::ChallengeId.eq(challenge_id));

    existing_q = match subject {
        SolveSubject::User => existing_q.filter(challenge_instances::Column::UserId.eq(user.id)),
        SolveSubject::Team => {
            existing_q.filter(challenge_instances::Column::TeamId.eq(team_id.unwrap()))
        }
    };

    if let Some(instance) = existing_q.one(db).await? {
        return Ok(instance);
    }

    let identifier = match subject {
        SolveSubject::User => {
            let event_id_prefix = common::get_uuid_prefix(&event_id);
            let user_id_prefix = common::get_uuid_prefix(&user.id);
            let challenge_id_prefix = common::get_uuid_prefix(&challenge_id);
            format!(
                "JS-{}-{}-{}",
                event_id_prefix, user_id_prefix, challenge_id_prefix
            )
        }
        SolveSubject::Team => {
            let event_id_prefix = common::get_uuid_prefix(&event_id);
            let team_id_prefix = common::get_uuid_prefix(&team_id.unwrap());
            let challenge_id_prefix = common::get_uuid_prefix(&challenge_id);
            format!(
                "JT-{}-{}-{}",
                event_id_prefix, team_id_prefix, challenge_id_prefix
            )
        }
    };

    jeopardy_event_challenges::Entity::find()
        .filter(
            jeopardy_event_challenges::Column::EventId
                .eq(event_id)
                .and(jeopardy_event_challenges::Column::ChallengeId.eq(challenge_id)),
        )
        .one(db)
        .await?
        .ok_or(anyhow!("challenge is not in this event"))?;

    common::launch_instance(
        &ctx.db,
        &ctx.docker,
        event_id,
        challenge_id,
        identifier,
        user.id,
        team_id,
        ctx.event.flag_prefix.clone(),
    )
    .await
    .map_err(|e| anyhow!(e))
}

/// Per-challenge solve status for the current user (and team when applicable).
pub async fn challenge_solve_status(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    challenge_id: Uuid,
    user_id: Uuid,
    subject: SolveSubject,
) -> Result<(bool, u64)> {
    match subject {
        SolveSubject::User => {
            let user_solve = jeopardy_challenge_solves::Entity::find()
                .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
                .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
                .filter(jeopardy_challenge_solves::Column::UserId.eq(user_id))
                .filter(jeopardy_challenge_solves::Column::TeamId.is_null())
                .one(db)
                .await?;
            let solved = user_solve.is_some();
            let mut solved_no = 0u64;
            if let Some(us) = user_solve {
                let before_count = jeopardy_challenge_solves::Entity::find()
                    .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
                    .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
                    .filter(jeopardy_challenge_solves::Column::CreatedAt.lt(us.created_at))
                    .count(db)
                    .await?;
                solved_no = before_count + 1;
            }
            Ok((solved, solved_no))
        }
        SolveSubject::Team => {
            let team_member = event_team_members::Entity::find()
                .filter(event_team_members::Column::EventId.eq(event_id))
                .filter(event_team_members::Column::UserId.eq(user_id))
                .one(db)
                .await?
                .ok_or(anyhow!("you are not in any team"))?;

            let team_solve = jeopardy_challenge_solves::Entity::find()
                .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
                .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
                .filter(jeopardy_challenge_solves::Column::TeamId.eq(team_member.team_id))
                .one(db)
                .await?;

            let solved = team_solve.is_some();
            let mut solved_no = 0u64;
            if let Some(ts) = team_solve {
                let before_count = jeopardy_challenge_solves::Entity::find()
                    .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
                    .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
                    .filter(jeopardy_challenge_solves::Column::CreatedAt.lt(ts.created_at))
                    .count(db)
                    .await?;
                solved_no = before_count + 1;
            }
            Ok((solved, solved_no))
        }
    }
}
