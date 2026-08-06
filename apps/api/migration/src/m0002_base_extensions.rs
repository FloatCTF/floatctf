//! Indexes, triggers, and seed data from `src/sql/init/{02,03,04}-*.sql`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared(include_str!("../../src/sql/init/02-index.sql"))
            .await?;
        conn.execute_unprepared(include_str!("../../src/sql/init/03-triggers.sql"))
            .await?;
        // Seed sysadmin row (idempotent if already applied via migration history).
        conn.execute_unprepared(include_str!("../../src/sql/init/04-init.sql"))
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration(
            "down is not supported for base extensions".into(),
        ))
    }
}
