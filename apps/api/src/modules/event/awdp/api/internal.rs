//! AWDP 内部路由（练习 JudgeServer 回调）。
//!
//! 路径：`POST /internal/awdp/practice/judge/callback`（bootstrap 顶层注册，无 /api 前缀）。
//! 鉴权：Bearer 令牌 = 平台 Secret HKDF 派生的练习 Judge 令牌（与部署容器注入的
//! INTERNAL_TOKEN 一致，两侧各自派生/比较，不落库不落日志）。

use actix_web::{
    FromRequest, HttpRequest,
    dev::Payload,
    web::{self, Json},
};
use std::future::Future;
use std::pin::Pin;

use crate::{
    api::{AppError, UniResponse, UniResult, prelude::*},
    modules::event::awdp::{
        domain::judge::practice_judge_token,
        service::practice_judge::{self, JudgeCallbackRequest},
    },
};

/// 练习 Judge 内部调用鉴权提取器。
pub struct PracticeJudgeInternalAuth {
    _private: (),
}

#[derive(Debug)]
pub enum PracticeJudgeAuthError {
    MissingToken,
    InvalidToken,
    ConfigMissing,
}

impl std::fmt::Display for PracticeJudgeAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::MissingToken => "Missing or invalid Authorization header",
                Self::InvalidToken => "Unauthorized",
                Self::ConfigMissing => "AppConfig not available",
            }
        )
    }
}

impl actix_web::ResponseError for PracticeJudgeAuthError {
    fn error_response(&self) -> actix_web::HttpResponse {
        actix_web::HttpResponse::Unauthorized().json(serde_json::json!({
            "code": 401,
            "message": self.to_string(),
        }))
    }

    fn status_code(&self) -> actix_web::http::StatusCode {
        actix_web::http::StatusCode::UNAUTHORIZED
    }
}

impl FromRequest for PracticeJudgeInternalAuth {
    type Error = PracticeJudgeAuthError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let token = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_string);
        let config = req
            .app_data::<actix_web::web::Data<crate::bootstrap::AppState>>()
            .map(|s| s.config.clone());

        Box::pin(async move {
            let token = token.ok_or(PracticeJudgeAuthError::MissingToken)?;
            let config = config.ok_or(PracticeJudgeAuthError::ConfigMissing)?;
            let expected = practice_judge_token(config.auth.jwt_secret.expose().as_bytes());
            if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
                Ok(Self { _private: () })
            } else {
                Err(PracticeJudgeAuthError::InvalidToken)
            }
        })
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (l, r)| diff | (l ^ r))
        == 0
}

/// POST /internal/awdp/practice/judge/callback
/// 由练习 JudgeServer 在每个任务完成后调用（带内部令牌）。
#[post("/internal/awdp/practice/judge/callback")]
pub async fn practice_judge_callback(
    _auth: PracticeJudgeInternalAuth,
    ctx: ReqCtx,
    body: Json<JudgeCallbackRequest>,
) -> UniResult<()> {
    let cb = body.into_inner();
    practice_judge::record_callback(ctx.db.get_ref(), &cb)
        .await
        .map_err(|e: crate::modules::event::awdp::AwdpError| match e {
            crate::modules::event::awdp::AwdpError::Validation(m) => AppError::Validation(m),
            crate::modules::event::awdp::AwdpError::NotFound(m) => AppError::NotFound(m),
            other => AppError::Internal(other.to_string()),
        })?;
    UniResponse::ok_none().into()
}

/// 注册内部路由（bootstrap 顶层，与 AWD internal 同风格）。
pub fn internal_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(practice_judge_callback);
}
