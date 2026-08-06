//! Team formal Jeopardy entry points.

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Result, anyhow};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::{
    entity::{
        challenges, event_challenge_solves, event_challenges, event_instances, event_team_members,
        event_teams, event_writeup, events, instances, sea_orm_active_enums::InstanceStatus, users,
    },
    infrastructure::WebDb,
    modules::event::jeopardy::{
        application::{
            context::{EventContext, ModeInstanceResult},
            core,
        },
        domain::{
            policy::JeopardyModePolicy,
            scoreboard::{ChallengeScoreboard, ScoreboardItem},
            solve::SolveSubject,
            trend::{TrendItem, TrendPoint},
        },
    },
};

use super::policy::JeopardyTeamPolicy;

#[derive(Clone, Default)]
pub struct JeopardyTeamServices;

impl JeopardyTeamServices {
    pub fn policy(&self) -> JeopardyTeamPolicy {
        JeopardyTeamPolicy
    }

    pub async fn submit_flag(
        &self,
        ctx: &EventContext,
        instance_id: Uuid,
        flag: &str,
    ) -> Result<()> {
        ctx.should_user_joined().await?;
        ctx.should_ongoing()?;
        let _ = self.policy();
        core::jeopardy_submit(ctx, instance_id, flag, SolveSubject::Team).await
    }

    pub async fn launch_instance(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<instances::Model> {
        ctx.should_user_joined().await?;
        ctx.should_ongoing()?;
        debug_assert_eq!(
            self.policy().instance_ref_label(),
            "JeopardyTeam",
            "team mode ref label"
        );
        core::jeopardy_launch(ctx, challenge_id, SolveSubject::Team).await
    }

    pub async fn challenge_solve_status(
        &self,
        db: &sea_orm::DatabaseConnection,
        event_id: Uuid,
        challenge_id: Uuid,
        user_id: Uuid,
    ) -> Result<(bool, u64)> {
        core::challenge_solve_status(db, event_id, challenge_id, user_id, SolveSubject::Team).await
    }

    pub async fn get_instance_by_challenge_id(
        &self,
        ctx: &EventContext,
        challenge_id: Uuid,
    ) -> Result<instances::Model> {
        ctx.should_user_joined().await?;
        ctx.should_ongoing_or_ended()?;

        let team_member = event_team_members::Entity::find()
            .filter(
                event_team_members::Column::EventId
                    .eq(ctx.event.id)
                    .and(event_team_members::Column::UserId.eq(ctx.user.id)),
            )
            .one(ctx.db.get_ref())
            .await?
            .ok_or(anyhow!("you are not in any team"))?;

        let (_event_instance, instance) = event_instances::Entity::find()
            .filter(
                event_instances::Column::EventId
                    .eq(ctx.event.id)
                    .and(event_instances::Column::TeamId.eq(team_member.team_id)),
            )
            .find_also_related(instances::Entity)
            .filter(
                instances::Column::Status
                    .eq(InstanceStatus::Running)
                    .and(instances::Column::ChallengeId.eq(challenge_id)),
            )
            .one(ctx.db.get_ref())
            .await?
            .ok_or(anyhow!("no instance"))?;

        instance.ok_or(anyhow!("no instance"))
    }

    pub async fn get_instances(&self, ctx: &EventContext) -> Result<Vec<ModeInstanceResult>> {
        ctx.should_user_joined().await?;
        ctx.should_ongoing_or_ended()?;

        let team_member = event_team_members::Entity::find()
            .filter(event_team_members::Column::EventId.eq(ctx.event.id))
            .filter(event_team_members::Column::UserId.eq(ctx.user.id))
            .one(ctx.db.get_ref())
            .await?
            .ok_or(anyhow!("you are not in any team"))?;

        let data = event_instances::Entity::find()
            .filter(event_instances::Column::EventId.eq(ctx.event.id))
            .filter(event_instances::Column::TeamId.eq(team_member.team_id))
            .find_also_related(instances::Entity)
            .all(ctx.db.get_ref())
            .await?;

        let mut instances_out = Vec::new();
        for (_ei, inst_opt) in data {
            if let Some(instance) = inst_opt {
                if instance.status != InstanceStatus::Running {
                    continue;
                }
                let challenge_name = if let Some(cid) = instance.challenge_id {
                    challenges::Entity::find_by_id(cid)
                        .one(ctx.db.get_ref())
                        .await?
                        .map(|c| c.name)
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                instances_out.push(ModeInstanceResult {
                    instance,
                    challenge_name,
                    nickname: "team_".to_string(),
                });
            }
        }
        Ok(instances_out)
    }

    pub async fn get_scoreboard(
        &self,
        db: &WebDb,
        event: &events::Model,
    ) -> Result<Vec<ScoreboardItem>> {
        let event_id = event.id;

        let event_challenges = event_challenges::Entity::find()
            .filter(event_challenges::Column::EventId.eq(event_id))
            .filter(event_challenges::Column::Hidden.eq(false))
            .all(db.get_ref())
            .await?;

        let challenge_ids: Vec<Uuid> = event_challenges.iter().map(|ec| ec.challenge_id).collect();

        let challenges = challenges::Entity::find()
            .filter(challenges::Column::Id.is_in(challenge_ids.clone()))
            .all(db.get_ref())
            .await?;
        let challenge_map: HashMap<Uuid, challenges::Model> =
            challenges.into_iter().map(|c| (c.id, c)).collect();

        let event_teams = event_teams::Entity::find()
            .filter(event_teams::Column::EventId.eq(event_id))
            .all(db.get_ref())
            .await?;

        let solves = event_challenge_solves::Entity::find()
            .filter(event_challenge_solves::Column::EventId.eq(event_id))
            .order_by_asc(event_challenge_solves::Column::ChallengeId)
            .order_by_asc(event_challenge_solves::Column::CreatedAt)
            .all(db.get_ref())
            .await?;

        let mut team_solved: HashSet<(Uuid, Uuid)> = HashSet::new();
        let mut solve_order: HashMap<(Uuid, Uuid), u64> = HashMap::new();
        let mut total_solved_per_chal: HashMap<Uuid, u64> = HashMap::new();

        for s in solves {
            team_solved.insert((s.team_id.unwrap(), s.challenge_id));
            let entry = total_solved_per_chal.entry(s.challenge_id).or_insert(0);
            *entry += 1;
            solve_order
                .entry((s.team_id.unwrap(), s.challenge_id))
                .or_insert(*entry);
        }

        let mut scoreboard = Vec::new();
        for (no, event_team) in event_teams.iter().enumerate() {
            let mut challenges = Vec::new();
            for ec in event_challenges.iter() {
                let solved = team_solved.contains(&(event_team.id, ec.challenge_id));
                let order_for_user = solve_order
                    .get(&(event_team.id, ec.challenge_id))
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
                id: event_team.id,
                no: no as u64 + 1,
                name: event_team.name.clone(),
                avatar: None,
                score: event_team.points,
                solved_count,
                challenges,
            });
        }
        Ok(scoreboard)
    }

    pub async fn get_trend(&self, db: &WebDb, event: &events::Model) -> Result<Vec<TrendItem>> {
        let event_id = event.id;
        let solves = event_challenge_solves::Entity::find()
            .filter(event_challenge_solves::Column::EventId.eq(event_id))
            .order_by_asc(event_challenge_solves::Column::CreatedAt)
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

        let team_ids: Vec<Uuid> = solves.iter().filter_map(|s| s.team_id).collect();
        let teams_map: HashMap<Uuid, event_teams::Model> = event_teams::Entity::find()
            .filter(event_teams::Column::Id.is_in(team_ids))
            .all(db.get_ref())
            .await?
            .into_iter()
            .map(|t| (t.id, t))
            .collect();

        let mut team_solves_map: HashMap<Uuid, Vec<event_challenge_solves::Model>> = HashMap::new();
        for solve in solves {
            if let Some(tid) = solve.team_id {
                team_solves_map.entry(tid).or_default().push(solve);
            }
        }

        let mut all_times = BTreeSet::new();
        for solves in team_solves_map.values() {
            for s in solves {
                all_times.insert(s.created_at);
            }
        }

        let mut team_scores: HashMap<Uuid, f64> = HashMap::new();
        let mut trend_items_map: HashMap<Uuid, Vec<TrendPoint>> = HashMap::new();

        for &time in &all_times {
            for (&team_id, solves) in &team_solves_map {
                let score = team_scores.entry(team_id).or_insert(0.0);
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
                    .entry(team_id)
                    .or_default()
                    .push(TrendPoint {
                        name,
                        score: *score,
                        time,
                    });
            }
        }

        Ok(team_scores
            .keys()
            .map(|team_id| TrendItem {
                name: teams_map.get(team_id).unwrap().name.clone(),
                points: trend_items_map.get(team_id).unwrap().clone(),
            })
            .collect())
    }

    pub async fn own_writeup_file_url(
        &self,
        db: &WebDb,
        event: &events::Model,
        user: &users::Model,
    ) -> Result<Option<String>> {
        let team_id = event_team_members::Entity::find()
            .filter(event_team_members::Column::UserId.eq(user.id))
            .filter(event_team_members::Column::EventId.eq(event.id))
            .one(db.get_ref())
            .await?
            .ok_or(anyhow!("This member has no team!"))?
            .team_id;

        let wp = event_writeup::Entity::find()
            .filter(event_writeup::Column::EventId.eq(event.id))
            .filter(event_writeup::Column::TeamId.eq(team_id))
            .one(db.get_ref())
            .await?;
        Ok(wp.map(|w| w.file_url))
    }
}
