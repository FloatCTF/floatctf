//! Jeopardy 实例启动 / 列表 / 销毁统一用例。

use anyhow::{Result, anyhow};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{
    challenges, event_challenge_instance, event_instances, jeopardy_challenge_solves,
    jeopardy_event_challenges,
    sea_orm_active_enums::{EventPurpose, ParticipantMode},
    users,
};
use crate::modules::event::jeopardy::application::{
    common,
    context::{EventContext, ModeInstanceResult},
    participant::{resolve_participant, resolve_team_id_for_user},
};
use crate::modules::event::jeopardy::domain::policy::JeopardyPolicy;
use crate::modules::event::jeopardy::domain::solve::SolveSubject;

/// 为 `challenge_id` 启动（或复用）运行中实例。
pub async fn launch_instance(
    ctx: &EventContext,
    challenge_id: Uuid,
) -> Result<event_challenge_instance::Model> {
    JeopardyPolicy::require_jeopardy_family(&ctx.event)?;
    let policy = JeopardyPolicy::from_event(&ctx.event).map_err(|e| anyhow!(e))?;

    // Competition requires join + ongoing; Practice is open training.
    if policy.is_competition() {
        ctx.should_user_joined().await?;
        ctx.should_ongoing()?;
    }

    let participant = resolve_participant(ctx).await?;
    let db = ctx.db.get_ref();
    let event_id = ctx.event.id;
    let user_id = ctx.user.id;
    let team_id = participant.team_id_for_instance();
    let max_instances = policy.max_concurrent_instances(participant.team_member_count);

    if policy.requires_event_challenge() {
        jeopardy_event_challenges::Entity::find()
            .filter(
                jeopardy_event_challenges::Column::EventId
                    .eq(event_id)
                    .and(jeopardy_event_challenges::Column::ChallengeId.eq(challenge_id)),
            )
            .one(db)
            .await?
            .ok_or_else(|| anyhow!("challenge is not in this event"))?;
    }

    let mut running_q = event_challenge_instance::Entity::find()
        .filter(event_challenge_instance::Column::EventId.eq(event_id))
        .inner_join(event_instances::Entity)
        .filter(event_instances::Column::RuntimeState.eq("running"));
    running_q = match participant.subject {
        SolveSubject::User => {
            running_q.filter(event_challenge_instance::Column::UserId.eq(user_id))
        }
        SolveSubject::Team => {
            running_q.filter(event_challenge_instance::Column::TeamId.eq(team_id.expect("team")))
        }
    };
    let running_count = running_q.count(db).await?;
    if running_count >= max_instances {
        return Err(anyhow!(
            "you can only launch {} instances at the same time in {} mode",
            max_instances,
            match (policy.purpose(), policy.participant_mode()) {
                (EventPurpose::Practice, _) => "practice",
                (_, ParticipantMode::Individual) => "individual",
                (_, ParticipantMode::Team) => "team",
            }
        ));
    }

    let mut existing_q = event_challenge_instance::Entity::find()
        .filter(event_challenge_instance::Column::EventId.eq(event_id))
        .inner_join(event_instances::Entity)
        .filter(event_instances::Column::RuntimeState.eq("running"))
        .filter(event_challenge_instance::Column::ChallengeId.eq(challenge_id));
    existing_q = match participant.subject {
        SolveSubject::User => {
            existing_q.filter(event_challenge_instance::Column::UserId.eq(user_id))
        }
        SolveSubject::Team => {
            existing_q.filter(event_challenge_instance::Column::TeamId.eq(team_id.expect("team")))
        }
    };
    if let Some(instance) = existing_q.one(db).await? {
        return Ok(instance);
    }

    let identifier = match participant.subject {
        SolveSubject::User if policy.is_practice() => {
            let user_id_prefix = common::get_uuid_prefix(&user_id);
            let challenge_id_prefix = common::get_uuid_prefix(&challenge_id);
            format!("JP-{}-{}", user_id_prefix, challenge_id_prefix)
        }
        SolveSubject::User => {
            let event_id_prefix = common::get_uuid_prefix(&event_id);
            let user_id_prefix = common::get_uuid_prefix(&user_id);
            let challenge_id_prefix = common::get_uuid_prefix(&challenge_id);
            format!(
                "JS-{}-{}-{}",
                event_id_prefix, user_id_prefix, challenge_id_prefix
            )
        }
        SolveSubject::Team => {
            let event_id_prefix = common::get_uuid_prefix(&event_id);
            let team_id_prefix = common::get_uuid_prefix(&team_id.expect("team"));
            let challenge_id_prefix = common::get_uuid_prefix(&challenge_id);
            format!(
                "JT-{}-{}-{}",
                event_id_prefix, team_id_prefix, challenge_id_prefix
            )
        }
    };

    common::launch_instance(
        &ctx.db,
        &ctx.docker,
        event_id,
        challenge_id,
        identifier,
        user_id,
        team_id,
        ctx.event.flag_prefix.clone(),
    )
    .await
    .map_err(|e| anyhow!(e))
}

/// 销毁当前主体拥有的运行中实例。
pub async fn destroy_instance(ctx: &EventContext, instance_id: Uuid) -> Result<()> {
    JeopardyPolicy::require_jeopardy_family(&ctx.event)?;
    common::destroy_instance(&ctx.db, &ctx.docker, instance_id, &ctx.user).await
}

/// 列出当前参赛主体可见的运行中实例。
pub async fn get_instances(ctx: &EventContext) -> Result<Vec<ModeInstanceResult>> {
    JeopardyPolicy::require_jeopardy_family(&ctx.event)?;
    let policy = JeopardyPolicy::from_event(&ctx.event).map_err(|e| anyhow!(e))?;

    if policy.is_competition() {
        ctx.should_user_joined().await?;
        ctx.should_ongoing_or_ended()?;
    }

    let participant = resolve_participant(ctx).await?;
    let db = ctx.db.get_ref();

    match participant.subject {
        SolveSubject::User => {
            let data = event_challenge_instance::Entity::find()
                .filter(event_challenge_instance::Column::UserId.eq(ctx.user.id))
                .filter(event_challenge_instance::Column::EventId.eq(ctx.event.id))
                .find_also_related(event_instances::Entity)
                .find_also_related(challenges::Entity)
                .filter(event_instances::Column::RuntimeState.eq("running"))
                .all(db)
                .await?;

            // 昵称单独批量水合（find_also_related 最多支持 3 实体）。
            let user_ids: Vec<Uuid> = data.iter().map(|(i, _, _)| i.user_id).collect();
            let mut nickname_by_id = std::collections::HashMap::new();
            if !user_ids.is_empty() {
                for user in users::Entity::find()
                    .filter(users::Column::Id.is_in(user_ids))
                    .all(db)
                    .await?
                {
                    nickname_by_id.insert(user.id, user.nickname);
                }
            }

            Ok(data
                .into_iter()
                .filter_map(|(instance, runtime_opt, challenge_opt)| {
                    runtime_opt.map(|runtime| {
                        let nickname = nickname_by_id
                            .get(&instance.user_id)
                            .cloned()
                            .unwrap_or_default();
                        ModeInstanceResult {
                            instance,
                            runtime,
                            challenge_name: challenge_opt.map(|c| c.name).unwrap_or_default(),
                            nickname,
                        }
                    })
                })
                .collect())
        }
        SolveSubject::Team => {
            let team_id = participant.team_id.expect("team");
            let data = event_challenge_instance::Entity::find()
                .filter(event_challenge_instance::Column::EventId.eq(ctx.event.id))
                .filter(event_challenge_instance::Column::TeamId.eq(team_id))
                .find_also_related(event_instances::Entity)
                .find_also_related(challenges::Entity)
                .filter(event_instances::Column::RuntimeState.eq("running"))
                .all(db)
                .await?;

            Ok(data
                .into_iter()
                .filter_map(|(instance, runtime_opt, challenge_opt)| {
                    runtime_opt.map(|runtime| ModeInstanceResult {
                        instance,
                        runtime,
                        challenge_name: challenge_opt.map(|c| c.name).unwrap_or_default(),
                        nickname: "team_".to_string(),
                    })
                })
                .collect())
        }
    }
}

/// 当前参赛范围内某题的运行中实例。
pub async fn get_instance_by_challenge_id(
    ctx: &EventContext,
    challenge_id: Uuid,
) -> Result<(event_challenge_instance::Model, event_instances::Model)> {
    JeopardyPolicy::require_jeopardy_family(&ctx.event)?;
    let policy = JeopardyPolicy::from_event(&ctx.event).map_err(|e| anyhow!(e))?;

    if policy.is_competition() {
        ctx.should_user_joined().await?;
        ctx.should_ongoing_or_ended()?;
    }

    let participant = resolve_participant(ctx).await?;
    let db = ctx.db.get_ref();

    let mut q = event_challenge_instance::Entity::find()
        .filter(event_challenge_instance::Column::EventId.eq(ctx.event.id))
        .filter(event_challenge_instance::Column::ChallengeId.eq(challenge_id))
        .find_also_related(event_instances::Entity)
        .filter(event_instances::Column::RuntimeState.eq("running"));

    q = match participant.subject {
        SolveSubject::User => q.filter(event_challenge_instance::Column::UserId.eq(ctx.user.id)),
        SolveSubject::Team => q.filter(
            event_challenge_instance::Column::TeamId.eq(participant.team_id.expect("team")),
        ),
    };

    let (instance, runtime) = q.one(db).await?.ok_or_else(|| anyhow!("no instance"))?;
    let runtime = runtime.ok_or_else(|| anyhow!("no runtime for instance"))?;
    Ok((instance, runtime))
}

/// 当前用户的逐题解题状态（战队模式按战队作用域）。
pub async fn challenge_solve_status(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    challenge_id: Uuid,
    user_id: Uuid,
    participant_mode: &ParticipantMode,
) -> Result<(bool, u64)> {
    match participant_mode {
        ParticipantMode::Individual => {
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
        ParticipantMode::Team => {
            let team_id = resolve_team_id_for_user(db, event_id, user_id).await?;
            let team_solve = jeopardy_challenge_solves::Entity::find()
                .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
                .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
                .filter(jeopardy_challenge_solves::Column::TeamId.eq(team_id))
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
