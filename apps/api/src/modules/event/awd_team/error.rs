use thiserror::Error;

#[derive(Debug, Error)]
pub enum AwdError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Docker error: {0}")]
    Docker(String),
    #[error("Crypto error: {0}")]
    Crypto(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type AwdResult<T> = Result<T, AwdError>;
