//! Player-facing event application service.
//!
//! Business logic extracted from `api/service/events` HTTP handlers.
//! Handlers: auth → parse → call these functions → map errors → UniResponse.

use std::str::FromStr;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, DatabaseConnection, EntityTrait,
    ModelTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api::{AppError, FilterMapping, prelude::*},
    entity::{
        challenge_instances, challenges, event_announcements, event_team_members, event_teams,
        event_users, events, jeopardy_challenge_solves, jeopardy_event_challenges,
        sea_orm_active_enums::{EventFamily, EventPurpose, EventTeamMemberRole, ParticipantMode},
        users,
    },
    infrastructure::{WebDb, WebDocker},
    modules::event::{
        jeopardy::{
            application::context::EventContextBuilder,
            domain::{
                scoreboard::ScoreboardItem, scoring::calculate_next_dynamic_score, trend::TrendItem,
            },
        },
        registry::event_registry,
    },
};

// ── DTOs (shared with HTTP layer via re-exports) ──────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct EventTeamMemberResult {
    pub member_name: String,
    pub member: event_team_members::Model,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventTeamResult {
    pub team: event_teams::Model,
    pub members: Vec<EventTeamMemberResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventInfo {
    pub event: events::Model,
    pub team_result: Option<EventTeamResult>,
    pub joined: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventChallengeResult {
    /// Enriched challenge DTO（latest ready revision 摘要 + 附件元数据）。
    pub challenge: crate::modules::challenge::catalog::ChallengesDto,
    pub current_points: f64,
    pub solved_count: u64,
    pub solved: bool,
    pub solved_no: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventInstanceResult {
    pub instance: challenge_instances::Model,
    pub challenge_name: String,
    pub user_nickname: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserTeam {
    pub name: String,
}

// ── Event lifecycle status ────────────────────────────────────────────────

pub enum EventStatus {
    NotStarted,
    Ongoing,
    Ended,
}

impl EventStatus {
    pub async fn check(db: &DatabaseConnection, event_id: &Uuid) -> Result<Self, AppError> {
        let event = events::Entity::find_by_id(*event_id)
            .filter(events::Column::Hidden.eq(false))
            .one(db)
            .await?
            .ok_or(AppError::NotFound("event not found".to_string()))?;

        use crate::modules::event::common::domain::time_state::{
            EventTimeStatus, event_time_status_of,
        };
        Ok(match event_time_status_of(&event, Utc::now()) {
            EventTimeStatus::NotStarted => Self::NotStarted,
            EventTimeStatus::Ongoing => Self::Ongoing,
            EventTimeStatus::Ended => Self::Ended,
        })
    }

    /// Variant that accepts `WebDb` (legacy call sites).
    pub async fn check_web(db: &WebDb, event_id: &Uuid) -> Result<Self, AppError> {
        Self::check(db.get_ref(), event_id).await
    }
}

fn require_competition(event: &events::Model) -> Result<(), AppError> {
    if event.purpose != EventPurpose::Competition {
        return Err(AppError::BadRequest(
            "UnsupportedForPurpose: only competition events support join/team".into(),
        ));
    }
    if event.system_key.is_some() {
        return Err(AppError::BadRequest(
            "system-managed events cannot be joined".into(),
        ));
    }
    Ok(())
}

fn require_team_mode(event: &events::Model) -> Result<(), AppError> {
    require_competition(event)?;
    if event.participant_mode != ParticipantMode::Team {
        return Err(AppError::BadRequest(
            "UnsupportedForParticipantMode: team operations require team participant mode".into(),
        ));
    }
    Ok(())
}

// ── Queries ───────────────────────────────────────────────────────────────

/// Enrich event rows with join status for `user_id`.
/// Filters/queries are applied by the caller (HTTP layer owns FilterMapping).
pub fn list_events_for_user(
    user_id: Uuid,
    events_with_users: Vec<(events::Model, Vec<event_users::Model>)>,
) -> Vec<EventInfo> {
    events_with_users
        .into_iter()
        .map(|(event, users)| {
            let joined = users.iter().any(|u| u.user_id == user_id);
            EventInfo {
                event,
                joined,
                team_result: None,
            }
        })
        .collect()
}

/// Load a single non-hidden event with team membership detail for the user.
pub async fn get_event_info(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> Result<EventInfo, AppError> {
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(db)
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;

    let joined = event_users::Entity::find_by_id((event_id, user_id))
        .one(db)
        .await?
        .is_some();

    let event_member = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .find_also_related(event_teams::Entity)
        .one(db)
        .await?;

    let team = event_member.and_then(|(_, team)| team);
    match team {
        Some(team) => {
            let members = event_team_members::Entity::find()
                .filter(event_team_members::Column::EventId.eq(event_id))
                .filter(event_team_members::Column::TeamId.eq(team.id))
                .find_also_related(users::Entity)
                .all(db)
                .await?;
            let members = members
                .into_iter()
                .map(|(member, user)| EventTeamMemberResult {
                    member_name: user.map(|u| u.nickname).unwrap_or_default(),
                    member,
                })
                .collect();
            Ok(EventInfo {
                event,
                joined,
                team_result: Some(EventTeamResult { team, members }),
            })
        }
        None => Ok(EventInfo {
            event,
            joined,
            team_result: None,
        }),
    }
}

/// Challenges for a joined user during ongoing/ended events.
pub async fn list_event_challenges(
    db: &WebDb,
    event_id: Uuid,
    user: &users::Model,
) -> Result<Vec<EventChallengeResult>, AppError> {
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;

    match EventStatus::check_web(db, &event.id).await? {
        EventStatus::NotStarted => {
            return Err(AppError::BadRequest("Event is not start".to_string()));
        }
        EventStatus::Ongoing | EventStatus::Ended => {}
    }

    let joined = event_users::Entity::find_by_id((event_id, user.id))
        .one(db.get_ref())
        .await?
        .is_some();

    if !joined {
        return Err(AppError::BadRequest("not joined".to_string()));
    }

    let c_ec = event
        .find_related(jeopardy_event_challenges::Entity)
        .filter(jeopardy_event_challenges::Column::Hidden.eq(false))
        .find_also_related(challenges::Entity)
        .all(db.get_ref())
        .await?;

    let mut result = Vec::new();
    for (event_challenge, challenge) in c_ec {
        if let Some(c) = challenge {
            let solved_count = jeopardy_challenge_solves::Entity::find()
                .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
                .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(c.id))
                .count(db.get_ref())
                .await?;

            let (solved, solved_no) = event_registry()
                .challenge_solve_status(db, &event, user, c.id)
                .await
                .map_err(|e| AppError::BadRequest(format!("{}", e)))?;

            let current_points =
                calculate_next_dynamic_score(db.get_ref(), event_challenge.points, solved_count)
                    .await
                    .map_err(|e| {
                        AppError::BadRequest(format!("calculate_next_dynamic_score error: {}", e))
                    })?;
            result.push(EventChallengeResult {
                challenge: crate::modules::challenge::catalog::ChallengesDto::from(&c),
                current_points,
                solved_count,
                solved,
                solved_no,
            });
        }
    }
    result.sort_by(|a, b| b.challenge.category.cmp(&a.challenge.category));
    Ok(result)
}

pub async fn list_event_instances(
    db: WebDb,
    docker: WebDocker,
    event_id: Uuid,
    user: users::Model,
) -> Result<Vec<EventInstanceResult>, AppError> {
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;

    let event_ctx = EventContextBuilder::new()
        .db(db)
        .docker(docker)
        .user(user)
        .event(Some(event))
        .build()
        .await
        .map_err(|e| AppError::BadRequest(format!("build event context error: {}", e)))?;

    let instances = event_registry()
        .get_instances(&event_ctx)
        .await
        .map_err(|e| AppError::BadRequest(format!("get_instances error: {}", e)))?;

    Ok(instances
        .into_iter()
        .map(|i| EventInstanceResult {
            instance: i.instance,
            challenge_name: i.challenge_name,
            user_nickname: i.nickname,
        })
        .collect())
}

pub async fn get_challenge_instance(
    db: WebDb,
    docker: WebDocker,
    event_id: Uuid,
    challenge_id: Uuid,
    user: users::Model,
) -> Result<challenge_instances::Model, AppError> {
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;

    let event_ctx = EventContextBuilder::new()
        .db(db)
        .docker(docker)
        .user(user)
        .event(Some(event))
        .build()
        .await
        .map_err(|e| AppError::BadRequest(format!("build event context error: {}", e)))?;

    event_registry()
        .get_instance_by_challenge_id(&event_ctx, challenge_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("get_instance_by_challenge_id error: {}", e)))
}

// ── Team membership workflows ─────────────────────────────────────────────

/// Create a team and join as captain (pre-start only). Returns the team model.
pub async fn create_team(
    db: &WebDb,
    event_id: Uuid,
    user_id: Uuid,
    name: String,
) -> Result<event_teams::Model, AppError> {
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;

    require_team_mode(&event)?;

    match EventStatus::check_web(db, &event.id).await? {
        EventStatus::Ongoing | EventStatus::Ended => {
            return Err(AppError::BadRequest("Event has not yet begun".to_string()));
        }
        EventStatus::NotStarted => {}
    }

    let event_user = event_users::Entity::find_by_id((event_id, user_id))
        .one(db.get_ref())
        .await?;

    if event_user.is_some() {
        return Err(AppError::BadRequest("already joined team".to_string()));
    }

    let team = event_teams::ActiveModel {
        name: Set(name),
        event_id: Set(event_id),
        ..Default::default()
    }
    .insert(db.get_ref())
    .await?;

    event_users::ActiveModel {
        event_id: Set(event_id),
        user_id: Set(user_id),
        ..Default::default()
    }
    .insert(db.get_ref())
    .await?;

    event_team_members::ActiveModel {
        event_id: Set(event_id),
        user_id: Set(user_id),
        team_id: Set(team.id),
        role: Set(EventTeamMemberRole::Captain),
        ..Default::default()
    }
    .insert(db.get_ref())
    .await?;

    Ok(team)
}

/// Captain quits → team deleted; member quits → membership removed. Also leaves event_users.
pub async fn quit_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let team_member = event_team_members::Entity::find_by_id((event_id, team_id, user_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound("You are not of the team".to_string()))?;

    if team_member.role == EventTeamMemberRole::Captain {
        let team = event_teams::Entity::find_by_id(team_id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("team not found".to_string()))?;
        team.delete(db).await?;
    } else {
        team_member.delete(db).await?;
    }

    let event_user = event_users::Entity::find_by_id((event_id, user_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound("You are not of the event".to_string()))?;
    event_user.delete(db).await?;
    Ok(())
}

pub async fn join_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<event_teams::Model, AppError> {
    let event = events::Entity::find_by_id(event_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;
    require_team_mode(&event)?;

    let team_member = event_team_members::Entity::find_by_id((event_id, team_id, user_id))
        .one(db)
        .await?;

    if team_member.is_some() {
        return Err(AppError::BadRequest("already joined team".to_string()));
    }
    let event_team = event_teams::Entity::find_by_id(team_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound("team not found".to_string()))?;

    event_team_members::ActiveModel {
        event_id: Set(event_id),
        user_id: Set(user_id),
        team_id: Set(event_team.id),
        role: Set(EventTeamMemberRole::Member),
        ..Default::default()
    }
    .insert(db)
    .await?;

    event_users::ActiveModel {
        event_id: Set(event_id),
        user_id: Set(user_id),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(event_team)
}

pub async fn leave_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let team_member = event_team_members::Entity::find_by_id((event_id, team_id, user_id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound("You are not of the team".to_string()))?;

    if team_member.role == EventTeamMemberRole::Captain {
        return Err(AppError::BadRequest("Captain can't leave team".to_string()));
    }

    team_member.delete(db).await?;
    Ok(())
}

// ── Join / leave event (solo) ─────────────────────────────────────────────

pub async fn join_event(
    db: &WebDb,
    event_id: Uuid,
    user_id: Uuid,
) -> Result<(events::Model, event_users::Model), AppError> {
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;

    require_competition(&event)?;

    if !event.allow_join {
        return Err(AppError::BadRequest("event not allow join".to_string()));
    }

    match EventStatus::check_web(db, &event.id).await? {
        EventStatus::Ongoing | EventStatus::Ended => {
            return Err(AppError::BadRequest("Event has not yet begun".to_string()));
        }
        EventStatus::NotStarted => {}
    }

    let eu = event_users::ActiveModel {
        event_id: Set(event_id),
        user_id: Set(user_id),
        ..Default::default()
    }
    .insert(db.get_ref())
    .await?;

    Ok((event, eu))
}

pub async fn leave_event(
    db: &WebDb,
    event_id: Uuid,
    user_id: Uuid,
) -> Result<(events::Model, u64), AppError> {
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;
    if !event.allow_join {
        return Err(AppError::BadRequest("event not allow leave".to_string()));
    }

    match EventStatus::check_web(db, &event.id).await? {
        EventStatus::Ongoing | EventStatus::Ended => {
            return Err(AppError::BadRequest("Event has not yet begun".to_string()));
        }
        EventStatus::NotStarted => {}
    }

    let event_user = event_users::Entity::find_by_id((event_id, user_id))
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event user not found".to_string()))?;

    let rows = event_user.delete(db.get_ref()).await?.rows_affected;
    Ok((event, rows))
}

// ── Scoreboard / trend adapters (EventModuleRegistry) ─────────────────────

pub async fn get_scoreboard(db: WebDb, event_id: Uuid) -> anyhow::Result<Vec<ScoreboardItem>> {
    let event = events::Entity::find_by_id(event_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;

    event_registry()
        .get_scoreboard(&db, &event)
        .await
        .map_err(|e| AppError::BadRequest(format!("{}", e)).into())
}

pub async fn get_trend(db: WebDb, event_id: Uuid) -> anyhow::Result<Vec<TrendItem>> {
    let event = events::Entity::find_by_id(event_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;

    event_registry()
        .get_trend(&db, &event)
        .await
        .map_err(|e| AppError::BadRequest(format!("{}", e)).into())
}

/// Scoreboard with pre-start challenge list blanked.
pub async fn get_scoreboard_for_player(
    db: WebDb,
    event_id: Uuid,
) -> Result<Vec<ScoreboardItem>, AppError> {
    let mut scoreboard = get_scoreboard(db.clone(), event_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("{}", e)))?;

    match EventStatus::check_web(&db, &event_id).await? {
        EventStatus::NotStarted => {
            for sb in &mut scoreboard {
                sb.challenges = vec![];
            }
        }
        EventStatus::Ongoing | EventStatus::Ended => {}
    }
    Ok(scoreboard)
}

pub async fn list_announcements(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Vec<event_announcements::Model>, AppError> {
    Ok(event_announcements::Entity::find()
        .filter(event_announcements::Column::EventId.eq(event_id))
        .order_by_desc(event_announcements::Column::CreatedAt)
        .all(db)
        .await?)
}

/// Resolve private object key for the player's own writeup (if any).
pub async fn own_writeup_file_url(
    db: &WebDb,
    event_id: Uuid,
    user: &users::Model,
) -> Result<String, AppError> {
    let event = events::Entity::find_by_id(event_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("event {} not found", event_id)))?;

    event_registry()
        .own_writeup_file_url(db, &event, user)
        .await
        .map_err(|e| AppError::BadRequest(format!("{}", e)))?
        .ok_or(AppError::NotFound("Has no wp".into()))
}

// ── Filter helpers for list endpoints (keep FilterMapping construction here) ─

/// Build sea_orm condition filter mappings for player event list.
pub fn player_event_filter_mappings() -> [FilterMapping; 4] {
    [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(events::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "title",
            column: Box::new(|v| Condition::all().add(events::Column::Title.contains(v))),
        },
        FilterMapping {
            key: "family",
            column: Box::new(|v| {
                Condition::all().add(
                    events::Column::Family
                        .eq(serde_json::from_str(v).unwrap_or(EventFamily::Jeopardy)),
                )
            }),
        },
        FilterMapping {
            key: "allow_join",
            column: Box::new(|v| {
                Condition::all()
                    .add(events::Column::AllowJoin.eq(v.parse::<bool>().unwrap_or(false)))
            }),
        },
    ]
}
