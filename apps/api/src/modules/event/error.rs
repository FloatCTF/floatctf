//! 赛事模块统一错误类型（供处理器使用的窄接口）。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unsupported for event type: {0}")]
    Unsupported(String),
    #[error("database: {0}")]
    Database(String),
    #[error("internal: {0}")]
    Internal(String),
}

pub type EventResult<T> = Result<T, EventError>;

impl From<sea_orm::DbErr> for EventError {
    fn from(value: sea_orm::DbErr) -> Self {
        EventError::Database(value.to_string())
    }
}
