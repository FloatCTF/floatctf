//! Resolve / ensure the system-managed Jeopardy Practice event.

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};

use crate::entity::{
    events,
    sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
};
use crate::modules::event::common::domain::event_mode::{
    PRACTICE_JEOPARDY_EVENT_ID, PRACTICE_JEOPARDY_SYSTEM_KEY,
};

/// Find Practice event by system_key (canonical lookup).
pub async fn find_practice_jeopardy_event<C: ConnectionTrait>(
    db: &C,
) -> Result<Option<events::Model>, sea_orm::DbErr> {
    events::Entity::find()
        .filter(events::Column::SystemKey.eq(PRACTICE_JEOPARDY_SYSTEM_KEY))
        .one(db)
        .await
}

pub async fn require_practice_jeopardy_event<C: ConnectionTrait>(
    db: &C,
) -> Result<events::Model, sea_orm::DbErr> {
    find_practice_jeopardy_event(db)
        .await?
        .ok_or_else(|| sea_orm::DbErr::RecordNotFound("practice:jeopardy event not found".into()))
}

/// Idempotent ensure of practice:jeopardy system event.
///
/// Fresh insert always uses [`PRACTICE_JEOPARDY_EVENT_ID`] from
/// `core::system_ids` (Rust well-known id, same pattern as scheduler seeds).
/// Existing rows are returned as-is (id remapped by migration if needed).
pub async fn ensure_practice_jeopardy_event<C: ConnectionTrait>(
    db: &C,
) -> Result<events::Model, sea_orm::DbErr> {
    if let Some(existing) = find_practice_jeopardy_event(db).await? {
        return Ok(existing);
    }

    let now = Utc::now().fixed_offset();
    let model = events::ActiveModel {
        id: Set(PRACTICE_JEOPARDY_EVENT_ID),
        family: Set(EventFamily::Jeopardy),
        purpose: Set(EventPurpose::Practice),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(Some(PRACTICE_JEOPARDY_SYSTEM_KEY.into())),
        title: Set("Practice".into()),
        description: Set(Some("Practice Event".into())),
        hidden: Set(true),
        allow_join: Set(false),
        start_time: Set(now),
        end_time: Set(None),
        rules: Set("do not cheat".into()),
        flag_prefix: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    };

    match model.insert(db).await {
        Ok(created) => Ok(created),
        Err(err) => {
            // Concurrent bootstrap: unique system_key (or fixed PK) race → re-select.
            if let Some(existing) = find_practice_jeopardy_event(db).await? {
                Ok(existing)
            } else {
                Err(err)
            }
        }
    }
}
