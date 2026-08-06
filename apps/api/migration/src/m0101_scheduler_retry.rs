//! Scheduler reliability columns (attempt/timeout/lock).
//!
//! SQL source: `src/sql/update/01-scheduler-retry.sql` (incremental updates).
//! Entity must be regenerated with sea-orm-cli after this migration — never hand-edit.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!(
                "../../src/sql/update/01-scheduler-retry.sql"
            ))
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration(
            "down not supported for scheduler retry columns".into(),
        ))
    }
}
