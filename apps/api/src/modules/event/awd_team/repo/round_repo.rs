//! Round repository.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
use uuid::Uuid;

use crate::entity::{awd_rounds, sea_orm_active_enums::RoundStatus};

pub async fn find_active_round(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Option<awd_rounds::Model>, sea_orm::DbErr> {
    awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .filter(awd_rounds::Column::Status.is_in([
            RoundStatus::Active,
            RoundStatus::Grace,
            RoundStatus::Paused,
        ]))
        .one(db)
        .await
}

pub async fn find_latest_round(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Option<awd_rounds::Model>, sea_orm::DbErr> {
    awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .order_by_desc(awd_rounds::Column::RoundNumber)
        .one(db)
        .await
}

pub async fn find_round_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<awd_rounds::Model>, sea_orm::DbErr> {
    awd_rounds::Entity::find_by_id(id).one(db).await
}

/// Create a new round in a transaction, ensuring at most one active round.
pub async fn create_round(
    db: &DatabaseConnection,
    event_id: Uuid,
    round_number: i32,
    phase: crate::entity::sea_orm_active_enums::AwdPhase,
    scheduled_end_at: chrono::DateTime<chrono::Utc>,
) -> Result<awd_rounds::Model, sea_orm::DbErr> {
    let model = awd_rounds::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_number: Set(round_number),
        status: Set(RoundStatus::Active),
        phase: Set(phase),
        started_at: Set(chrono::Utc::now().into()),
        scheduled_end_at: Set(scheduled_end_at.into()),
        ..Default::default()
    };

    model.insert(db).await
}

pub async fn update_round_status(
    db: &DatabaseConnection,
    id: Uuid,
    status: RoundStatus,
) -> Result<(), sea_orm::DbErr> {
    let is_completed = status == RoundStatus::Completed;
    let mut active: awd_rounds::ActiveModel = awd_rounds::ActiveModel {
        id: Set(id),
        status: Set(status),
        ..Default::default()
    };
    if is_completed {
        active.completed_at = Set(Some(chrono::Utc::now().into()));
    }
    active.update(db).await?;
    Ok(())
}

pub async fn pause_round(
    db: &DatabaseConnection,
    id: Uuid,
    remaining_secs: i32,
) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_rounds::ActiveModel = awd_rounds::ActiveModel {
        id: Set(id),
        status: Set(RoundStatus::Paused),
        paused_at: Set(Some(chrono::Utc::now().into())),
        remaining_secs: Set(Some(remaining_secs)),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}
