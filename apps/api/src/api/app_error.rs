//! 统一应用错误类型。

use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use sea_orm::DbErr;
use thiserror::Error;

use super::response::UniResponse;
use crate::modules::event::awd::AwdError;

/// 统一应用错误，带结构化 HTTP 响应。
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Authentication required")]
    Unauthorized,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn code(&self) -> i32 {
        match self {
            AppError::Database(_) => 500,
            AppError::NotFound(_) => 404,
            AppError::BadRequest(_) => 400,
            AppError::Unauthorized => 401,
            AppError::Forbidden(_) => 403,
            AppError::Conflict(_) => 409,
            AppError::InvalidState(_) => 400,
            AppError::Validation(_) => 400,
            AppError::Internal(_) => 500,
        }
    }

    pub fn to_response(&self) -> UniResponse<()> {
        UniResponse::err(self.code(), self.to_string())
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(self.to_response())
    }

    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::InvalidState(_) => StatusCode::BAD_REQUEST,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<DbErr> for AppError {
    fn from(value: DbErr) -> Self {
        AppError::Database(value.to_string())
    }
}

impl From<AwdError> for AppError {
    fn from(value: AwdError) -> Self {
        match value {
            AwdError::NotFound(m) => AppError::NotFound(m),
            AwdError::Forbidden(m) => AppError::Forbidden(m),
            AwdError::Validation(m) => AppError::Validation(m),
            AwdError::InvalidState(m) => AppError::InvalidState(m),
            AwdError::Conflict(m) => AppError::Conflict(m),
            AwdError::Database(m) => AppError::Database(m),
            AwdError::Network(m) => AppError::Internal(format!("Network: {m}")),
            AwdError::PoolExhausted(m) => AppError::Conflict(m),
            AwdError::NetworkLocked(m) => AppError::InvalidState(m),
            AwdError::NetworkOverlap(m) => AppError::Conflict(m),
            AwdError::Docker(m) => AppError::Internal(format!("Docker: {m}")),
            AwdError::Crypto(m) => AppError::Internal(format!("Crypto: {m}")),
            AwdError::Internal(m) => AppError::Internal(m),
        }
    }
}

/// 处理器结果类型（名称保留以稳定调用点）。
pub type UniResult<T> = Result<UniResponse<T>, AppError>;

impl<T> From<UniResponse<T>> for Result<UniResponse<T>, AppError> {
    fn from(resp: UniResponse<T>) -> Self {
        Ok(resp)
    }
}

impl<T> From<AppError> for Result<UniResponse<T>, AppError> {
    fn from(err: AppError) -> Self {
        Err(err)
    }
}
