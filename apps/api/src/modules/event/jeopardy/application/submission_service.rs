//! Jeopardy submission application service.
//!
//! Scoring (points + solve row) commits in one DB transaction.
//! Container destruction runs only after commit and does not roll back a valid solve.

use anyhow::{Result, anyhow};
use bollard::Docker;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, TransactionTrait};
use tracing::error;
use uuid::Uuid;

use crate::{
    entity::{challenge_instances, challenges, sea_orm_active_enums::InstanceStatus, users},
    infrastructure::settings::get_setting,
    modules::event::jeopardy::{
        application::instance_service::InstanceService,
        domain::{
            scoring::dynamic_score,
            solve::{JeopardySubmitRequest, SolveSubject},
        },
        infrastructure::solve_repository as repo,
    },
};

pub struct JeopardySubmissionService {
    db: DatabaseConnection,
    docker: Docker,
}

impl JeopardySubmissionService {
    pub fn new(db: DatabaseConnection, docker: Docker) -> Self {
        Self { db, docker }
    }

    /// Validate flag, award points + insert solve atomically, then destroy the instance.
    pub async fn submit(&self, req: JeopardySubmitRequest) -> Result<()> {
        let scored = self.score_in_transaction(&req).await?;

        if scored {
            // Post-commit side effect: never roll back the solve if Docker fails.
            let instances = InstanceService::with_docker(self.db.clone(), self.docker.clone());
            if let Err(e) = instances.destroy_owned(req.instance_id, req.user_id).await {
                error!(
                    instance_id = %req.instance_id,
                    user_id = %req.user_id,
                    error = %e,
                    "failed to destroy instance after successful flag submit; solve retained"
                );
            }
        }

        Ok(())
    }

    /// Returns `true` when a new solve was recorded (instance should be destroyed).
    async fn score_in_transaction(&self, req: &JeopardySubmitRequest) -> Result<bool> {
        // Settings are stable for the request; load outside the short scoring txn.
        let decay = get_setting(&self.db, "EVENT_SCORE_DECAY")
            .await?
            .parse::<f64>()?;
        let min_percent = get_setting(&self.db, "EVENT_SCORE_MIN_PERCENT")
            .await?
            .parse::<f64>()?;

        let txn = self.db.begin().await?;

        let instance = challenge_instances::Entity::find_by_id(req.instance_id)
            .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
            .one(&txn)
            .await?
            .ok_or_else(|| anyhow!("no instance"))?;

        // Team instances are launched by one member but any teammate may submit.
        // Ownership is enforced via team membership + event join checks at the strategy layer.

        let challenge_id = instance.challenge_id;

        let challenge = challenges::Entity::find_by_id(challenge_id)
            .one(&txn)
            .await?
            .ok_or_else(|| anyhow!("no challenge"))?;

        if req.flag != instance.flag {
            return Err(anyhow!("wrong flag"));
        }

        let team_id = match req.subject {
            SolveSubject::User => None,
            SolveSubject::Team => Some(
                repo::find_team_id_for_user(&txn, req.event_id, req.user_id)
                    .await?
                    .ok_or_else(|| anyhow!("you are not in any team"))?,
            ),
        };

        if repo::already_solved(
            &txn,
            req.event_id,
            challenge.id,
            req.user_id,
            team_id,
            req.subject,
        )
        .await?
        {
            txn.commit().await?;
            return Ok(false);
        }

        let base_points = repo::find_event_challenge_points(&txn, req.event_id, challenge.id)
            .await?
            .ok_or_else(|| anyhow!("no event_challenge"))?;

        let solved_count = repo::solved_count(&txn, req.event_id, challenge.id).await?;
        let current_points = dynamic_score(base_points, solved_count, decay, min_percent);

        match req.subject {
            SolveSubject::User => {
                repo::award_user_points(&txn, req.event_id, req.user_id, current_points).await?;
            }
            SolveSubject::Team => {
                repo::award_team_points(&txn, team_id.expect("team subject"), current_points)
                    .await?;
            }
        }

        match repo::insert_solve(
            &txn,
            req.event_id,
            challenge.id,
            req.user_id,
            team_id,
            current_points,
        )
        .await
        {
            Ok(()) => {}
            Err(err) if is_unique_violation(&err.to_string()) => {
                // Concurrent submit won the race — treat as already scored.
                txn.rollback().await.ok();
                return Ok(false);
            }
            Err(e) => return Err(e.into()),
        }

        txn.commit().await?;
        Ok(true)
    }
}

fn is_unique_violation(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("duplicate key") || lower.contains("unique constraint")
}

#[cfg(test)]
mod team_duplicate_solve_tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sea_orm::{
        ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    };

    use crate::entity::{
        challenges, event_team_members, event_teams, event_users, events,
        jeopardy_challenge_solves, jeopardy_event_challenges,
        sea_orm_active_enums::{
            EventFamily, EventPurpose, EventTeamMemberRole, InstanceStatus, ParticipantMode,
        },
    };

    fn db_url() -> String {
        std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
    }

    #[tokio::test]
    async fn team_second_member_submit_does_not_double_award() {
        let db = match sea_orm::Database::connect(db_url()).await {
            Ok(db) => db,
            Err(e) => {
                eprintln!("skip team_duplicate_solve: DB unreachable ({e})");
                return;
            }
        };

        let tag = Uuid::new_v4().simple().to_string();
        let now = Utc::now();
        let event_id = Uuid::new_v4();
        events::ActiveModel {
            id: Set(event_id),
            family: Set(EventFamily::Jeopardy),
            purpose: Set(EventPurpose::Competition),
            participant_mode: Set(ParticipantMode::Team),
            system_key: Set(None),
            title: Set(format!("team-dup-{tag}")),
            hidden: Set(false),
            allow_join: Set(true),
            start_time: Set((now - Duration::hours(1)).into()),
            end_time: Set(Some((now + Duration::hours(4)).fixed_offset())),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("event");

        let team_id = Uuid::new_v4();
        event_teams::ActiveModel {
            id: Set(team_id),
            event_id: Set(event_id),
            name: Set(format!("team-{tag}")),
            points: Set(0.0),
            banned: Set(false),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("team");

        let mut user_ids = Vec::new();
        for who in ["a", "b"] {
            let uid = Uuid::new_v4();
            users::ActiveModel {
                id: Set(uid),
                username: Set(format!("tu-{who}-{tag}")),
                nickname: Set(format!("tn-{who}-{tag}")),
                password: Set("x".into()),
                email: Set(format!("{who}-{tag}@example.test")),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("user");
            event_users::ActiveModel {
                event_id: Set(event_id),
                user_id: Set(uid),
                points: Set(0.0),
                banned: Set(false),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("join");
            event_team_members::ActiveModel {
                event_id: Set(event_id),
                team_id: Set(team_id),
                user_id: Set(uid),
                role: Set(if who == "a" {
                    EventTeamMemberRole::Captain
                } else {
                    EventTeamMemberRole::Member
                }),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("member");
            user_ids.push(uid);
        }
        let user_a = user_ids[0];
        let user_b = user_ids[1];

        let challenge_id = Uuid::new_v4();
        challenges::ActiveModel {
            id: Set(challenge_id),
            name: Set(format!("ch-dup-{tag}")),
            safe_name: Set(format!("ch-dup-{tag}")),
            category: Set("web".into()),
            description: Set("t".into()),
            hidden: Set(true),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("challenge");

        jeopardy_event_challenges::ActiveModel {
            event_id: Set(event_id),
            challenge_id: Set(challenge_id),
            points: Set(100.0),
            hidden: Set(false),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("jec");

        let flag = format!("flag{{{tag}}}");
        let instance_a = Uuid::new_v4();
        challenge_instances::ActiveModel {
            id: Set(instance_a),
            status: Set(InstanceStatus::Running),
            flag: Set(flag.clone()),
            challenge_id: Set(challenge_id),
            user_id: Set(user_a),
            identifier: Set(format!("JT-a-{tag}")),
            destroy_at: Set((now + Duration::hours(1)).into()),
            event_id: Set(event_id),
            team_id: Set(Some(team_id)),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("inst a");

        let docker = match bollard::Docker::connect_with_local_defaults() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skip team_duplicate_solve: docker client ({e})");
                let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
                return;
            }
        };
        let service = JeopardySubmissionService::new(db.clone(), docker);

        service
            .submit(JeopardySubmitRequest {
                event_id,
                user_id: user_a,
                instance_id: instance_a,
                flag: flag.clone(),
                subject: SolveSubject::Team,
            })
            .await
            .expect("A submit");

        let points_after_a = event_teams::Entity::find_by_id(team_id)
            .one(&db)
            .await
            .expect("team")
            .expect("exists")
            .points;
        assert!(points_after_a > 0.0, "first solve awards points");

        let instance_b = Uuid::new_v4();
        challenge_instances::ActiveModel {
            id: Set(instance_b),
            status: Set(InstanceStatus::Running),
            flag: Set(flag.clone()),
            challenge_id: Set(challenge_id),
            user_id: Set(user_b),
            identifier: Set(format!("JT-b-{tag}")),
            destroy_at: Set((now + Duration::hours(1)).into()),
            event_id: Set(event_id),
            team_id: Set(Some(team_id)),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("inst b");

        service
            .submit(JeopardySubmitRequest {
                event_id,
                user_id: user_b,
                instance_id: instance_b,
                flag: flag.clone(),
                subject: SolveSubject::Team,
            })
            .await
            .expect("B submit already-solved path");

        let solve_count = jeopardy_challenge_solves::Entity::find()
            .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
            .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
            .filter(jeopardy_challenge_solves::Column::TeamId.eq(team_id))
            .count(&db)
            .await
            .expect("count");
        assert_eq!(solve_count, 1);

        let points_after_b = event_teams::Entity::find_by_id(team_id)
            .one(&db)
            .await
            .expect("team")
            .expect("exists")
            .points;
        assert_eq!(
            points_after_a, points_after_b,
            "second teammate must not double-award"
        );

        let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
        let _ = challenges::Entity::delete_by_id(challenge_id)
            .exec(&db)
            .await;
        let _ = users::Entity::delete_by_id(user_a).exec(&db).await;
        let _ = users::Entity::delete_by_id(user_b).exec(&db).await;
    }
}

/// Practice mode: record jeopardy_challenge_solves on practice event (0 points), then destroy instance.
pub async fn submit_practice(
    db: &DatabaseConnection,
    docker: &Docker,
    user: &users::Model,
    instance_id: Uuid,
    flag: &str,
) -> Result<()> {
    use sea_orm::{ActiveModelTrait, ColumnTrait, QueryFilter, Set};

    use crate::entity::jeopardy_challenge_solves;

    let instance = challenge_instances::Entity::find_by_id(instance_id)
        .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
        .one(db)
        .await?
        .ok_or_else(|| anyhow!("no instance"))?;

    let challenge_id = instance.challenge_id;
    let event_id = instance.event_id;

    let challenge = challenges::Entity::find_by_id(challenge_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow!("no challenge"))?;

    if flag != instance.flag {
        return Err(anyhow!("flag is not correct"));
    }

    let already = jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
        .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge.id))
        .filter(jeopardy_challenge_solves::Column::UserId.eq(user.id))
        .filter(jeopardy_challenge_solves::Column::TeamId.is_null())
        .one(db)
        .await?;

    if already.is_none() {
        jeopardy_challenge_solves::ActiveModel {
            event_id: Set(event_id),
            challenge_id: Set(challenge.id),
            user_id: Set(user.id),
            team_id: Set(None),
            obtained_points: Set(0.0),
            bonus_points: Set(0.0),
            ..Default::default()
        }
        .insert(db)
        .await?;
    }

    let instances = InstanceService::with_docker(db.clone(), docker.clone());
    if let Err(e) = instances.destroy_owned(instance_id, user.id).await {
        error!(
            instance_id = %instance_id,
            error = %e,
            "failed to destroy practice instance after submit"
        );
    }

    Ok(())
}
