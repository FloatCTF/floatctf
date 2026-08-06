//! Score event repository.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::entity::{awd_score_events, sea_orm_active_enums::ScoreEventType};

pub async fn create_score_event(
    db: &impl ConnectionTrait,
    event_id: Uuid,
    round_id: Option<Uuid>,
    team_id: Uuid,
    event_type: ScoreEventType,
    delta: i64,
    idempotency_key: &str,
    related_team_id: Option<Uuid>,
    gamebox_instance_id: Option<Uuid>,
    gamebox_template_id: Option<Uuid>,
    reason: Option<&str>,
) -> Result<awd_score_events::Model, sea_orm::DbErr> {
    let model = awd_score_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round_id),
        team_id: Set(team_id),
        event_type: Set(event_type),
        delta: Set(delta),
        idempotency_key: Set(idempotency_key.to_string()),
        related_team_id: Set(related_team_id),
        gamebox_instance_id: Set(gamebox_instance_id),
        gamebox_template_id: Set(gamebox_template_id),
        reason: Set(reason.map(|s| s.to_string())),
        ..Default::default()
    };

    model.insert(db).await
}

/// Return the current total score for a team by summing deltas.
pub async fn team_total_score(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<i64, sea_orm::DbErr> {
    let scores = awd_score_events::Entity::find()
        .filter(awd_score_events::Column::EventId.eq(event_id))
        .filter(awd_score_events::Column::TeamId.eq(team_id))
        .all(db)
        .await?;
    Ok(scores.iter().map(|s| s.delta).sum())
}

/// Sum deltas for a team filtered by score event types.
pub async fn team_score_for_types(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    types: &[ScoreEventType],
) -> Result<i64, sea_orm::DbErr> {
    if types.is_empty() {
        return Ok(0);
    }
    let scores = awd_score_events::Entity::find()
        .filter(awd_score_events::Column::EventId.eq(event_id))
        .filter(awd_score_events::Column::TeamId.eq(team_id))
        .filter(awd_score_events::Column::EventType.is_in(types.iter().cloned()))
        .all(db)
        .await?;
    Ok(scores.iter().map(|s| s.delta).sum())
}

pub async fn find_score_events_by_team(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<Vec<awd_score_events::Model>, sea_orm::DbErr> {
    awd_score_events::Entity::find()
        .filter(awd_score_events::Column::EventId.eq(event_id))
        .filter(awd_score_events::Column::TeamId.eq(team_id))
        .order_by_desc(awd_score_events::Column::CreatedAt)
        .all(db)
        .await
}

pub async fn find_score_event_by_idempotency_key(
    db: &DatabaseConnection,
    key: &str,
) -> Result<Option<awd_score_events::Model>, sea_orm::DbErr> {
    awd_score_events::Entity::find()
        .filter(awd_score_events::Column::IdempotencyKey.eq(key))
        .one(db)
        .await
}
