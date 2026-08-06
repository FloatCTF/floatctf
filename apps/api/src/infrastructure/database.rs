//! Database connection initialization.

use anyhow::Result;
use sea_orm::DbConn;
use tracing::info;

use crate::core::config::DatabaseConfig;

pub async fn connect(config: &DatabaseConfig) -> Result<DbConn> {
    let db = sea_orm::Database::connect(config.url.expose()).await?;
    db.ping().await?;
    info!("Database connected OK");
    Ok(db)
}
