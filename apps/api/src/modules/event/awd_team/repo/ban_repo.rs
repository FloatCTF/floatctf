//! Team ban repository.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::entity::{awd_team_bans, sea_orm_active_enums::BanStatus};

pub async fn find_active_ban(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<Option<awd_team_bans::Model>, sea_orm::DbErr> {
    awd_team_bans::Entity::find()
        .filter(awd_team_bans::Column::EventId.eq(event_id))
        .filter(awd_team_bans::Column::TeamId.eq(team_id))
        .filter(awd_team_bans::Column::Status.eq(BanStatus::Active))
        .one(db)
        .await
}

pub async fn create_ban(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    reason: Option<&str>,
    effective_round_id: Option<Uuid>,
    banned_by: Option<Uuid>,
) -> Result<awd_team_bans::Model, sea_orm::DbErr> {
    let model = awd_team_bans::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        team_id: Set(team_id),
        status: Set(BanStatus::Active),
        reason: Set(reason.map(|s| s.to_string())),
        effective_round_id: Set(effective_round_id),
        banned_by: Set(banned_by),
        ..Default::default()
    };
    model.insert(db).await
}

pub async fn request_unban(
    db: &DatabaseConnection,
    id: Uuid,
    unban_effective_round_id: Uuid,
) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_team_bans::ActiveModel = awd_team_bans::ActiveModel {
        id: Set(id),
        status: Set(BanStatus::PendingUnban),
        unban_requested_at: Set(Some(chrono::Utc::now().into())),
        unban_effective_round_id: Set(Some(unban_effective_round_id)),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

pub async fn complete_unban(
    db: &DatabaseConnection,
    id: Uuid,
    unbanned_by: Option<Uuid>,
) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_team_bans::ActiveModel = awd_team_bans::ActiveModel {
        id: Set(id),
        status: Set(BanStatus::Unbanned),
        unbanned_by: Set(unbanned_by),
        unbanned_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}
