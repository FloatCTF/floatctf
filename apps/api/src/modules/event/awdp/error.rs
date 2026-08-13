//! AWDP 错误类型（与 AwdError 形状一致，但独立）。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AwdpError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Docker error: {0}")]
    Docker(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type AwdpResult<T> = Result<T, AwdpError>;

impl From<crate::modules::gamebox::GameboxError> for AwdpError {
    fn from(value: crate::modules::gamebox::GameboxError) -> Self {
        match value {
            crate::modules::gamebox::GameboxError::NotFound(m) => AwdpError::NotFound(m),
            crate::modules::gamebox::GameboxError::Validation(m) => AwdpError::Validation(m),
            crate::modules::gamebox::GameboxError::Conflict(m) => AwdpError::Conflict(m),
            crate::modules::gamebox::GameboxError::Database(m) => AwdpError::Database(m),
            crate::modules::gamebox::GameboxError::Docker(m) => AwdpError::Docker(m),
            crate::modules::gamebox::GameboxError::Internal(m) => AwdpError::Internal(m),
        }
    }
}
