//! Shared Jeopardy launch + solve-status helpers.
//! Formal flag scoring lives in `submission_service`.

use anyhow::{Result, anyhow};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, JoinType, PaginatorTrait, QueryFilter, QuerySelect,
    RelationTrait, Set,
};
use uuid::Uuid;

use crate::{
    entity::{
        event_challenge_solves, event_instances, event_team_members, instances,
        sea_orm_active_enums::InstanceStatus,
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
) -> Result<instances::Model> {
    let db = ctx.db.get_ref();
    let user = ctx.user.clone();
    let event_id = ctx.event.id;

    let (team_id, max_instances, ref_label) = match subject {
        SolveSubject::User => {
            // 每用户可同时启动的实例数：静态默认 2（原 [challenge].instance_max_per_user）
            let max: u64 = 2;
            (None, max, "JeopardySingle")
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
                team_member_count * 2,
                "JeopardyTeam",
            )
        }
    };

    let running_instances_count = match subject {
        SolveSubject::User => {
            event_instances::Entity::find()
                .filter(
                    event_instances::Column::EventId
                        .eq(event_id)
                        .and(event_instances::Column::UserId.eq(user.id)),
                )
                .join(
                    JoinType::InnerJoin,
                    event_instances::Relation::Instances.def(),
                )
                .filter(
                    instances::Column::Status
                        .eq(InstanceStatus::Running)
                        .and(instances::Column::Ref.eq("JeopardySingle")),
                )
                .count(db)
                .await?
        }
        SolveSubject::Team => {
            event_instances::Entity::find()
                .filter(
                    event_instances::Column::EventId
                        .eq(event_id)
                        .and(event_instances::Column::UserId.eq(user.id))
                        .and(event_instances::Column::TeamId.eq(team_id.unwrap())),
                )
                .join(
                    JoinType::InnerJoin,
                    event_instances::Relation::Instances.def(),
                )
                .filter(
                    instances::Column::Status
                        .eq(InstanceStatus::Running)
                        .and(instances::Column::Ref.eq("JeopardyTeam")),
                )
                .count(db)
                .await?
        }
    };

    if running_instances_count >= max_instances {
        return Err(anyhow!(
            "you can only launch {} instances at the same time in {} mode",
            max_instances,
            ref_label
        ));
    }

    // Existing running instance for this challenge
    let existing = match subject {
        SolveSubject::User => {
            event_instances::Entity::find()
                .filter(
                    event_instances::Column::EventId
                        .eq(event_id)
                        .and(event_instances::Column::UserId.eq(user.id)),
                )
                .find_also_related(instances::Entity)
                .filter(
                    instances::Column::Status
                        .eq(InstanceStatus::Running)
                        .and(instances::Column::ChallengeId.eq(challenge_id)),
                )
                .one(db)
                .await?
        }
        SolveSubject::Team => {
            event_instances::Entity::find()
                .filter(
                    event_instances::Column::EventId
                        .eq(event_id)
                        .and(event_instances::Column::TeamId.eq(team_id.unwrap())),
                )
                .find_also_related(instances::Entity)
                .filter(
                    instances::Column::Status
                        .eq(InstanceStatus::Running)
                        .and(instances::Column::ChallengeId.eq(challenge_id)),
                )
                .one(db)
                .await?
        }
    };
    if let Some((_, Some(instance))) = existing {
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

    let res_instance = common::launch_instance(
        &ctx.db,
        &ctx.docker,
        challenge_id,
        identifier,
        user.id,
        ref_label.into(),
        ctx.event.flag_prefix.clone(),
    )
    .await
    .map_err(|e| anyhow!(e))?;

    event_instances::ActiveModel {
        event_id: Set(event_id),
        user_id: Set(user.id),
        instance_id: Set(res_instance.id),
        team_id: Set(team_id),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(res_instance)
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
            let user_solve =
                event_challenge_solves::Entity::find_by_id((event_id, challenge_id, user_id))
                    .one(db)
                    .await?;
            let solved = user_solve.is_some();
            let mut solved_no = 0u64;
            if let Some(us) = user_solve {
                let before_count = event_challenge_solves::Entity::find()
                    .filter(event_challenge_solves::Column::EventId.eq(event_id))
                    .filter(event_challenge_solves::Column::ChallengeId.eq(challenge_id))
                    .filter(event_challenge_solves::Column::CreatedAt.lt(us.created_at))
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

            let team_solve = event_challenge_solves::Entity::find()
                .filter(event_challenge_solves::Column::EventId.eq(event_id))
                .filter(event_challenge_solves::Column::ChallengeId.eq(challenge_id))
                .filter(event_challenge_solves::Column::TeamId.eq(team_member.team_id))
                .one(db)
                .await?;

            let solved = team_solve.is_some();
            let mut solved_no = 0u64;
            if let Some(ts) = team_solve {
                let before_count = event_challenge_solves::Entity::find()
                    .filter(event_challenge_solves::Column::EventId.eq(event_id))
                    .filter(event_challenge_solves::Column::ChallengeId.eq(challenge_id))
                    .filter(event_challenge_solves::Column::CreatedAt.lt(ts.created_at))
                    .count(db)
                    .await?;
                solved_no = before_count + 1;
            }
            Ok((solved, solved_no))
        }
    }
}
