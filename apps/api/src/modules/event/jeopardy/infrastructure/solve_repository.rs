//! Jeopardy 解题记录持久化辅助（支持事务）。

use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, PaginatorTrait,
    QueryFilter, Set, Statement,
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

/// 对同一赛事题目加事务级 advisory lock，统一动态分值的 solve 顺序。
/// PostgreSQL 在事务结束时自动释放该锁。
pub async fn lock_event_challenge<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
    challenge_id: Uuid,
) -> Result<(), sea_orm::DbErr> {
    let lock_sql = format!(
        "SELECT pg_advisory_xact_lock(hashtextextended('jeopardy-score:{event_id}:{challenge_id}'::text, 0))"
    );
    db.execute(Statement::from_string(DbBackend::Postgres, lock_sql))
        .await?;
    Ok(())
}

pub async fn award_user_points<C: ConnectionTrait>(
    db: &C,
    event_id: Uuid,
    user_id: Uuid,
    points: f64,
) -> Result<(), anyhow::Error> {
    let result = event_users::Entity::update_many()
        .col_expr(
            event_users::Column::Points,
            Expr::col(event_users::Column::Points).add(points),
        )
        .filter(event_users::Column::EventId.eq(event_id))
        .filter(event_users::Column::UserId.eq(user_id))
        .filter(event_users::Column::Banned.eq(false))
        .exec(db)
        .await?;
    if result.rows_affected != 1 {
        return Err(anyhow::anyhow!("no active event_user"));
    }
    Ok(())
}

pub async fn award_team_points<C: ConnectionTrait>(
    db: &C,
    team_id: Uuid,
    points: f64,
) -> Result<(), anyhow::Error> {
    let result = event_teams::Entity::update_many()
        .col_expr(
            event_teams::Column::Points,
            Expr::col(event_teams::Column::Points).add(points),
        )
        .filter(event_teams::Column::Id.eq(team_id))
        .filter(event_teams::Column::Banned.eq(false))
        .exec(db)
        .await?;
    if result.rows_affected != 1 {
        return Err(anyhow::anyhow!("no active event_team"));
    }
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
        obtained_points: Set(points),
        bonus_points: Set(0.0),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(())
}
