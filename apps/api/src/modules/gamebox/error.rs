//! GameBox 库共享错误类型。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GameboxError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Docker error: {0}")]
    Docker(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type GameboxResult<T> = Result<T, GameboxError>;
