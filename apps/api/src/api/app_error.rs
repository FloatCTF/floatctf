//! 统一应用错误类型。
//!
//! ⚠️ 分层现状（风险清单 #6）：本模块在顶部 `use crate::modules::event::...` 并为
//! `AwdError` / `GameboxError` / `AwdpError` 各实现了一个 `From`；而所有业务模块又通过
//! `crate::api::prelude::*` 反向依赖 api 层 —— 于是 api ↔ modules 形成环。这不是编译
//! 错误，但「api 是纯横切层」已被打破：每新增一个赛制，其错误类型都要回到这里注册，
//! api 层因此成为所有域的耦合中心，想把某个域抽成独立 crate 会直接卡住。
//!
//! 目标方向（新代码请照此办理，**不要**继续往本文件加 `use crate::modules::...`）：
//! - 域内错误在域内实现到 `AppError` 的转换（模块侧 impl `From`）；
//! - `AppError` 只保留真正横切的变体（Validation / NotFound / Unauthorized / Conflict…）；
//! - 新增域时优先 `AppError::from_xxx(...)` 这类显式构造。
//!
//! 排查建议：看到某个域错误没被映射成预期状态码时，先确认它的 `From` 实现落在哪一层。

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
        UniResponse::err(self.code(), self.client_message())
    }

    /// 面向客户端的错误文案。
    ///
    /// `Database` 携带的是原始驱动/SQL 文本（含约束名、表结构），既不可读也不该外泄：
    /// 详情只写服务端日志，客户端统一收到通用提示。其余变体是代码作者编写、面向用户的
    /// 文案，原样返回。
    fn client_message(&self) -> String {
        // 只取业务文案本身：枚举 Display 前缀（"Not found: "/"Forbidden: "/"Validation error: "）
        // 是给日志看的英文标签，混进用户提示会变成中英夹杂。
        match self {
            AppError::Database(detail) => {
                tracing::error!(error = %detail, "database error");
                "服务器内部错误，请稍后重试或联系管理员".to_string()
            }
            AppError::Unauthorized => "登录状态已失效，请重新登录".to_string(),
            AppError::NotFound(message) => fallback_message(message, "请求的资源不存在或已被删除"),
            AppError::Forbidden(message) => fallback_message(message, "没有权限执行该操作"),
            AppError::Conflict(message) => fallback_message(message, "操作冲突，请刷新后重试"),
            AppError::BadRequest(message)
            | AppError::Validation(message)
            | AppError::InvalidState(message) => {
                fallback_message(message, "请求参数有误，请检查后重试")
            }
            AppError::Internal(message) => message.clone(),
        }
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

impl From<crate::modules::gamebox::GameboxError> for AppError {
    fn from(value: crate::modules::gamebox::GameboxError) -> Self {
        match value {
            crate::modules::gamebox::GameboxError::NotFound(m) => AppError::NotFound(m),
            crate::modules::gamebox::GameboxError::Validation(m) => AppError::Validation(m),
            crate::modules::gamebox::GameboxError::Conflict(m) => AppError::Conflict(m),
            crate::modules::gamebox::GameboxError::Database(m) => AppError::Database(m),
            crate::modules::gamebox::GameboxError::Docker(m) => {
                AppError::Internal(format!("Docker: {m}"))
            }
            crate::modules::gamebox::GameboxError::Internal(m) => AppError::Internal(m),
        }
    }
}

impl From<crate::modules::event::awdp::AwdpError> for AppError {
    fn from(value: crate::modules::event::awdp::AwdpError) -> Self {
        match value {
            crate::modules::event::awdp::AwdpError::NotFound(m) => AppError::NotFound(m),
            crate::modules::event::awdp::AwdpError::Forbidden(m) => AppError::Forbidden(m),
            crate::modules::event::awdp::AwdpError::Validation(m) => AppError::Validation(m),
            crate::modules::event::awdp::AwdpError::InvalidState(m) => AppError::InvalidState(m),
            crate::modules::event::awdp::AwdpError::Conflict(m) => AppError::Conflict(m),
            crate::modules::event::awdp::AwdpError::Database(m) => AppError::Database(m),
            crate::modules::event::awdp::AwdpError::Docker(m) => {
                AppError::Internal(format!("Docker: {m}"))
            }
            crate::modules::event::awdp::AwdpError::Network(m) => {
                AppError::Internal(format!("Network: {m}"))
            }
            crate::modules::event::awdp::AwdpError::Internal(m) => AppError::Internal(m),
            crate::modules::event::awdp::AwdpError::Retry => AppError::Internal("retry".into()),
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

/// 业务文案为空时回落为通用中文提示。
fn fallback_message(message: &str, fallback: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

/// 把唯一约束冲突映射成精确的中文冲突提示。
///
/// `users` 同时有 `users_username_key` 与 `users_nickname_key` 两个唯一约束，
/// 一律提示「用户名已被占用」会误导（实测：仅昵称重复也会报用户名）。
pub fn unique_violation_message(message: &str) -> AppError {
    let lower = message.to_ascii_lowercase();
    let msg = if lower.contains("users_nickname_key") || lower.contains("nickname") {
        "昵称已被占用"
    } else if lower.contains("users_username_key") || lower.contains("username") {
        "用户名已被占用"
    } else if lower.contains("email") {
        "邮箱已被注册"
    } else if lower.contains("team") {
        "队伍名称已被占用"
    } else {
        "该名称已被占用"
    };
    AppError::Conflict(msg.into())
}

/// 判断数据库错误是否为唯一约束冲突（SeaORM 不暴露 PG 错误码，只能按消息匹配）。
///
/// 唯一约束冲突属于业务冲突，必须映射成 4xx 中文提示；直接冒泡会变成 500
/// 并把约束名等内部信息回传给用户。
pub fn is_unique_violation(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("duplicate key")
        || lower.contains("unique constraint")
        || lower.contains("23505")
}
