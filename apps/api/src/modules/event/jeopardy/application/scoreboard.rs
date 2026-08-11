//! Official Jeopardy scoreboard (Competition only; Individual or Team rows).

use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::entity::{
    challenges, event_teams, event_users, events, jeopardy_challenge_solves,
    jeopardy_event_challenges, sea_orm_active_enums::ParticipantMode, users,
};
use crate::infrastructure::WebDb;
use crate::modules::event::jeopardy::domain::policy::JeopardyPolicy;
use crate::modules::event::jeopardy::domain::scoreboard::{ChallengeScoreboard, ScoreboardItem};

/// Build official scoreboard for a Jeopardy competition event.
pub async fn get_scoreboard(db: &WebDb, event: &events::Model) -> Result<Vec<ScoreboardItem>> {
    JeopardyPolicy::require_jeopardy_family(event)?;
    let policy = JeopardyPolicy::from_event(event).map_err(|e| anyhow!(e))?;
    if !policy.supports_official_scoreboard() {
        return Err(anyhow!(
            "UnsupportedForPurpose: practice has no official event scoreboard"
        ));
    }

    let event_id = event.id;
    let (challenge_rows, challenge_map) = load_event_challenges(db, event_id).await?;

    match policy.participant_mode() {
        ParticipantMode::Individual => {
            assemble_individual(db, event_id, &challenge_rows, &challenge_map).await
        }
        ParticipantMode::Team => assemble_team(db, event_id, &challenge_rows, &challenge_map).await,
    }
}

async fn load_event_challenges(
    db: &WebDb,
    event_id: Uuid,
) -> Result<(
    Vec<jeopardy_event_challenges::Model>,
    HashMap<Uuid, challenges::Model>,
)> {
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
        .filter(challenges::Column::Id.is_in(challenge_ids))
        .all(db.get_ref())
        .await?;
    let challenge_map: HashMap<Uuid, challenges::Model> =
        challenges.into_iter().map(|c| (c.id, c)).collect();

    Ok((jeopardy_event_challenges, challenge_map))
}

async fn load_solves_ordered(
    db: &WebDb,
    event_id: Uuid,
) -> Result<Vec<jeopardy_challenge_solves::Model>> {
    Ok(jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
        .order_by_asc(jeopardy_challenge_solves::Column::ChallengeId)
        .order_by_asc(jeopardy_challenge_solves::Column::CreatedAt)
        .all(db.get_ref())
        .await?)
}

/// Build per-owner solved set and first-solve ordinal within each challenge.
fn solve_maps_by_owner(
    solves: &[jeopardy_challenge_solves::Model],
    owner_of: impl Fn(&jeopardy_challenge_solves::Model) -> Option<Uuid>,
) -> (HashSet<(Uuid, Uuid)>, HashMap<(Uuid, Uuid), u64>) {
    let mut solved: HashSet<(Uuid, Uuid)> = HashSet::new();
    let mut solve_order: HashMap<(Uuid, Uuid), u64> = HashMap::new();
    let mut total_solved_per_chal: HashMap<Uuid, u64> = HashMap::new();

    for s in solves {
        let Some(owner) = owner_of(s) else {
            continue;
        };
        solved.insert((owner, s.challenge_id));
        let entry = total_solved_per_chal.entry(s.challenge_id).or_insert(0);
        *entry += 1;
        solve_order.entry((owner, s.challenge_id)).or_insert(*entry);
    }
    (solved, solve_order)
}

fn challenge_cells(
    owner_id: Uuid,
    challenge_rows: &[jeopardy_event_challenges::Model],
    challenge_map: &HashMap<Uuid, challenges::Model>,
    solved: &HashSet<(Uuid, Uuid)>,
    solve_order: &HashMap<(Uuid, Uuid), u64>,
) -> Result<Vec<ChallengeScoreboard>> {
    let mut challenges = Vec::new();
    for ec in challenge_rows {
        let is_solved = solved.contains(&(owner_id, ec.challenge_id));
        let order = solve_order
            .get(&(owner_id, ec.challenge_id))
            .cloned()
            .unwrap_or(0);
        let challenge = challenge_map
            .get(&ec.challenge_id)
            .ok_or_else(|| anyhow!("challenge not found"))?;
        challenges.push(ChallengeScoreboard {
            name: challenge.name.clone(),
            solved: is_solved,
            solved_no: order,
        });
    }
    Ok(challenges)
}

async fn assemble_individual(
    db: &WebDb,
    event_id: Uuid,
    challenge_rows: &[jeopardy_event_challenges::Model],
    challenge_map: &HashMap<Uuid, challenges::Model>,
) -> Result<Vec<ScoreboardItem>> {
    let event_users = event_users::Entity::find()
        .filter(event_users::Column::EventId.eq(event_id))
        .filter(event_users::Column::Banned.eq(false))
        .order_by_desc(event_users::Column::Points)
        .all(db.get_ref())
        .await?;
    let user_ids: Vec<Uuid> = event_users.iter().map(|eu| eu.user_id).collect();

    let users = users::Entity::find()
        .filter(users::Column::Id.is_in(user_ids))
        .all(db.get_ref())
        .await?;
    let user_map: HashMap<Uuid, users::Model> = users.into_iter().map(|u| (u.id, u)).collect();

    let solves = load_solves_ordered(db, event_id).await?;
    let (user_solved, solve_order) = solve_maps_by_owner(&solves, |s| Some(s.user_id));

    let mut scoreboard = Vec::new();
    for (no, event_user) in event_users.iter().enumerate() {
        let user = user_map
            .get(&event_user.user_id)
            .ok_or_else(|| anyhow!("user not found"))?;

        let challenges = challenge_cells(
            event_user.user_id,
            challenge_rows,
            challenge_map,
            &user_solved,
            &solve_order,
        )?;
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

async fn assemble_team(
    db: &WebDb,
    event_id: Uuid,
    challenge_rows: &[jeopardy_event_challenges::Model],
    challenge_map: &HashMap<Uuid, challenges::Model>,
) -> Result<Vec<ScoreboardItem>> {
    let event_teams = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .all(db.get_ref())
        .await?;

    let solves = load_solves_ordered(db, event_id).await?;
    let (team_solved, solve_order) = solve_maps_by_owner(&solves, |s| s.team_id);

    let mut scoreboard = Vec::new();
    for (no, event_team) in event_teams.iter().enumerate() {
        let challenges = challenge_cells(
            event_team.id,
            challenge_rows,
            challenge_map,
            &team_solved,
            &solve_order,
        )?;
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
