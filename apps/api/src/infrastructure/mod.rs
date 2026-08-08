//! Infrastructure adapters: database, Docker, object storage, realtime, audit, settings.

use actix_web::web;
use bollard::Docker;
use sea_orm::DbConn;

pub mod audit;
pub mod database;
pub mod docker;
pub mod logging;
pub mod ratelimit;
pub mod realtime;
pub mod settings;
pub mod storage;

pub use logging::{LogService, WebLog};
pub use settings::{get_setting, seed_default_settings};

/// Actix `web::Data` handle for the SeaORM connection.
pub type WebDb = web::Data<DbConn>;
/// Actix `web::Data` handle for the Bollard Docker client.
pub type WebDocker = web::Data<Docker>;
/// Actix `web::Data` handle for the S3-compatible storage client (RustFS).
pub type WebRustfs = web::Data<aws_sdk_s3::Client>;
