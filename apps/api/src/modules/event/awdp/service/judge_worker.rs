//! AWDP 评估 Pull + Lease worker 服务：claim 负载构建 / heartbeat / result 落库计分。
//!
//! 职责（plan §12-§18）：
//! - `claim_jobs`：调用 evaluation_repo::claim_jobs 领取，并为每条 job 解析完整负载
//!   （instance/run/gamebox/target_ip/healthchecks/脚本）；worker 仅消费 official
//!   （manual = Test Check 同步流程独占，含 exploit 诊断，不走 worker）；
//! - `heartbeat`：延长 lease；
//! - `record_result`：验证 lease + attempt + runtime_generation 后写终态；official+patched
//!   由平台侧幂等计分（awdp_score_events idempotency_key）；stale 结果拒绝（409）。

use bollard::Docker;
use fcmc::{ContainerRuntime, DockerContainerRuntime};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::{AwdpEvaluationKind, AwdpEvaluationStatus};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::fix_idempotency_key,
    repo::{evaluation_repo, event_gamebox_repo, instance_repo, run_repo, score_repo},
};

/// claim 请求（JudgeServer → FloatCTF）。
#[derive(Debug, Deserialize)]
pub struct ClaimRequest {
    pub worker_id: String,
    /// 本次最多领取数量（bounded concurrency）。
    pub capacity: u64,
    /// 请求所属赛事（赛事专属 JudgeServer 携带自身 EVENT_ID；平台进程内 worker 可省略）。
    #[serde(default)]
    pub event_id: Option<Uuid>,
}

/// 单条 job 负载（仅包含实际评估所需内容；不返回用户隐私/JWT/DB 凭据/无关 GameBox）。
#[derive(Debug, Clone, Serialize)]
pub struct ClaimJobDto {
    pub evaluation_id: Uuid,
    pub attempt: i32,
    pub lease_token: String,
    pub kind: String,
    pub run_id: Uuid,
    pub round_id: Option<Uuid>,
    pub instance_id: Uuid,
    pub runtime_generation: i64,
    /// 容器内网 IP（data plane 目标；容器未运行时为 None）。
    pub target_ip: Option<String>,
    /// GameBox healthcheck 声明（container_port 语义，JudgeServer 打 target_ip:port）。
    pub healthchecks: Vec<serde_json::Value>,
    pub judge_script: Option<String>,
    /// 仅 official 包含（manual 恒 None）。
    pub exploit_script: Option<String>,
    /// official 一次性 target-bound proof URL（exploit 可通过环境变量 FLOATCTF_PROOF_URL 使用）。
    pub proof_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClaimResponse {
    pub lease_seconds: i64,
    pub jobs: Vec<ClaimJobDto>,
}

/// heartbeat 请求。
#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub lease_token: String,
    pub worker_id: String,
}

/// Break flag 解析请求（JudgeServer `/flag` 转发，source_ip = 真实 TCP peer）。
#[derive(Debug, Deserialize)]
pub struct ResolveFlagRequest {
    /// 请求所属赛事（JudgeServer 携带自身 EVENT_ID；决定解析哪个 data 网络）。
    pub event_id: Uuid,
    pub source_ip: String,
}

/// proof 消费请求（JudgeServer `/proof/{token}` 转发）。
#[derive(Debug, Deserialize)]
pub struct ConsumeProofRequest {
    pub token: String,
    /// 请求所属赛事（JudgeServer 携带自身 EVENT_ID；决定解析哪个 data 网络）。
    pub event_id: Uuid,
    pub source_ip: String,
}

fn sha256_hex(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

fn new_proof_token() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

/// 为 official 评估创建一次性 target-bound proof（plan §27）。
/// 返回明文 token（仅出现在 job payload；DB 只存 sha256）。
pub async fn create_proof_for_job(
    db: &DatabaseConnection,
    evaluation_id: Uuid,
    instance_id: Uuid,
    runtime_generation: i64,
    lease_seconds: i64,
) -> AwdpResult<String> {
    use crate::entity::awdp_evaluation_proofs;
    use sea_orm::ActiveModelTrait;
    use sea_orm::ActiveValue::Set;
    let token = new_proof_token();
    let now = chrono::Utc::now();
    awdp_evaluation_proofs::ActiveModel {
        id: Set(Uuid::new_v4()),
        evaluation_id: Set(evaluation_id),
        token_hash: Set(sha256_hex(&token)),
        target_instance_id: Set(instance_id),
        runtime_generation: Set(runtime_generation),
        expires_at: Set((now + chrono::Duration::seconds(lease_seconds.max(60) * 3)).into()),
        consumed_at: Set(None),
        created_at: Set(now.into()),
    }
    .insert(db)
    .await
    .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(token)
}

/// 消费 proof（plan §28）：验证 token 有效 / 未过期 / 未消费 / source IP == 目标实例当前
/// 容器 IP / runtime_generation 匹配 / 评估 running → 原子置 consumed_at（一次性）。
pub async fn consume_proof(
    db: &DatabaseConnection,
    docker: &Docker,
    event_id: Uuid,
    token: &str,
    source_ip: &str,
) -> AwdpResult<()> {
    use crate::entity::sea_orm_active_enums::AwdpEvaluationStatus;
    use crate::entity::{awdp_evaluation_proofs, awdp_evaluations};
    use crate::modules::event::awdp::service::network_resolve;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

    let now = chrono::Utc::now();
    let proof = awdp_evaluation_proofs::Entity::find()
        .filter(awdp_evaluation_proofs::Column::TokenHash.eq(sha256_hex(token)))
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::Forbidden("invalid proof token".into()))?;

    if proof.consumed_at.is_some() {
        return Err(AwdpError::Conflict("proof already consumed".into()));
    }
    if proof.expires_at.with_timezone(&chrono::Utc) <= now {
        return Err(AwdpError::Conflict("proof expired".into()));
    }

    // source IP 必须等于目标实例当前容器 IP（真实网络附着）。
    let (instance, _ext) =
        network_resolve::resolve_instance_by_network_ip(db, docker, event_id, source_ip)
            .await
            .map_err(|_| AwdpError::Forbidden("source ip mismatch".into()))?;
    if instance.id != proof.target_instance_id {
        return Err(AwdpError::Forbidden("source ip mismatch".into()));
    }
    if instance.runtime_generation != proof.runtime_generation {
        return Err(AwdpError::Conflict("runtime generation mismatch".into()));
    }

    // 评估必须仍在执行（running）。
    let ev = awdp_evaluations::Entity::find_by_id(proof.evaluation_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::Conflict("evaluation not found".into()))?;
    if ev.status != AwdpEvaluationStatus::Running {
        return Err(AwdpError::Conflict(format!(
            "evaluation not running (status={:?})",
            ev.status
        )));
    }

    // 原子消费（条件 UPDATE 防双消费）。
    let res = awdp_evaluation_proofs::Entity::update_many()
        .col_expr(
            awdp_evaluation_proofs::Column::ConsumedAt,
            sea_orm::prelude::Expr::value(now),
        )
        .filter(awdp_evaluation_proofs::Column::Id.eq(proof.id))
        .filter(awdp_evaluation_proofs::Column::ConsumedAt.is_null())
        .exec(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    if res.rows_affected == 0 {
        return Err(AwdpError::Conflict("proof already consumed".into()));
    }
    Ok(())
}

/// result 请求（JudgeServer → FloatCTF）。
#[derive(Debug, Deserialize)]
pub struct ResultRequest {
    pub evaluation_id: Uuid,
    pub worker_id: String,
    pub lease_token: String,
    pub attempt: i32,
    /// claim 时的 runtime_generation；与当前 Instance 不一致 → 409 拒绝。
    pub runtime_generation: i64,
    /// 终态 status（no_patch/service_down/functional_broken/vulnerable/patched/platform_error）。
    pub status: String,
    #[serde(default)]
    pub healthcheck_result: Option<String>,
    #[serde(default)]
    pub judge_result: Option<String>,
    #[serde(default)]
    pub exploit_result: Option<String>,
    #[serde(default)]
    pub stdout_limited: Option<String>,
    #[serde(default)]
    pub stderr_limited: Option<String>,
}

/// claim：领取 + 构建负载（事务领取在 evaluation_repo 内完成）。
///
/// 额外处理：official 评估若本轮无 APPLIED patch（NO_PATCH 短路，plan §22）
/// ——直接以 no_patch 终态结算，不占 worker 资源、不进入 job 负载。
pub async fn claim_jobs(
    db: &DatabaseConnection,
    docker: &Docker,
    worker_id: &str,
    capacity: u64,
    lease_seconds: i64,
    max_attempts: i32,
    event_id: Option<Uuid>,
) -> AwdpResult<ClaimResponse> {
    use crate::modules::event::awdp::repo::patch_repo;
    // JudgeServer 仅消费 official：manual = Test Check 同步流程独占（HTTP 请求内
    // 直接执行，含 exploit 诊断）；worker 领取 manual 会与同步路径双写同一行，
    // 触发 awdp_evaluations_lease_consistency_check 违例。赛事专属 worker 只领本赛事 job。
    let claimed = evaluation_repo::claim_jobs(
        db,
        worker_id,
        capacity,
        lease_seconds,
        max_attempts,
        &[AwdpEvaluationKind::Official],
        event_id,
    )
    .await?;

    let runtime = DockerContainerRuntime::new(docker.clone());
    let mut jobs = Vec::with_capacity(claimed.len());
    for job in claimed {
        let ev = &job.evaluation;
        let (instance, ext) = instance_repo::find_by_instance_id(db, ev.instance_id).await?;
        let run = run_repo::require_by_id(db, ext.run_id).await?;
        let gamebox = event_gamebox_repo::find_gamebox_identity(db, ext.gamebox_id).await?;

        // NO_PATCH 短路：official 且本轮无 APPLIED patch → 直接终态（用本次 lease 结算）。
        if ev.kind == AwdpEvaluationKind::Official {
            if let Some(round_id) = ev.fix_round_id {
                if !patch_repo::has_applied_patch(db, ev.instance_id, round_id).await? {
                    let outcome = evaluation_repo::finish_with_lease(
                        db,
                        ev.id,
                        worker_id,
                        &job.lease_token,
                        job.attempt,
                        AwdpEvaluationStatus::NoPatch,
                        Some("no applied patch this round"),
                        None,
                        None,
                        None,
                        None,
                    )
                    .await?;
                    if outcome == evaluation_repo::FinishOutcome::StaleRejected {
                        tracing::warn!(evaluation_id = %ev.id, "NO_PATCH short-circuit stale");
                    }
                    continue; // 不进入 job 负载
                }
            }
        }

        // 容器内网 IP（data plane 目标；JudgeServer 在该网络上直连）。
        let target_ip = if instance.runtime_state == "running" {
            match runtime.inspect_container(&instance.container_name).await {
                Ok(state) => state.ip_address,
                Err(e) => {
                    tracing::warn!(
                        container = %instance.container_name,
                        error = %e,
                        "claim: inspect container failed"
                    );
                    None
                }
            }
        } else {
            None
        };

        let healthchecks = gamebox
            .healthchecks_json
            .clone()
            .unwrap_or_else(|| serde_json::json!([]))
            .as_array()
            .cloned()
            .unwrap_or_default();

        let kind = ev.kind.clone();
        // official：生成一次性 target-bound proof（token 只出现在 job payload）。
        let proof_url = match &kind {
            AwdpEvaluationKind::Official => {
                let token = create_proof_for_job(
                    db,
                    ev.id,
                    ev.instance_id,
                    instance.runtime_generation,
                    lease_seconds,
                )
                .await?;
                Some(format!(
                    "http://{}/proof/{token}",
                    crate::modules::event::awdp::domain::judge::JUDGE_DATA_ALIAS
                ))
            }
            // worker 只 claim official；manual 由 Test Check 同步流程独占（含 exploit 诊断）。
            AwdpEvaluationKind::Manual => None,
        };
        jobs.push(ClaimJobDto {
            evaluation_id: ev.id,
            attempt: job.attempt,
            lease_token: job.lease_token,
            kind: kind_string(&kind).to_string(),
            run_id: run.id,
            round_id: ev.fix_round_id,
            instance_id: ev.instance_id,
            runtime_generation: instance.runtime_generation,
            target_ip,
            healthchecks,
            judge_script: gamebox.judge_script_content.clone(),
            // 仅 official 下发给 worker（manual 的 exploit 由同步 Test Check 路径执行）。
            exploit_script: gamebox.awdp_exploit_script_content.clone(),
            proof_url,
        });
    }

    Ok(ClaimResponse {
        lease_seconds,
        jobs,
    })
}

fn kind_string(kind: &AwdpEvaluationKind) -> &'static str {
    match kind {
        AwdpEvaluationKind::Manual => "manual",
        AwdpEvaluationKind::Official => "official",
    }
}

/// heartbeat：延长 lease（无效 lease 返回 NoLease）。
pub async fn heartbeat_job(
    db: &DatabaseConnection,
    evaluation_id: Uuid,
    req: &HeartbeatRequest,
    lease_seconds: i64,
) -> AwdpResult<evaluation_repo::HeartbeatOutcome> {
    evaluation_repo::heartbeat(
        db,
        evaluation_id,
        &req.worker_id,
        &req.lease_token,
        lease_seconds,
    )
    .await
}

/// result：验证 lease + attempt + runtime_generation 后写终态；official+patched 平台侧幂等计分。
/// `max_attempts`：platform_error 基础设施失败的可重试上限。
pub async fn record_result(
    db: &DatabaseConnection,
    req: &ResultRequest,
    max_attempts: i32,
) -> AwdpResult<()> {
    let ev = evaluation_repo::find_by_id(db, req.evaluation_id).await?;

    // runtime_generation 必须匹配当前 Instance（容器 reset 后旧 worker 结果作废）。
    let (instance, _ext) = instance_repo::find_by_instance_id(db, ev.instance_id).await?;
    if instance.runtime_generation != req.runtime_generation {
        return Err(AwdpError::Conflict(format!(
            "stale runtime_generation: job={} current={}",
            req.runtime_generation, instance.runtime_generation
        )));
    }

    let status = parse_terminal_status(&req.status)?;

    // platform_error = 基础设施失败（spawn/超时/畸形输出/协议违规）：不能判玩家失败，
    // 未达 max_attempts 释放回 pending 重试，达到则终态 PLATFORM_ERROR（plan §18/§25/§26）。
    if status == AwdpEvaluationStatus::PlatformError {
        let outcome = evaluation_repo::release_or_fail(
            db,
            req.evaluation_id,
            &req.worker_id,
            &req.lease_token,
            req.attempt,
            max_attempts,
            req.stderr_limited
                .as_deref()
                .or(req.judge_result.as_deref())
                .unwrap_or("platform error"),
        )
        .await?;
        return match outcome {
            evaluation_repo::FinishOutcome::Ok => Ok(()),
            evaluation_repo::FinishOutcome::StaleRejected => Err(AwdpError::Conflict(
                "stale worker result rejected (lease/token/attempt mismatch)".into(),
            )),
        };
    }

    let status_clone = status.clone();
    let outcome = evaluation_repo::finish_with_lease(
        db,
        req.evaluation_id,
        &req.worker_id,
        &req.lease_token,
        req.attempt,
        status_clone,
        req.healthcheck_result.as_deref(),
        req.judge_result.as_deref(),
        req.exploit_result.as_deref(),
        req.stdout_limited.as_deref(),
        req.stderr_limited.as_deref(),
    )
    .await?;
    if outcome == evaluation_repo::FinishOutcome::StaleRejected {
        return Err(AwdpError::Conflict(
            "stale worker result rejected (lease/token/attempt mismatch)".into(),
        ));
    }

    // official + PATCHED → 平台侧幂等计分（idempotency_key 全局唯一兜底）。
    if status == AwdpEvaluationStatus::Patched && ev.kind == AwdpEvaluationKind::Official {
        let round_id = ev.fix_round_id.ok_or_else(|| {
            AwdpError::Internal("official evaluation missing fix_round_id".into())
        })?;
        let run = run_repo::require_by_id(db, ev.run_id).await?;
        let (_, ext) = instance_repo::find_by_instance_id(db, ev.instance_id).await?;
        let key = fix_idempotency_key(run.id, round_id, ev.instance_id);
        score_repo::create_score_event(
            db,
            run.id,
            ext.owner_user_id,
            ext.owner_team_id,
            ext.gamebox_id,
            "fix",
            Some(round_id),
            run.fix_round_score,
            &key,
        )
        .await?;
    }
    Ok(())
}

/// 解析终态字符串；非终态（pending/running）或未知 → Validation 错误。
fn parse_terminal_status(s: &str) -> AwdpResult<AwdpEvaluationStatus> {
    match s {
        "no_patch" => Ok(AwdpEvaluationStatus::NoPatch),
        "service_down" => Ok(AwdpEvaluationStatus::ServiceDown),
        "functional_broken" => Ok(AwdpEvaluationStatus::FunctionalBroken),
        "vulnerable" => Ok(AwdpEvaluationStatus::Vulnerable),
        "patched" => Ok(AwdpEvaluationStatus::Patched),
        "platform_error" => Ok(AwdpEvaluationStatus::PlatformError),
        other => Err(AwdpError::Validation(format!(
            "invalid terminal evaluation status: {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_status_parsing() {
        assert_eq!(
            parse_terminal_status("patched").unwrap(),
            AwdpEvaluationStatus::Patched
        );
        assert_eq!(
            parse_terminal_status("no_patch").unwrap(),
            AwdpEvaluationStatus::NoPatch
        );
        assert!(parse_terminal_status("pending").is_err());
        assert!(parse_terminal_status("running").is_err());
        assert!(parse_terminal_status("bogus").is_err());
    }

    #[test]
    fn manual_kind_never_carries_exploit_script() {
        // worker 只 claim official；manual 由 Test Check 同步流程独占（含 exploit 诊断）。
        // 这里直接验证 kind_string 稳定。
        assert_eq!(kind_string(&AwdpEvaluationKind::Manual), "manual");
        assert_eq!(kind_string(&AwdpEvaluationKind::Official), "official");
    }
}
