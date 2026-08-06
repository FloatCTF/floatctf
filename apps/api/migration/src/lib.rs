//! FloatCTF schema migrator.
//!
//! SQL sources remain under `../src/sql/{init,awd,update}/` and are applied via
//! `include_str!` + `execute_unprepared` so semantics stay identical to the
//! historical Docker init / manual AWD scripts.

pub use sea_orm_migration::prelude::*;

mod m0001_base_schema;
mod m0002_base_extensions;
mod m0100_awd_schema;
mod m0101_scheduler_retry;
pub mod schema_check;

use sea_orm::{Database, DbBackend, Statement};
use std::time::SystemTime;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m0001_base_schema::Migration),
            Box::new(m0002_base_extensions::Migration),
            Box::new(m0100_awd_schema::Migration),
            Box::new(m0101_scheduler_retry::Migration),
        ]
    }
}

/// Run all pending migrations against `DATABASE_URL`.
pub async fn migrate_up(database_url: &str) -> Result<(), DbErr> {
    let db = Database::connect(database_url).await?;
    Migrator::up(&db, None).await
}

/// Print applied / pending migration status.
pub async fn migrate_status(database_url: &str) -> Result<(), DbErr> {
    let db = Database::connect(database_url).await?;
    Migrator::status(&db).await
}

/// Mark all migration versions as applied **without** running DDL.
///
/// Use on existing databases that already have the full schema (Docker init +
/// manual AWD, etc.). Always run [`schema_check::check_schema`] first and only
/// baseline when the report is `ok()`.
///
/// Idempotent: already-recorded versions are skipped. Uses SeaORM
/// `MigratorTrait::install` + `get_pending_migrations`, then inserts into
/// `seaql_migrations` (same shape as sea-orm-migration's internal up path).
pub async fn migrate_baseline(database_url: &str) -> Result<BaselineReport, DbErr> {
    let db = Database::connect(database_url).await?;
    Migrator::install(&db).await?;

    let pending = Migrator::get_pending_migrations(&db).await?;
    let mut inserted = Vec::new();
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH")
        .as_secs() as i64;

    for m in pending {
        let version = m.name().to_owned();
        // Escape single quotes for safety (version names are module identifiers).
        let safe = version.replace('\'', "''");
        db.execute(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "INSERT INTO seaql_migrations (version, applied_at) VALUES ('{safe}', {now}) \
                 ON CONFLICT (version) DO NOTHING"
            ),
        ))
        .await?;
        inserted.push(version);
    }

    Ok(BaselineReport { inserted })
}

#[derive(Debug, Default)]
pub struct BaselineReport {
    pub inserted: Vec<String>,
}

impl BaselineReport {
    pub fn summary(&self) -> String {
        if self.inserted.is_empty() {
            "baseline: no pending versions (already recorded)".into()
        } else {
            format!(
                "baseline: marked {} migration(s) applied: {:?}",
                self.inserted.len(),
                self.inserted
            )
        }
    }
}

/// Owned list of migration version names in apply order.
pub fn migration_version_names() -> Vec<String> {
    Migrator::get_migration_files()
        .into_iter()
        .map(|m| m.name().to_string())
        .collect()
}
