//! AWDP 手动 Test Check（plan §24）：Healthcheck + Judge（练习模式追加 Exploit），绝不计分。

use std::time::Duration;

use bollard::Docker;
use fcmc::{ContainerRuntime, DockerContainerRuntime};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::infrastructure::script_runner::{parse_batch_results, run_script};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::fix_idempotency_key,
    repo::{
        evaluation_repo, event_gamebox_repo, instance_repo, patch_repo, round_repo, run_repo,
        score_repo,
    },
    service::{lock::InstanceAdvisoryLock, runtime::Subject},
};

/// 手动检查结果（无分数）。
#[derive(Debug, Clone)]
pub struct ManualCheckResult {
    pub healthcheck_ok: bool,
    pub healthcheck_detail: Vec<String>,
    pub judge_ok: bool,
    pub judge_detail: String,
    /// 练习模式才执行（exploit 诊断展示；不计分）。
    pub exploit_ok: Option<bool>,
    pub exploit_detail: Option<String>,
}

/// 手动 Test Check 入队（plan §20）：创建 manual 评估（pending），由 worker
/// （JudgeServer / 进程内）异步执行 healthcheck + judge。返回评估 id + 初始状态；
/// 前端轮询 evaluations 直到终态。不再让 HTTP 请求阻塞等待执行。
pub async fn manual_check_enqueue(
    db: &DatabaseConnection,
    run_id: Uuid,
    instance_id: Uuid,
    subject: Subject,
) -> AwdpResult<crate::entity::awdp_evaluations::Model> {
    let run = run_repo::require_by_id(db, run_id).await?;
    if run.phase != crate::entity::sea_orm_active_enums::AwdpPhase::Fix {
        return Err(AwdpError::InvalidState(
            "Test Check 仅在 Fix 阶段可用".into(),
        ));
    }
    let (instance, _ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
    {
        let owned = match subject {
            Subject {
                user_id: Some(u),
                team_id: None,
            } => instance.owner_user_id == Some(u),
            Subject {
                user_id: None,
                team_id: Some(t),
            } => instance.owner_team_id == Some(t),
            _ => false,
        };
        if !owned {
            return Err(AwdpError::Forbidden(
                "instance does not belong to you".into(),
            ));
        }
    }
    let evaluation = evaluation_repo::create_manual(db, run_id, instance_id).await?;
    Ok(evaluation)
}

/// 同步执行 manual Test Check（不排队）：HTTP 请求内直接执行 healthcheck + judge，
/// 写终态后返回结果。与 worker 经 instance advisory lock 串行；无 lease（同步独占）。
pub async fn manual_check_run_now(
    db: &DatabaseConnection,
    docker: &Docker,
    evaluation: &crate::entity::awdp_evaluations::Model,
) -> AwdpResult<ManualCheckResult> {
    let lock = InstanceAdvisoryLock::acquire(db, evaluation.instance_id).await?;
    let result = manual_leased_locked(db, docker, evaluation).await;
    lock.release().await;

    let (status, health_detail, judge_detail, _exploit_detail) = result?;
    evaluation_repo::finish(
        db,
        evaluation.id,
        status.clone(),
        Some(&health_detail),
        Some(&judge_detail),
        None,
        None,
        None,
    )
    .await?;

    use crate::entity::sea_orm_active_enums::AwdpEvaluationStatus as S;
    Ok(ManualCheckResult {
        healthcheck_ok: status != S::ServiceDown,
        healthcheck_detail: if health_detail.is_empty() {
            Vec::new()
        } else {
            vec![health_detail]
        },
        judge_ok: status != S::ServiceDown && status != S::FunctionalBroken,
        judge_detail,
        // 同步路径仅比赛使用（exploit 恒不执行）。
        exploit_ok: None,
        exploit_detail: None,
    })
}

/// 进程内 worker 执行 manual 评估（lease 版）：internal healthcheck + judge，绝不 exploit/计分。
async fn process_manual_leased(
    db: &DatabaseConnection,
    docker: &Docker,
    worker_id: &str,
    ev: &crate::entity::awdp_evaluations::Model,
    lease_token: &str,
    attempt: i32,
) -> AwdpResult<()> {
    let lock = InstanceAdvisoryLock::acquire(db, ev.instance_id).await?;
    let result = manual_leased_locked(db, docker, ev).await;
    lock.release().await;

    match result {
        Ok((status, health_detail, judge_detail, exploit_detail)) => {
            let exploit = if exploit_detail.is_empty() {
                None
            } else {
                Some(exploit_detail.as_str())
            };
            let _ = evaluation_repo::finish_with_lease(
                db,
                ev.id,
                worker_id,
                lease_token,
                attempt,
                status,
                Some(&health_detail),
                Some(&judge_detail),
                exploit,
                None,
                None,
            )
            .await;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

async fn manual_leased_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    ev: &crate::entity::awdp_evaluations::Model,
) -> AwdpResult<(
    crate::entity::sea_orm_active_enums::AwdpEvaluationStatus,
    String,
    String,
    String,
)> {
    use crate::entity::sea_orm_active_enums::AwdpEvaluationStatus as S;
    let (instance, ext) = instance_repo::find_by_instance_id(db, ev.instance_id).await?;
    let gamebox = event_gamebox_repo::find_gamebox_identity(db, ext.gamebox_id).await?;
    let run = run_repo::require_by_id(db, ext.run_id).await?;
    // 练习模式（run.gamebox_id 非空）才执行 exploit 诊断；竞赛保持 healthcheck + judge。
    let is_practice = run.gamebox_id.is_some();

    // 容器内网 IP（internal healthcheck 目标；public NAT 失败不算 SERVICE_DOWN）。
    let container_ip = if instance.runtime_state == "running" {
        let runtime = DockerContainerRuntime::new(docker.clone());
        match runtime.inspect_container(&instance.container_name).await {
            Ok(state) => state.ip_address,
            Err(e) => {
                return Err(AwdpError::Docker(format!("inspect for manual check: {e}")));
            }
        }
    } else {
        None
    };
    let Some(container_ip) = container_ip else {
        return Ok((
            S::ServiceDown,
            "instance not running".into(),
            "manual check skipped".into(),
            String::new(),
        ));
    };

    // 1. internal healthcheck（target_ip:container_port，带重试；plan §23/§24）。
    let healthchecks = crate::modules::gamebox::healthcheck::parse_healthchecks(
        &gamebox
            .healthchecks_json
            .clone()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    let mut health_detail = Vec::new();
    let mut health_ok = true;
    for hc in &healthchecks {
        // 打容器内网 IP + container_port（不依赖 public NAT）。
        let internal = match hc {
            crate::modules::gamebox::healthcheck::AppHealthcheck::Http {
                port,
                path,
                expected_status,
                ..
            } => crate::modules::gamebox::healthcheck::AppHealthcheck::Http {
                port: *port,
                path: path.clone(),
                expected_status: *expected_status,
            },
            crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { port } => {
                crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { port: *port }
            }
        };
        let result = crate::modules::gamebox::healthcheck::probe_one_with_retries(
            &container_ip,
            &internal,
            Duration::from_secs(5),
            3,
            Duration::from_secs(2),
        )
        .await;
        health_detail.push(result.detail.clone());
        if !result.ok {
            health_ok = false;
        }
    }

    // 2. Judge（check.py，批量契约单目标）。
    let (judge_ok, judge_detail) = match gamebox.judge_script_content.clone() {
        Some(script) => {
            let outcome = run_script(
                &script,
                "python3",
                &[container_ip.clone()],
                &[],
                Duration::from_secs(30),
                16 * 1024,
                16 * 1024,
            )
            .await
            .map_err(|e| AwdpError::Internal(format!("judge runner: {e}")))?;
            match parse_batch_results(&outcome.stdout) {
                Ok(rows) => {
                    let hit = rows
                        .iter()
                        .find(|r| r.ip == container_ip || r.ip.is_empty())
                        .or_else(|| rows.first());
                    match hit {
                        Some(r) => (
                            r.success,
                            r.error.clone().unwrap_or_else(|| {
                                format!("{}: {}", r.ip, if r.success { "PASS" } else { "FAIL" })
                            }),
                        ),
                        None => (false, "judge 脚本未返回该目标结果".into()),
                    }
                }
                Err(e) => (
                    false,
                    format!("judge 输出解析失败: {e}; exit={:?}", outcome.exit_code),
                ),
            }
        }
        None => (false, "GameBox 未配置 judge 脚本".into()),
    };

    // 3. Exploit（仅练习模式；诊断展示，不计分）：与 official 同款批量契约单目标。
    let (exploit_ok, exploit_detail) = if is_practice {
        match gamebox.awdp_exploit_script_content.clone() {
            Some(script) => {
                let outcome = run_script(
                    &script,
                    "python3",
                    &[container_ip.clone()],
                    &[],
                    Duration::from_secs(60),
                    32 * 1024,
                    32 * 1024,
                )
                .await
                .map_err(|e| AwdpError::Internal(format!("exploit runner: {e}")))?;
                match parse_batch_results(&outcome.stdout) {
                    Ok(rows) => {
                        let hit = rows
                            .iter()
                            .find(|r| r.ip == container_ip || r.ip.is_empty())
                            .or_else(|| rows.first());
                        match hit {
                            Some(r) => (
                                Some(r.success),
                                r.error.clone().unwrap_or_else(|| {
                                    format!(
                                        "{}: {}",
                                        r.ip,
                                        if r.success { "SUCCESS" } else { "FAIL" }
                                    )
                                }),
                            ),
                            None => (Some(false), "exploit 无该目标结果".into()),
                        }
                    }
                    Err(e) => (
                        Some(false),
                        format!("exploit 输出解析失败: {e}; exit={:?}", outcome.exit_code),
                    ),
                }
            }
            None => (Some(false), "GameBox 未配置 exploit 脚本".into()),
        }
    } else {
        (None, String::new())
    };

    // 4. 终态（manual 不计分；练习模式 exploit 成功 → Vulnerable）。
    let status = if !health_ok {
        S::ServiceDown
    } else if !judge_ok {
        S::FunctionalBroken
    } else if exploit_ok == Some(true) {
        S::Vulnerable
    } else {
        S::Patched
    };
    Ok((
        status,
        health_detail.join("; "),
        judge_detail,
        exploit_detail,
    ))
}

// ────────────────────────────────────────────────────────────────────────────
// Official evaluation（plan §25/§32/§33/§36）
// ────────────────────────────────────────────────────────────────────────────

/// 物化一条 official 评估（round × instance 唯一；幂等）。
pub async fn materialize_official_evaluations(
    db: &DatabaseConnection,
    run_id: Uuid,
    instance_id: Uuid,
    fix_round_id: Uuid,
) -> AwdpResult<bool> {
    let _ = evaluation_repo::create_official(db, run_id, instance_id, fix_round_id).await?;
    Ok(true)
}

/// worker 单轮：领取 pending 评估（Pull + Lease，SKIP LOCKED）并逐一执行。
///
/// `worker_id`：进程内 worker 固定标识；`lease_duration_secs` / `max_attempts` 来自
/// config（awdp.eval_lease_duration_secs / awdp.eval_max_attempts）。
/// 仅领取 official（manual 由 Test Check 同步流程独占）。
#[allow(clippy::too_many_arguments)]
pub async fn worker_round(
    db: &DatabaseConnection,
    docker: &Docker,
    worker_id: &str,
    concurrency: u64,
    lease_duration_secs: i64,
    max_attempts: i32,
) -> AwdpResult<usize> {
    use crate::entity::sea_orm_active_enums::AwdpEvaluationKind;
    // 进程内 worker 仅消费 official：manual = Test Check 同步流程独占（HTTP 请求内
    // 直接执行），worker 不得领取 manual——否则与同步路径双写同一行会触发
    // awdp_evaluations_lease_consistency_check 违例（终态 + lease_token_hash 并存）。
    // JudgeServer 与进程内 worker 经 lease 互斥竞争。
    // 进程内 worker 在宿主网络，可达所有赛事网络 → 不按 event 过滤（None）。
    let claimed = evaluation_repo::claim_jobs(
        db,
        worker_id,
        concurrency,
        lease_duration_secs,
        max_attempts,
        &[AwdpEvaluationKind::Official],
        None,
    )
    .await?;
    let mut n = 0usize;
    for job in claimed {
        let ev = job.evaluation;
        let kind = ev.kind.clone();
        let processed = match kind {
            AwdpEvaluationKind::Manual => {
                process_manual_leased(db, docker, worker_id, &ev, &job.lease_token, job.attempt)
                    .await
            }
            AwdpEvaluationKind::Official => {
                process_official_leased(db, docker, worker_id, &ev, &job.lease_token, job.attempt)
                    .await
            }
        };
        match processed {
            Ok(()) => n += 1,
            Err(e) => {
                // 平台/基础设施失败：不能判玩家失败；未达 max_attempts 释放回 pending 重试，
                // 达到则终态 PLATFORM_ERROR（允许区分 VULNERABLE/BROKEN 等业务终态）。
                let _ = evaluation_repo::release_or_fail(
                    db,
                    ev.id,
                    worker_id,
                    &job.lease_token,
                    job.attempt,
                    max_attempts,
                    &format!("{e}"),
                )
                .await;
                tracing::warn!(evaluation_id = %ev.id, attempt = job.attempt, error = %e, "AWDP evaluation failed (release or fail)");
            }
        }
    }
    Ok(n)
}

/// 执行单条 official 评估（lease 版）：NO_PATCH → Healthcheck → Judge → Exploit → Score。
async fn process_official_leased(
    db: &DatabaseConnection,
    docker: &Docker,
    worker_id: &str,
    ev: &crate::entity::awdp_evaluations::Model,
    lease_token: &str,
    attempt: i32,
) -> AwdpResult<()> {
    let lock = InstanceAdvisoryLock::acquire(db, ev.instance_id).await?;
    let result = process_official_locked(db, docker, worker_id, ev, lease_token, attempt).await;
    lock.release().await;
    result
}

/// 执行单条 official 评估（持有 instance lock）：NO_PATCH → Healthcheck → Judge → Exploit → Score。
async fn process_official_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    worker_id: &str,
    ev: &crate::entity::awdp_evaluations::Model,
    lease_token: &str,
    attempt: i32,
) -> AwdpResult<()> {
    let (_instance, ext) = instance_repo::find_by_instance_id(db, ev.instance_id).await?;
    let run = run_repo::require_by_id(db, ext.run_id).await?;
    let gamebox = event_gamebox_repo::find_gamebox_identity(db, ext.gamebox_id).await?;
    let round_id = ev
        .fix_round_id
        .ok_or_else(|| AwdpError::Internal("official evaluation missing fix_round_id".into()))?;
    let round = round_repo::find_by_id(db, round_id).await?;
    run_official_pipeline(
        db,
        docker,
        &run,
        &gamebox,
        ev,
        &round,
        worker_id,
        lease_token,
        attempt,
    )
    .await?;
    Ok(())
}

/// 共享检查执行（NO_PATCH → Healthcheck → Judge → Exploit），只判状态不落账。
/// worker 的 official 管线与练习 ALL Check 共用，保证判定语义一致。
#[allow(clippy::too_many_arguments)]
async fn run_checks(
    db: &DatabaseConnection,
    docker: &Docker,
    gamebox: &crate::entity::gameboxes::Model,
    instance_id: Uuid,
    round: &crate::entity::awdp_fix_rounds::Model,
) -> AwdpResult<CheckOutcome> {
    use crate::entity::sea_orm_active_enums::AwdpEvaluationStatus as S;

    // 1. NO_PATCH 短路（plan §21/§33：本轮无 APPLIED patch → 不浪费资源）。
    if !patch_repo::has_applied_patch(db, instance_id, round.id).await? {
        return Ok(CheckOutcome {
            status: S::NoPatch,
            healthcheck_detail: "no applied patch this round".into(),
            judge_detail: String::new(),
            exploit_detail: String::new(),
        });
    }

    // 容器可能已停 → 评估前 inspect（评估官方语义：对当前 instance 状态判）。
    let (instance, _ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
    let runtime = DockerContainerRuntime::new(docker.clone());
    let container_ip = if instance.runtime_state == "running" {
        let state = runtime
            .inspect_container(&instance.container_name)
            .await
            .map_err(|e| AwdpError::Docker(format!("inspect for eval: {e}")))?;
        state.ip_address
    } else {
        None
    };

    // 2. Healthcheck（internal network：容器内网 IP + container_port；plan §23/§24）。
    //    public NAT/host 端口出问题不误判 SERVICE_DOWN（只服务玩家）。
    let healthchecks = crate::modules::gamebox::healthcheck::parse_healthchecks(
        &gamebox
            .healthchecks_json
            .clone()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    let mut health_ok = true;
    let mut health_detail = Vec::new();
    for hc in &healthchecks {
        let Some(ip) = &container_ip else {
            health_ok = false;
            health_detail.push("容器未运行，无法 healthcheck".into());
            continue;
        };
        let internal_check = match hc {
            crate::modules::gamebox::healthcheck::AppHealthcheck::Http {
                port,
                path,
                expected_status,
                ..
            } => crate::modules::gamebox::healthcheck::AppHealthcheck::Http {
                port: *port,
                path: path.clone(),
                expected_status: *expected_status,
            },
            crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { port } => {
                crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { port: *port }
            }
        };
        let r = crate::modules::gamebox::healthcheck::probe_one_with_retries(
            ip,
            &internal_check,
            Duration::from_secs(5),
            3,
            Duration::from_secs(2),
        )
        .await;
        health_detail.push(r.detail.clone());
        if !r.ok {
            health_ok = false;
        }
    }
    if !health_ok {
        return Ok(CheckOutcome {
            status: S::ServiceDown,
            healthcheck_detail: health_detail.join("; "),
            judge_detail: String::new(),
            exploit_detail: String::new(),
        });
    }

    // 3. Judge（check.py）。
    let judge_content = gamebox.judge_script_content.clone();
    let (judge_ok, judge_detail) = match (judge_content, &container_ip) {
        (Some(script), Some(ip)) => {
            let outcome = run_script(
                &script,
                "python3",
                &[ip.clone()],
                &[],
                Duration::from_secs(30),
                16 * 1024,
                16 * 1024,
            )
            .await
            .map_err(|e| AwdpError::Internal(format!("judge runner: {e}")))?;
            match parse_batch_results(&outcome.stdout) {
                Ok(rows) => {
                    let hit = rows
                        .iter()
                        .find(|r| r.ip == *ip || r.ip.is_empty())
                        .or_else(|| rows.first());
                    match hit {
                        Some(r) => (
                            r.success,
                            r.error.clone().unwrap_or_else(|| {
                                format!("judge: {}", if r.success { "PASS" } else { "FAIL" })
                            }),
                        ),
                        None => (false, "judge 无该目标结果".into()),
                    }
                }
                Err(e) => (false, format!("judge 输出解析失败: {e}")),
            }
        }
        (None, _) => (false, "GameBox 无 judge 脚本".into()),
        (_, None) => (false, "容器未运行，无法 judge".into()),
    };
    if !judge_ok {
        return Ok(CheckOutcome {
            status: S::FunctionalBroken,
            healthcheck_detail: health_detail.join("; "),
            judge_detail,
            exploit_detail: String::new(),
        });
    }

    // 4. Exploit（平台侧执行；成功 = VULNERABLE，失败 = PATCHED）。
    let exploit_content = gamebox.awdp_exploit_script_content.clone();
    let (exploit_ok, exploit_detail) = match (exploit_content, &container_ip) {
        (Some(script), Some(ip)) => {
            let outcome = run_script(
                &script,
                "python3",
                &[ip.clone()],
                &[],
                Duration::from_secs(60),
                32 * 1024,
                32 * 1024,
            )
            .await
            .map_err(|e| AwdpError::Internal(format!("exploit runner: {e}")))?;
            match parse_batch_results(&outcome.stdout) {
                Ok(rows) => {
                    let hit = rows
                        .iter()
                        .find(|r| r.ip == *ip || r.ip.is_empty())
                        .or_else(|| rows.first());
                    match hit {
                        Some(r) => (
                            r.success,
                            r.error.clone().unwrap_or_else(|| "exploit executed".into()),
                        ),
                        None => (false, "exploit 无该目标结果".into()),
                    }
                }
                Err(e) => (false, format!("exploit 输出解析失败: {e}")),
            }
        }
        (None, _) => (false, "GameBox 无 [awdp] exploit 脚本".into()),
        (_, None) => (false, "容器未运行，无法执行 exploit".into()),
    };

    let status = if exploit_ok {
        S::Vulnerable
    } else {
        S::Patched
    };
    Ok(CheckOutcome {
        status,
        healthcheck_detail: health_detail.join("; "),
        judge_detail,
        exploit_detail,
    })
}

/// 检查执行结果（仅判定，未落账）。
#[derive(Debug, Clone)]
struct CheckOutcome {
    status: crate::entity::sea_orm_active_enums::AwdpEvaluationStatus,
    healthcheck_detail: String,
    judge_detail: String,
    exploit_detail: String,
}

/// 练习模式 check 失败扣分：NO_PATCH / SERVICE_DOWN / FUNCTIONAL_BROKEN / VULNERABLE
/// 每轮每实例以同一幂等键 `awdp:fix:{run}:{round}:{instance}` 写一条负 delta 账本
/// （与 PATCHED +fix_round_score 互斥；重试不会重复扣）。
async fn score_fix_penalty(
    db: &DatabaseConnection,
    run: &crate::entity::awdp_runs::Model,
    round: &crate::entity::awdp_fix_rounds::Model,
    ext: &crate::entity::awdp_instances::Model,
) -> AwdpResult<()> {
    let key = fix_idempotency_key(run.id, round.id, ext.instance_id);
    let _ = score_repo::create_score_event(
        db,
        run.id,
        ext.owner_user_id,
        ext.owner_team_id,
        ext.gamebox_id,
        "fix",
        Some(round.id),
        -crate::modules::event::awdp::domain::config::DEFAULT_FIX_ROUND_PENALTY,
        &key,
    )
    .await?;
    Ok(())
}

/// 共享官方评估管线：NO_PATCH 短路 → Healthcheck → Judge → Exploit → 计分。
/// 返回终态（worker 与练习 ALL Check 共用，保证语义一致）。
///
/// 有 lease 时（lease_token 非空）用 `finish_with_lease` 写终态（stale 拒绝）；
/// 无 lease（练习 ALL Check 进程内同步路径）用 `finish`。
///
/// 计分：练习模式（run.gamebox_id 非空）PATCHED → +fix_round_score，其余终态 → -penalty；
/// 竞赛模式维持历史语义：仅 PATCHED +fix_round_score，其余 +0。
#[allow(clippy::too_many_arguments)]
async fn run_official_pipeline(
    db: &DatabaseConnection,
    docker: &Docker,
    run: &crate::entity::awdp_runs::Model,
    gamebox: &crate::entity::gameboxes::Model,
    ev: &crate::entity::awdp_evaluations::Model,
    round: &crate::entity::awdp_fix_rounds::Model,
    worker_id: &str,
    lease_token: &str,
    attempt: i32,
) -> AwdpResult<crate::entity::sea_orm_active_enums::AwdpEvaluationStatus> {
    use crate::entity::sea_orm_active_enums::AwdpEvaluationStatus as S;

    // 终态写入助手：有 lease → lease 校验版；否则直接写。
    async fn finish_eval(
        db: &DatabaseConnection,
        ev: &crate::entity::awdp_evaluations::Model,
        worker_id: &str,
        lease_token: &str,
        attempt: i32,
        status: S,
        healthcheck_result: Option<&str>,
        judge_result: Option<&str>,
        exploit_result: Option<&str>,
        stdout_limited: Option<&str>,
        stderr_limited: Option<&str>,
    ) -> AwdpResult<()> {
        if lease_token.is_empty() {
            evaluation_repo::finish(
                db,
                ev.id,
                status,
                healthcheck_result,
                judge_result,
                exploit_result,
                stdout_limited,
                stderr_limited,
            )
            .await?;
        } else {
            let outcome = evaluation_repo::finish_with_lease(
                db,
                ev.id,
                worker_id,
                lease_token,
                attempt,
                status,
                healthcheck_result,
                judge_result,
                exploit_result,
                stdout_limited,
                stderr_limited,
            )
            .await?;
            if outcome
                == crate::modules::event::awdp::repo::evaluation_repo::FinishOutcome::StaleRejected
            {
                tracing::warn!(evaluation_id = %ev.id, attempt, "AWDP stale worker result rejected");
            }
        }
        Ok(())
    }

    // 计分主体取实例归属（competition 的 run 无主体；practice 与 run 一致）。
    let ext = {
        let (_inst, ext) = instance_repo::find_by_instance_id(db, ev.instance_id).await?;
        ext
    };
    let is_practice = run.gamebox_id.is_some();

    let outcome = run_checks(db, docker, gamebox, ev.instance_id, round).await?;
    match outcome.status {
        S::NoPatch => {
            if is_practice {
                score_fix_penalty(db, run, round, &ext).await?;
            }
            finish_eval(
                db,
                ev,
                worker_id,
                lease_token,
                attempt,
                S::NoPatch,
                Some(&outcome.healthcheck_detail),
                None,
                None,
                None,
                None,
            )
            .await?;
            Ok(S::NoPatch)
        }
        S::ServiceDown => {
            if is_practice {
                score_fix_penalty(db, run, round, &ext).await?;
            }
            finish_eval(
                db,
                ev,
                worker_id,
                lease_token,
                attempt,
                S::ServiceDown,
                Some(&outcome.healthcheck_detail),
                None,
                None,
                None,
                None,
            )
            .await?;
            Ok(S::ServiceDown)
        }
        S::FunctionalBroken => {
            if is_practice {
                score_fix_penalty(db, run, round, &ext).await?;
            }
            finish_eval(
                db,
                ev,
                worker_id,
                lease_token,
                attempt,
                S::FunctionalBroken,
                Some(&outcome.healthcheck_detail),
                Some(&outcome.judge_detail),
                None,
                None,
                None,
            )
            .await?;
            Ok(S::FunctionalBroken)
        }
        S::Vulnerable => {
            if is_practice {
                score_fix_penalty(db, run, round, &ext).await?;
            }
            finish_eval(
                db,
                ev,
                worker_id,
                lease_token,
                attempt,
                S::Vulnerable,
                Some(&outcome.healthcheck_detail),
                Some(&outcome.judge_detail),
                Some(&outcome.exploit_detail),
                None,
                None,
            )
            .await?;
            Ok(S::Vulnerable)
        }
        S::Patched => {
            // PATCHED → +fix_round_score（幂等键 awdp:fix:{run}:{round}:{instance}）。
            let key = fix_idempotency_key(run.id, round.id, ev.instance_id);
            let _scored = score_repo::create_score_event(
                db,
                run.id,
                ext.owner_user_id,
                ext.owner_team_id,
                ext.gamebox_id,
                "fix",
                Some(round.id),
                run.fix_round_score,
                &key,
            )
            .await?;
            finish_eval(
                db,
                ev,
                worker_id,
                lease_token,
                attempt,
                S::Patched,
                Some(&outcome.healthcheck_detail),
                Some(&outcome.judge_detail),
                Some(&outcome.exploit_detail),
                None,
                None,
            )
            .await?;
            Ok(S::Patched)
        }
        other => Err(AwdpError::Internal(format!(
            "unexpected check outcome status {other:?}"
        ))),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Practice ALL Check（练习模式）：一键检查成功 → 剩余回合全部计分 + 直接结束
// ────────────────────────────────────────────────────────────────────────────

/// ALL Check 结果（练习模式；status=Patched 时触发自动计分 + 结束 run）。
#[derive(Debug, Clone)]
pub struct AllCheckResult {
    pub status: crate::entity::sea_orm_active_enums::AwdpEvaluationStatus,
    /// status == Patched（剩余回合全部计分 + run 已结束）。
    pub swept: bool,
    /// 自动计分的回合数（当前轮起含：total_rounds - target_round + 1）。
    pub swept_rounds: i32,
    pub target_round: i32,
    pub healthcheck_detail: String,
    pub judge_detail: String,
    pub exploit_detail: String,
}

/// 练习 ALL Check：对当前回合立即执行官方判定管线（healthcheck→judge→exploit）。
///
/// - PATCHED（修复成功）→ 从当前轮起（含）全部剩余回合 +fix_round_score（幂等账本 +
///   幂等评估结算），停止实例，run 直接 Ended（练习模式无需等后续回合）。
/// - 其余终态（NO_PATCH / SERVICE_DOWN / FUNCTIONAL_BROKEN / VULNERABLE）→ **不落账、
///   不扣分、不写评估**，等本轮 cutoff 的官方 check 照常判定。
///
/// 仅练习 run（gamebox_id 非空）且 Fix 阶段可用；exploit 全程平台侧执行，不下发给玩家。
pub async fn all_check(
    db: &DatabaseConnection,
    docker: &Docker,
    run_id: Uuid,
    instance_id: Uuid,
    subject: Subject,
) -> AwdpResult<AllCheckResult> {
    use crate::entity::sea_orm_active_enums::AwdpPhase;
    let run = run_repo::require_by_id(db, run_id).await?;
    if run.gamebox_id.is_none() {
        return Err(AwdpError::InvalidState("ALL Check 仅练习模式可用".into()));
    }
    if run.phase != AwdpPhase::Fix {
        return Err(AwdpError::InvalidState(
            "ALL Check 仅在 Fix 阶段可用".into(),
        ));
    }
    let (instance, _ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
    {
        let owned = match subject {
            Subject {
                user_id: Some(u),
                team_id: None,
            } => instance.owner_user_id == Some(u),
            Subject {
                user_id: None,
                team_id: Some(t),
            } => instance.owner_team_id == Some(t),
            _ => false,
        };
        if !owned {
            return Err(AwdpError::Forbidden(
                "instance does not belong to you".into(),
            ));
        }
    }

    let lock = InstanceAdvisoryLock::acquire(db, instance_id).await?;
    let result = all_check_locked(db, docker, &run, instance_id, subject).await;
    lock.release().await;
    result
}

async fn all_check_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    run: &crate::entity::awdp_runs::Model,
    instance_id: Uuid,
    subject: Subject,
) -> AwdpResult<AllCheckResult> {
    use crate::entity::sea_orm_active_enums::AwdpEvaluationStatus as S;

    // 目标回合：下一个未完成回合（pending 或 evaluating）——即「当前 turn」。
    let round = round_repo::next_due_round(db, run.id)
        .await?
        .ok_or_else(|| AwdpError::InvalidState("所有回合均已评估/结束".into()))?;
    let gamebox = {
        let (_inst, ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
        event_gamebox_repo::find_gamebox_identity(db, ext.gamebox_id).await?
    };

    // 执行判定（不创建评估行、不落账）。失败 → 原样返回，等本轮官方 check。
    let outcome = run_checks(db, docker, &gamebox, instance_id, &round).await?;

    let mut result = AllCheckResult {
        status: outcome.status.clone(),
        swept: false,
        swept_rounds: 0,
        target_round: round.sequence,
        healthcheck_detail: outcome.healthcheck_detail.clone(),
        judge_detail: outcome.judge_detail.clone(),
        exploit_detail: outcome.exploit_detail.clone(),
    };

    if outcome.status != S::Patched {
        return Ok(result);
    }

    // 成功：从当前轮起（含）全部剩余回合自动计分（幂等账本 + 幂等评估结算）。
    let ext = {
        let (_inst, ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
        ext
    };
    let mut swept = 0i32;
    for seq in round.sequence..=run.total_rounds {
        let Some(r) = round_repo::find_by_sequence(db, run.id, seq).await? else {
            continue;
        };
        let eval = evaluation_repo::create_official(db, run.id, instance_id, r.id).await?;
        let (hc, jd, xd) = if seq == round.sequence {
            (
                Some(outcome.healthcheck_detail.as_str()),
                Some(outcome.judge_detail.as_str()),
                Some(outcome.exploit_detail.as_str()),
            )
        } else {
            (Some("ALL check: 修复已确认，自动计分"), None, None)
        };
        evaluation_repo::finish(db, eval.id, S::Patched, hc, jd, xd, None, None).await?;
        let key = fix_idempotency_key(run.id, r.id, instance_id);
        let _scored = score_repo::create_score_event(
            db,
            run.id,
            ext.owner_user_id,
            ext.owner_team_id,
            ext.gamebox_id,
            "fix",
            Some(r.id),
            run.fix_round_score,
            &key,
        )
        .await?;
        swept += 1;
    }
    result.swept = true;
    result.swept_rounds = swept;

    // 比赛直接结束：停止全部实例（保留逻辑实例/端点）+ run → Ended。
    let views = crate::modules::event::awdp::service::runtime::list_instances(db, run.id).await?;
    for v in views {
        if let Err(e) = crate::modules::event::awdp::service::runtime::stop_instance(
            db,
            docker,
            v.instance_id,
            subject,
        )
        .await
        {
            // best-effort：容器停止失败不阻塞计分/结束。
            tracing::warn!(
                run_id = %run.id,
                instance_id = %v.instance_id,
                error = %e,
                "ALL Check stop instance skipped"
            );
        }
    }
    run_repo::end_practice_session(db, run.id).await?;
    Ok(result)
}
