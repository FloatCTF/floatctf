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
    #[error("Network pool exhausted: {0}")]
    PoolExhausted(String),
    #[error("Network allocation locked: {0}")]
    NetworkLocked(String),
    #[error("Network overlap: {0}")]
    NetworkOverlap(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type AwdResult<T> = Result<T, AwdError>;

impl From<crate::modules::gamebox::GameboxError> for AwdError {
    fn from(value: crate::modules::gamebox::GameboxError) -> Self {
        match value {
            crate::modules::gamebox::GameboxError::NotFound(m) => AwdError::NotFound(m),
            crate::modules::gamebox::GameboxError::Validation(m) => AwdError::Validation(m),
            crate::modules::gamebox::GameboxError::Conflict(m) => AwdError::Conflict(m),
            crate::modules::gamebox::GameboxError::Database(m) => AwdError::Database(m),
            crate::modules::gamebox::GameboxError::Docker(m) => AwdError::Docker(m),
            crate::modules::gamebox::GameboxError::Internal(m) => AwdError::Internal(m),
        }
    }
}
