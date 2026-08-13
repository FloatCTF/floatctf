//! AWDP 内部路由（练习 JudgeServer 评估 Pull/Lease + 数据面回调）。
//!
//! 路径（bootstrap 顶层注册，无 /api 前缀）：
//! - `POST /internal/awdp/judge/jobs/claim` —— 领取评估作业
//! - `POST /internal/awdp/judge/jobs/{id}/heartbeat` —— 延长 lease
//! - `POST /internal/awdp/judge/jobs/{id}/result` —— 提交结果
//!
//! 鉴权：Bearer 令牌 = 平台 Secret HKDF 派生的练习 Judge 令牌（与部署容器注入的
//! INTERNAL_TOKEN 一致，两侧各自派生/比较，不落库不落日志）。
//!
//! 旧的 practice judge callback 路由（push /batch 回调）已随 push 流程移除（plan §61）。

use actix_web::{
    FromRequest, HttpRequest,
    dev::Payload,
    web::{self, Json},
};
use std::future::Future;
use std::pin::Pin;

use crate::{
    api::{AppError, UniResponse, UniResult, prelude::*},
    modules::event::awdp::domain::judge::practice_judge_token,
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

/// POST /internal/awdp/judge/jobs/claim
/// JudgeServer 主动领取评估作业（Pull + Lease）。
#[post("/internal/awdp/judge/jobs/claim")]
pub async fn judge_jobs_claim(
    _auth: PracticeJudgeInternalAuth,
    ctx: ReqCtx,
    body: Json<crate::modules::event::awdp::service::judge_worker::ClaimRequest>,
) -> UniResult<crate::modules::event::awdp::service::judge_worker::ClaimResponse> {
    use crate::modules::event::awdp::service::judge_worker;
    let req = body.into_inner();
    if req.worker_id.trim().is_empty() {
        return Err(AppError::Validation("worker_id is required".into()));
    }
    if req.capacity == 0 || req.capacity > 64 {
        return Err(AppError::Validation("capacity must be in [1, 64]".into()));
    }
    let resp = judge_worker::claim_jobs(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        &req.worker_id,
        req.capacity,
        ctx.config.awdp.eval_lease_duration_secs,
        ctx.config.awdp.eval_max_attempts,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(Some(resp)).into()
}

/// POST /internal/awdp/judge/jobs/{id}/heartbeat
/// JudgeServer 延长 lease。
#[post("/internal/awdp/judge/jobs/{id}/heartbeat")]
pub async fn judge_jobs_heartbeat(
    _auth: PracticeJudgeInternalAuth,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: Json<crate::modules::event::awdp::service::judge_worker::HeartbeatRequest>,
) -> UniResult<()> {
    use crate::modules::event::awdp::service::judge_worker;
    let evaluation_id = path.into_inner();
    let req = body.into_inner();
    let outcome = judge_worker::heartbeat_job(
        ctx.db.get_ref(),
        evaluation_id,
        &req,
        ctx.config.awdp.eval_lease_duration_secs,
    )
    .await
    .map_err(AppError::from)?;
    match outcome {
        crate::modules::event::awdp::repo::evaluation_repo::HeartbeatOutcome::Ok => {
            UniResponse::ok_none().into()
        }
        crate::modules::event::awdp::repo::evaluation_repo::HeartbeatOutcome::NoLease => {
            Err(AppError::Conflict("no valid lease for evaluation".into()))
        }
    }
}

/// POST /internal/awdp/judge/jobs/{id}/result
/// JudgeServer 提交评估结果（stale 结果 409 拒绝）。
#[post("/internal/awdp/judge/jobs/{id}/result")]
pub async fn judge_jobs_result(
    _auth: PracticeJudgeInternalAuth,
    ctx: ReqCtx,
    _path: web::Path<Uuid>,
    body: Json<crate::modules::event::awdp::service::judge_worker::ResultRequest>,
) -> UniResult<()> {
    use crate::modules::event::awdp::service::judge_worker;
    let req = body.into_inner();
    judge_worker::record_result(ctx.db.get_ref(), &req, ctx.config.awdp.eval_max_attempts)
        .await
        .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// POST /internal/awdp/flag/resolve
/// JudgeServer `/flag` 转发的 Break flag 解析（source_ip 由 JudgeServer 从 TCP peer 读取）。
/// 仅 Break 阶段返回；未知 source / 非 running / 非 Break → 403/409。
#[post("/internal/awdp/flag/resolve")]
pub async fn resolve_break_flag(
    _auth: PracticeJudgeInternalAuth,
    ctx: ReqCtx,
    body: Json<crate::modules::event::awdp::service::judge_worker::ResolveFlagRequest>,
) -> UniResult<String> {
    use crate::modules::event::awdp::service::break_service;
    let req = body.into_inner();
    if req.source_ip.trim().is_empty() {
        return Err(AppError::Validation("source_ip is required".into()));
    }
    let flag = break_service::resolve_break_flag(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        req.source_ip.trim(),
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(Some(flag)).into()
}

/// POST /internal/awdp/proof/consume
/// JudgeServer `/proof/{token}` 转发的一次性 proof 消费（target-bound 验证 + 原子置位）。
#[post("/internal/awdp/proof/consume")]
pub async fn consume_eval_proof(
    _auth: PracticeJudgeInternalAuth,
    ctx: ReqCtx,
    body: Json<crate::modules::event::awdp::service::judge_worker::ConsumeProofRequest>,
) -> UniResult<()> {
    use crate::modules::event::awdp::service::judge_worker;
    let req = body.into_inner();
    if req.token.trim().is_empty() || req.source_ip.trim().is_empty() {
        return Err(AppError::Validation(
            "token and source_ip are required".into(),
        ));
    }
    judge_worker::consume_proof(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        req.token.trim(),
        req.source_ip.trim(),
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// 注册内部路由（bootstrap 顶层，与 AWD internal 同风格）。
pub fn internal_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(judge_jobs_claim);
    cfg.service(judge_jobs_heartbeat);
    cfg.service(judge_jobs_result);
    cfg.service(resolve_break_flag);
    cfg.service(consume_eval_proof);
}
