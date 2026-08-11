//! Persistence helpers for Jeopardy event solves (transaction-aware).

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, Set,
};
use uuid::Uuid;

use crate::entity::{
    event_team_members, event_teams, event_users, jeopardy_challenge_solves,
    jeopardy_event_challenges,
};

use crate::modules::event::jeopardy::domain::solve::SolveSubject;

pub async fn find_team_id_for_user<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
    user_id: Uuid,
) -> Result<Option<Uuid>, sea_orm::DbErr> {
    let member = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await?;
    Ok(member.map(|m| m.team_id))
}

pub async fn already_solved<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
    challenge_id: Uuid,
    user_id: Uuid,
    team_id: Option<Uuid>,
    subject: SolveSubject,
) -> Result<bool, sea_orm::DbErr> {
    match subject {
        SolveSubject::User => Ok(jeopardy_challenge_solves::Entity::find()
            .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
            .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
            .filter(jeopardy_challenge_solves::Column::UserId.eq(user_id))
            .one(db)
            .await?
            .is_some()),
        SolveSubject::Team => {
            let team_id = team_id.ok_or_else(|| {
                sea_orm::DbErr::Custom("team_id required for team solve check".into())
            })?;
            Ok(jeopardy_challenge_solves::Entity::find()
                .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
                .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
                .filter(jeopardy_challenge_solves::Column::TeamId.eq(team_id))
                .one(db)
                .await?
                .is_some())
        }
    }
}

pub async fn solved_count<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
    challenge_id: Uuid,
) -> Result<u64, sea_orm::DbErr> {
    jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
        .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
        .count(db)
        .await
}

pub async fn find_event_challenge_points<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
    challenge_id: Uuid,
) -> Result<Option<f64>, sea_orm::DbErr> {
    Ok(
        jeopardy_event_challenges::Entity::find_by_id((event_id, challenge_id))
            .one(db)
            .await?
            .map(|ec| ec.points),
    )
}

pub async fn award_user_points<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
    user_id: Uuid,
    points: f64,
) -> Result<(), anyhow::Error> {
    let event_user = event_users::Entity::find_by_id((event_id, user_id))
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no event_user"))?;
    if event_user.banned {
        return Err(anyhow::anyhow!("you are banned"));
    }
    let new_points = event_user.points + points;
    let mut m = event_user.into_active_model();
    m.points = Set(new_points);
    m.update(db).await?;
    Ok(())
}

pub async fn award_team_points<C: ConnectionTrait>(
    db: &C,
    team_id: Uuid,
    points: f64,
) -> Result<(), anyhow::Error> {
    let event_team = event_teams::Entity::find_by_id(team_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no event_team"))?;
    if event_team.banned {
        return Err(anyhow::anyhow!("you are banned"));
    }
    let new_points = event_team.points + points;
    let mut m = event_team.into_active_model();
    m.points = Set(new_points);
    m.update(db).await?;
    Ok(())
}

pub async fn insert_solve<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
    challenge_id: Uuid,
    user_id: Uuid,
    team_id: Option<Uuid>,
    points: f64,
) -> Result<(), sea_orm::DbErr> {
    jeopardy_challenge_solves::ActiveModel {
        event_id: Set(event_id),
        challenge_id: Set(challenge_id),
        user_id: Set(user_id),
        team_id: Set(team_id),
        bonus_points: Set(points),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(())
}
