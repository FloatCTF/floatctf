//! Jeopardy 官方积分趋势（仅竞赛）。

use std::collections::{BTreeSet, HashMap};

use anyhow::{Result, anyhow};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::entity::{
    challenges, event_teams, events, jeopardy_challenge_solves,
    sea_orm_active_enums::ParticipantMode, users,
};
use crate::infrastructure::WebDb;
use crate::modules::event::jeopardy::domain::policy::JeopardyPolicy;
use crate::modules::event::jeopardy::domain::trend::{TrendItem, TrendPoint};

pub async fn get_trend(db: &WebDb, event: &events::Model) -> Result<Vec<TrendItem>> {
    JeopardyPolicy::require_jeopardy_family(event)?;
    let policy = JeopardyPolicy::from_event(event).map_err(|e| anyhow!(e))?;
    if !policy.supports_official_scoreboard() {
        return Err(anyhow!(
            "UnsupportedForPurpose: practice has no official event trend"
        ));
    }

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

    match policy.participant_mode() {
        ParticipantMode::Individual => individual_trend(db, solves, &challenges_map).await,
        ParticipantMode::Team => team_trend(db, solves, &challenges_map).await,
    }
}

fn build_points(
    owner_solves_map: &HashMap<Uuid, Vec<jeopardy_challenge_solves::Model>>,
    challenges_map: &HashMap<Uuid, challenges::Model>,
) -> (HashMap<Uuid, f64>, HashMap<Uuid, Vec<TrendPoint>>) {
    let mut all_times = BTreeSet::new();
    for solves in owner_solves_map.values() {
        for s in solves {
            all_times.insert(s.created_at);
        }
    }

    let mut owner_scores: HashMap<Uuid, f64> = HashMap::new();
    let mut trend_items_map: HashMap<Uuid, Vec<TrendPoint>> = HashMap::new();

    for &time in &all_times {
        for (&owner_id, solves) in owner_solves_map {
            let score = owner_scores.entry(owner_id).or_insert(0.0);
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
                .entry(owner_id)
                .or_default()
                .push(TrendPoint {
                    name,
                    score: *score,
                    time,
                });
        }
    }
    (owner_scores, trend_items_map)
}

async fn individual_trend(
    db: &WebDb,
    solves: Vec<jeopardy_challenge_solves::Model>,
    challenges_map: &HashMap<Uuid, challenges::Model>,
) -> Result<Vec<TrendItem>> {
    let user_ids: Vec<Uuid> = solves.iter().map(|s| s.user_id).collect();
    let users_map: HashMap<Uuid, users::Model> = users::Entity::find()
        .filter(users::Column::Id.is_in(user_ids))
        .all(db.get_ref())
        .await?
        .into_iter()
        .map(|u| (u.id, u))
        .collect();

    let mut user_solves_map: HashMap<Uuid, Vec<jeopardy_challenge_solves::Model>> = HashMap::new();
    for solve in solves {
        user_solves_map
            .entry(solve.user_id)
            .or_default()
            .push(solve);
    }

    let (user_scores, trend_items_map) = build_points(&user_solves_map, challenges_map);

    Ok(user_scores
        .keys()
        .map(|user_id| TrendItem {
            name: users_map
                .get(user_id)
                .map(|u| u.nickname.clone())
                .unwrap_or_default(),
            points: trend_items_map.get(user_id).cloned().unwrap_or_default(),
        })
        .collect())
}

async fn team_trend(
    db: &WebDb,
    solves: Vec<jeopardy_challenge_solves::Model>,
    challenges_map: &HashMap<Uuid, challenges::Model>,
) -> Result<Vec<TrendItem>> {
    let team_ids: Vec<Uuid> = solves.iter().filter_map(|s| s.team_id).collect();
    let teams_map: HashMap<Uuid, event_teams::Model> = event_teams::Entity::find()
        .filter(event_teams::Column::Id.is_in(team_ids))
        .all(db.get_ref())
        .await?
        .into_iter()
        .map(|t| (t.id, t))
        .collect();

    let mut team_solves_map: HashMap<Uuid, Vec<jeopardy_challenge_solves::Model>> = HashMap::new();
    for solve in solves {
        if let Some(tid) = solve.team_id {
            team_solves_map.entry(tid).or_default().push(solve);
        }
    }

    let (team_scores, trend_items_map) = build_points(&team_solves_map, challenges_map);

    Ok(team_scores
        .keys()
        .map(|team_id| TrendItem {
            name: teams_map
                .get(team_id)
                .map(|t| t.name.clone())
                .unwrap_or_default(),
            points: trend_items_map.get(team_id).cloned().unwrap_or_default(),
        })
        .collect())
}
