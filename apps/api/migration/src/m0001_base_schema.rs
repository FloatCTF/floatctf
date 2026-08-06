//! Base tables and enums from `src/sql/init/01-up.sql`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Path relative to this file: migration/src -> ../../src/sql/init
        let sql = include_str!("../../src/sql/init/01-up.sql");
        manager.get_connection().execute_unprepared(sql).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Destructive full tear-down is intentionally not automated in stage 1.
        // Use src/sql/down.sql manually in development only.
        Err(DbErr::Migration(
            "down is not supported for base schema; use src/sql/down.sql carefully".into(),
        ))
    }
}
