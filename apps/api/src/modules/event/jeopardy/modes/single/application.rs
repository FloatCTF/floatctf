//! Single-player formal Jeopardy entry points.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Result, anyhow};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::{
    entity::{
        challenge_instances, challenges, event_users, event_writeup, events,
        jeopardy_challenge_solves, jeopardy_event_challenges, sea_orm_active_enums::InstanceStatus,
        users,
    },
    infrastructure::WebDb,
    modules::event::jeopardy::{
        application::{
            context::{EventContext, ModeInstanceResult},
            core,
        },
        domain::{
            scoreboard::{ChallengeScoreboard, ScoreboardItem},
            solve::SolveSubject,
            trend::{TrendItem, TrendPoint},
        },
    },
};

use super::policy::JeopardySinglePolicy;

#[derive(Clone, Default)]
pub struct JeopardySingleServices;

impl JeopardySingleServices {
    pub fn policy(&self) -> JeopardySinglePolicy {
        JeopardySinglePolicy
    }

    pub async fn submit_flag(
        &self,
        ctx: &EventContext,
        instance_id: Uuid,
        flag: &str,
    ) -> Result<()> {
        ctx.should_user_joined().await?;
        ctx.should_ongoing()?;
        core::jeopardy_submit(ctx, instance_id, flag, SolveSubject::User).await
    }

    pub async fn launch_instance(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<challenge_instances::Model> {
        ctx.should_user_joined().await?;
        ctx.should_ongoing()?;
        core::jeopardy_launch(ctx, challenge_id, SolveSubject::User).await
    }

    pub async fn challenge_solve_status(
        &self,
        db: &sea_orm::DatabaseConnection,
        event_id: Uuid,
        challenge_id: Uuid,
        user_id: Uuid,
    ) -> Result<(bool, u64)> {
        core::challenge_solve_status(db, event_id, challenge_id, user_id, SolveSubject::User).await
    }

    pub async fn get_instance_by_challenge_id(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<challenge_instances::Model> {
        ctx.should_user_joined().await?;
        ctx.should_ongoing_or_ended()?;

        challenge_instances::Entity::find()
            .filter(challenge_instances::Column::EventId.eq(ctx.event.id))
            .filter(challenge_instances::Column::UserId.eq(ctx.user.id))
            .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
            .filter(challenge_instances::Column::ChallengeId.eq(challenge_id))
            .one(ctx.db.get_ref())
            .await?
            .ok_or(anyhow!("no instance"))
    }

    pub async fn get_instances(&self, ctx: &EventContext) -> Result<Vec<ModeInstanceResult>> {
        ctx.should_user_joined().await?;
        ctx.should_ongoing_or_ended()?;

        let db = ctx.db.get_ref();
        let data = challenge_instances::Entity::find()
            .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
            .filter(challenge_instances::Column::UserId.eq(ctx.user.id))
            .filter(challenge_instances::Column::EventId.eq(ctx.event.id))
            .find_also_related(challenges::Entity)
            .find_also_related(users::Entity)
            .all(db)
            .await?;

        Ok(data
            .into_iter()
            .map(|(instance, challenge_opt, user_opt)| ModeInstanceResult {
                instance,
                challenge_name: challenge_opt.map(|c| c.name).unwrap_or_default(),
                nickname: user_opt.map(|u| u.nickname).unwrap_or_default(),
            })
            .collect())
    }

    pub async fn get_scoreboard(
        &self,
        db: &WebDb,
        event: &events::Model,
    ) -> Result<Vec<ScoreboardItem>> {
        let event_id = event.id;

        let jeopardy_event_challenges = jeopardy_event_challenges::Entity::find()
            .filter(jeopardy_event_challenges::Column::EventId.eq(event_id))
            .filter(jeopardy_event_challenges::Column::Hidden.eq(false))
            .all(db.get_ref())
            .await?;

        let challenge_ids: Vec<Uuid> = jeopardy_event_challenges
            .iter()
            .map(|ec| ec.challenge_id)
            .collect();

        let challenges = challenges::Entity::find()
            .filter(challenges::Column::Id.is_in(challenge_ids.clone()))
            .all(db.get_ref())
            .await?;
        let challenge_map: HashMap<Uuid, challenges::Model> =
            challenges.into_iter().map(|c| (c.id, c)).collect();

        let event_users = event_users::Entity::find()
            .filter(event_users::Column::EventId.eq(event_id))
            .filter(event_users::Column::Banned.eq(false))
            .order_by_desc(event_users::Column::Points)
            .all(db.get_ref())
            .await?;
        let user_ids: Vec<Uuid> = event_users.iter().map(|eu| eu.user_id).collect();

        let users = users::Entity::find()
            .filter(users::Column::Id.is_in(user_ids.clone()))
            .all(db.get_ref())
            .await?;

        let user_map: HashMap<Uuid, users::Model> = users.into_iter().map(|u| (u.id, u)).collect();

        let solves = jeopardy_challenge_solves::Entity::find()
            .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
            .order_by_asc(jeopardy_challenge_solves::Column::ChallengeId)
            .order_by_asc(jeopardy_challenge_solves::Column::CreatedAt)
            .all(db.get_ref())
            .await?;

        let mut user_solved: HashSet<(Uuid, Uuid)> = HashSet::new();
        let mut solve_order: HashMap<(Uuid, Uuid), u64> = HashMap::new();
        let mut total_solved_per_chal: HashMap<Uuid, u64> = HashMap::new();

        for s in solves {
            user_solved.insert((s.user_id, s.challenge_id));
            let entry = total_solved_per_chal.entry(s.challenge_id).or_insert(0);
            *entry += 1;
            solve_order
                .entry((s.user_id, s.challenge_id))
                .or_insert(*entry);
        }

        let mut scoreboard = Vec::new();
        for (no, event_user) in event_users.iter().enumerate() {
            let user = user_map
                .get(&event_user.user_id)
                .ok_or(anyhow!("user not found"))?;

            let mut challenges = Vec::new();
            for ec in jeopardy_event_challenges.iter() {
                let solved = user_solved.contains(&(event_user.user_id, ec.challenge_id));
                let order_for_user = solve_order
                    .get(&(event_user.user_id, ec.challenge_id))
                    .cloned()
                    .unwrap_or(0);
                let challenge = challenge_map
                    .get(&ec.challenge_id)
                    .ok_or(anyhow!("challenge not found"))?;
                challenges.push(ChallengeScoreboard {
                    name: challenge.name.clone(),
                    solved,
                    solved_no: order_for_user,
                });
            }

            let solved_count = challenges.iter().filter(|c| c.solved).count() as u64;
            scoreboard.push(ScoreboardItem {
                id: user.id,
                no: no as u64 + 1,
                name: user.nickname.clone(),
                avatar: user.avatar.clone(),
                score: event_user.points,
                solved_count,
                challenges,
            });
        }

        Ok(scoreboard)
    }

    pub async fn get_trend(&self, db: &WebDb, event: &events::Model) -> Result<Vec<TrendItem>> {
        let event_id = event.id;
        let solves = jeopardy_challenge_solves::Entity::find()
            .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
            .order_by_asc(jeopardy_challenge_solves::Column::CreatedAt)
            .all(db.get_ref())
            .await?;

        let challenge_ids: Vec<Uuid> = solves.iter().map(|s| s.challenge_id).collect();
        let challenges_map: HashMap<Uuid, challenges::Model> = challenges::Entity::find()
            .filter(challenges::Column::Id.is_in(challenge_ids))
            .all(db.get_ref())
            .await?
            .into_iter()
            .map(|c| (c.id, c))
            .collect();

        let user_ids: Vec<Uuid> = solves.iter().map(|s| s.user_id).collect();
        let users_map: HashMap<Uuid, users::Model> = users::Entity::find()
            .filter(users::Column::Id.is_in(user_ids.clone()))
            .all(db.get_ref())
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();

        let mut user_solves_map: HashMap<Uuid, Vec<jeopardy_challenge_solves::Model>> =
            HashMap::new();
        for solve in solves {
            user_solves_map
                .entry(solve.user_id)
                .or_default()
                .push(solve);
        }

        let mut all_times = BTreeSet::new();
        for solves in user_solves_map.values() {
            for s in solves {
                all_times.insert(s.created_at);
            }
        }

        let mut user_scores: HashMap<Uuid, f64> = HashMap::new();
        let mut trend_items_map: HashMap<Uuid, Vec<TrendPoint>> = HashMap::new();

        for &time in &all_times {
            for (&user_id, solves) in &user_solves_map {
                let score = user_scores.entry(user_id).or_insert(0.0);
                for solve in solves.iter().filter(|s| s.created_at == time) {
                    *score += solve.bonus_points;
                }
                let name = solves
                    .iter()
                    .find(|s| s.created_at == time)
                    .and_then(|s| challenges_map.get(&s.challenge_id))
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                trend_items_map
                    .entry(user_id)
                    .or_default()
                    .push(TrendPoint {
                        name,
                        score: *score,
                        time,
                    });
            }
        }

        Ok(user_scores
            .keys()
            .map(|user_id| TrendItem {
                name: users_map.get(user_id).unwrap().nickname.clone(),
                points: trend_items_map.get(user_id).unwrap().clone(),
            })
            .collect())
    }

    pub async fn own_writeup_file_url(
        &self,
        db: &WebDb,
        event: &events::Model,
        user: &users::Model,
    ) -> Result<Option<String>> {
        let wp = event_writeup::Entity::find()
            .filter(event_writeup::Column::EventId.eq(event.id))
            .filter(event_writeup::Column::UserId.eq(user.id))
            .one(db.get_ref())
            .await?;
        Ok(wp.map(|w| w.file_url))
    }
}
