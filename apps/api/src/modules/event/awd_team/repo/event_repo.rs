//! AWD event repository — single style: struct methods + free-function adapters.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::entity::{awd_events, sea_orm_active_enums::AwdEventStatus};

/// Instantiated repository (preferred for new call sites).
pub struct AwdEventRepository {
    db: DatabaseConnection,
}

impl AwdEventRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<awd_events::Model>, sea_orm::DbErr> {
        find_by_event_id(&self.db, event_id).await
    }

    pub async fn find_active_events(&self) -> Result<Vec<awd_events::Model>, sea_orm::DbErr> {
        find_active_events(&self.db).await
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: AwdEventStatus,
    ) -> Result<(), sea_orm::DbErr> {
        update_status(&self.db, id, status).await
    }

    pub async fn update_phase(
        &self,
        id: Uuid,
        phase: crate::entity::sea_orm_active_enums::AwdPhase,
    ) -> Result<(), sea_orm::DbErr> {
        update_phase(&self.db, id, phase).await
    }

    pub async fn mark_verified(&self, id: Uuid, revision: &str) -> Result<(), sea_orm::DbErr> {
        mark_verified(&self.db, id, revision).await
    }

    pub async fn clear_verified(&self, id: Uuid) -> Result<(), sea_orm::DbErr> {
        clear_verified(&self.db, id).await
    }
}

/// Backward-compatible name used by older call sites.
pub type EventRepo<'a> = AwdEventRepositoryRef<'a>;

/// Borrowed repository (avoids cloning `DatabaseConnection` when not needed).
pub struct AwdEventRepositoryRef<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> AwdEventRepositoryRef<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_by_event_id(
        &self,
        event_id: Uuid,
    ) -> Result<Option<awd_events::Model>, sea_orm::DbErr> {
        find_by_event_id(self.db, event_id).await
    }
}

// ── Connection-generic helpers (usable inside transactions) ──

pub async fn find_by_event_id<C: ConnectionTrait + Send>(
    db: &C,
    event_id: Uuid,
) -> Result<Option<awd_events::Model>, sea_orm::DbErr> {
    awd_events::Entity::find()
        .filter(awd_events::Column::EventId.eq(event_id))
        .one(db)
        .await
}

pub async fn find_active_events<C: ConnectionTrait + Send>(
    db: &C,
) -> Result<Vec<awd_events::Model>, sea_orm::DbErr> {
    awd_events::Entity::find()
        .filter(awd_events::Column::Status.is_in([AwdEventStatus::Running, AwdEventStatus::Paused]))
        .all(db)
        .await
}

pub async fn update_status<C: ConnectionTrait + Send>(
    db: &C,
    id: Uuid,
    status: AwdEventStatus,
) -> Result<(), sea_orm::DbErr> {
    let active: awd_events::ActiveModel = awd_events::ActiveModel {
        id: Set(id),
        status: Set(status),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

pub async fn update_phase<C: ConnectionTrait + Send>(
    db: &C,
    id: Uuid,
    phase: crate::entity::sea_orm_active_enums::AwdPhase,
) -> Result<(), sea_orm::DbErr> {
    let active: awd_events::ActiveModel = awd_events::ActiveModel {
        id: Set(id),
        phase: Set(phase),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

pub async fn mark_verified<C: ConnectionTrait + Send>(
    db: &C,
    id: Uuid,
    revision: &str,
) -> Result<(), sea_orm::DbErr> {
    use chrono::Utc;
    let active: awd_events::ActiveModel = awd_events::ActiveModel {
        id: Set(id),
        verified_at: Set(Some(Utc::now().into())),
        verified_revision: Set(Some(revision.to_string())),
        status: Set(AwdEventStatus::Verified),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}

pub async fn clear_verified<C: ConnectionTrait + Send>(
    db: &C,
    id: Uuid,
) -> Result<(), sea_orm::DbErr> {
    let active: awd_events::ActiveModel = awd_events::ActiveModel {
        id: Set(id),
        verified_at: Set(None),
        verified_revision: Set(None),
        status: Set(AwdEventStatus::Configuring),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}
