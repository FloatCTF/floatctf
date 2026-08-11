//! 基础设施适配：数据库、Docker、对象存储、实时推送、审计、动态设置。

use actix_web::web;
use bollard::Docker;
use sea_orm::DbConn;

pub mod audit;
pub mod database;
pub mod docker;
pub mod logging;
pub mod package;
pub mod ratelimit;
pub mod realtime;
pub mod settings;
pub mod storage;

pub use logging::{LogService, WebLog};
pub use settings::{get_setting, seed_default_settings};

/// SeaORM 连接的 Actix `web::Data` 句柄。
pub type WebDb = web::Data<DbConn>;
/// Bollard Docker 客户端的 Actix `web::Data` 句柄。
pub type WebDocker = web::Data<Docker>;
/// S3 兼容存储客户端（RustFS）的 Actix `web::Data` 句柄。
pub type WebRustfs = web::Data<aws_sdk_s3::Client>;
