//! AWD tables — applies `src/sql/awd/01` … `06` in order.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const AWD_SQL: &[&str] = &[
    include_str!("../../src/sql/awd/01-awd-core.sql"),
    include_str!("../../src/sql/awd/02-awd-wireguard.sql"),
    include_str!("../../src/sql/awd/03-awd-rounds-flags-scores.sql"),
    include_str!("../../src/sql/awd/04-awd-judge-reset-ban.sql"),
    include_str!("../../src/sql/awd/05-awd-precheck-runtime.sql"),
    include_str!("../../src/sql/awd/06-awd-indexes.sql"),
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        for sql in AWD_SQL {
            conn.execute_unprepared(sql).await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration(
            "down is not supported for AWD schema".into(),
        ))
    }
}
