//! AWDP 手动 Test Check（plan §24）：Healthcheck + Judge，绝不执行 exploit / 计分。

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
}

/// 手动 Test Check：healthcheck（public endpoints）+ judge（容器内 IP，check.py 批量契约）。
pub async fn manual_check(
    db: &DatabaseConnection,
    docker: &Docker,
    run_id: Uuid,
    instance_id: Uuid,
    subject: Subject,
) -> AwdpResult<ManualCheckResult> {
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

    let lock = InstanceAdvisoryLock::acquire(db, instance_id).await?;
    let result = manual_check_locked(db, docker, run_id, instance_id, &instance).await;
    lock.release().await;
    result
}

async fn manual_check_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    run_id: Uuid,
    instance_id: Uuid,
    instance: &crate::entity::event_instances::Model,
) -> AwdpResult<ManualCheckResult> {
    let evaluation = evaluation_repo::create_manual(db, run_id, instance_id).await?;

    if instance.runtime_state != "running" {
        evaluation_repo::finish(
            db,
            evaluation.id,
            crate::entity::sea_orm_active_enums::AwdpEvaluationStatus::ServiceDown,
            Some("instance not running"),
            None,
            None,
            None,
            None,
        )
        .await?;
        return Err(AwdpError::InvalidState("instance is not running".into()));
    }

    let runtime = DockerContainerRuntime::new(docker.clone());
    let state = runtime
        .inspect_container(&instance.container_name)
        .await
        .map_err(|e| AwdpError::Docker(format!("inspect for manual check: {e}")))?;
    let container_ip = state
        .ip_address
        .clone()
        .ok_or_else(|| AwdpError::Docker("container has no ip_address".into()))?;

    let (_instance, ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
    let gamebox = event_gamebox_repo::find_gamebox_identity(db, ext.gamebox_id).await?;

    // 1. Healthcheck（public endpoints，带重试）。
    let endpoints = super::runtime::instance_endpoints_for(db, instance_id).await?;
    let healthchecks = crate::modules::gamebox::healthcheck::parse_healthchecks(
        &gamebox
            .healthchecks_json
            .clone()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    let mut health_detail = Vec::new();
    let mut health_ok = true;
    for hc in &healthchecks {
        let (port, protocol) = match hc {
            crate::modules::gamebox::healthcheck::AppHealthcheck::Http { port, .. } => {
                (*port, "http")
            }
            crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { port } => (*port, "tcp"),
        };
        let Some(ep) = endpoints
            .iter()
            .find(|e| e.protocol == protocol && e.container_port == port as i32)
        else {
            health_detail.push(format!("{protocol}:{port} 未发布端点"));
            health_ok = false;
            continue;
        };
        // 探针必须打 public 端口（容器端口映射到宿主随机 high port）。
        let public_check = match hc {
            crate::modules::gamebox::healthcheck::AppHealthcheck::Http {
                path,
                expected_status,
                ..
            } => crate::modules::gamebox::healthcheck::AppHealthcheck::Http {
                port: ep.public_port as u16,
                path: path.clone(),
                expected_status: *expected_status,
            },
            crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { .. } => {
                crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp {
                    port: ep.public_port as u16,
                }
            }
        };
        let result = crate::modules::gamebox::healthcheck::probe_one_with_retries(
            &ep.public_host,
            &public_check,
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
    let judge_content = gamebox.judge_script_content.clone();
    let judge_ok;
    let judge_detail;
    if let Some(script) = judge_content {
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
                    Some(r) => {
                        judge_ok = r.success;
                        judge_detail = r.error.clone().unwrap_or_else(|| {
                            format!("{}: {}", r.ip, if r.success { "PASS" } else { "FAIL" })
                        });
                    }
                    None => {
                        judge_ok = false;
                        judge_detail = "judge 脚本未返回该目标结果".into();
                    }
                }
            }
            Err(e) => {
                judge_ok = false;
                judge_detail = format!("judge 输出解析失败: {e}; exit={:?}", outcome.exit_code);
            }
        }
    } else {
        judge_ok = false;
        judge_detail = "GameBox 未配置 judge 脚本".into();
    }

    // 3. 落库（manual 不计分；health+judge 全过 → patched 语义，exploit 恒 NULL）。
    let status = if !health_ok {
        crate::entity::sea_orm_active_enums::AwdpEvaluationStatus::ServiceDown
    } else if !judge_ok {
        crate::entity::sea_orm_active_enums::AwdpEvaluationStatus::FunctionalBroken
    } else {
        crate::entity::sea_orm_active_enums::AwdpEvaluationStatus::Patched
    };
    evaluation_repo::finish(
        db,
        evaluation.id,
        status,
        Some(&health_detail.join("; ")),
        Some(&judge_detail),
        None,
        None,
        None,
    )
    .await?;

    Ok(ManualCheckResult {
        healthcheck_ok: health_ok,
        healthcheck_detail: health_detail,
        judge_ok,
        judge_detail,
    })
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
    let claimed = evaluation_repo::claim_jobs(
        db,
        worker_id,
        concurrency,
        lease_duration_secs,
        max_attempts,
        &[AwdpEvaluationKind::Official],
    )
    .await?;
    let mut n = 0usize;
    for job in claimed {
        let ev = job.evaluation;
        match process_official_leased(db, docker, worker_id, &ev, &job.lease_token, job.attempt)
            .await
        {
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

/// 共享官方评估管线：NO_PATCH 短路 → Healthcheck → Judge → Exploit → 计分。
/// 返回终态（worker 与练习提前 Check 共用，保证语义一致）。
///
/// 有 lease 时（lease_token 非空）用 `finish_with_lease` 写终态（stale 拒绝）；
/// 无 lease（练习提前 Check 进程内同步路径）用 `finish`。
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

    // 1. NO_PATCH 短路（plan §21/§33：本轮无 APPLIED patch → +0，不浪费资源）。
    if !patch_repo::has_applied_patch(db, ev.instance_id, round.id).await? {
        finish_eval(
            db,
            ev,
            worker_id,
            lease_token,
            attempt,
            S::NoPatch,
            Some("no applied patch this round"),
            None,
            None,
            None,
            None,
        )
        .await?;
        return Ok(S::NoPatch);
    }

    // 容器可能已停 → 评估前确保运行（评估官方语义：对当前 instance 状态判）。
    // 计分主体取实例归属（competition 的 run 无主体；practice 与 run 一致）。
    let (instance, ext) = instance_repo::find_by_instance_id(db, ev.instance_id).await?;
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

    // 2. Healthcheck（public endpoints，重试）。
    let endpoints = super::runtime::instance_endpoints_for(db, ev.instance_id).await?;
    let healthchecks = crate::modules::gamebox::healthcheck::parse_healthchecks(
        &gamebox
            .healthchecks_json
            .clone()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    let mut health_ok = true;
    let mut health_detail = Vec::new();
    for hc in &healthchecks {
        let (port, protocol) = match hc {
            crate::modules::gamebox::healthcheck::AppHealthcheck::Http { port, .. } => {
                (*port, "http")
            }
            crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { port } => (*port, "tcp"),
        };
        let Some(ep) = endpoints
            .iter()
            .find(|e| e.protocol == protocol && e.container_port == port as i32)
        else {
            health_ok = false;
            health_detail.push(format!("{protocol}:{port} 未发布"));
            continue;
        };
        let public_check = match hc {
            crate::modules::gamebox::healthcheck::AppHealthcheck::Http {
                path,
                expected_status,
                ..
            } => crate::modules::gamebox::healthcheck::AppHealthcheck::Http {
                port: ep.public_port as u16,
                path: path.clone(),
                expected_status: *expected_status,
            },
            crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { .. } => {
                crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp {
                    port: ep.public_port as u16,
                }
            }
        };
        let r = crate::modules::gamebox::healthcheck::probe_one_with_retries(
            &ep.public_host,
            &public_check,
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
        finish_eval(
            db,
            ev,
            worker_id,
            lease_token,
            attempt,
            S::ServiceDown,
            Some(&health_detail.join("; ")),
            None,
            None,
            None,
            None,
        )
        .await?;
        return Ok(S::ServiceDown);
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
        finish_eval(
            db,
            ev,
            worker_id,
            lease_token,
            attempt,
            S::FunctionalBroken,
            Some(&health_detail.join("; ")),
            Some(&judge_detail),
            None,
            None,
            None,
        )
        .await?;
        return Ok(S::FunctionalBroken);
    }

    // 4. Official exploit（平台侧执行；成功 = VULNERABLE +0，失败 = PATCHED +score）。
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

    if exploit_ok {
        // VULNERABLE → +0（记录即可）。
        finish_eval(
            db,
            ev,
            worker_id,
            lease_token,
            attempt,
            S::Vulnerable,
            Some(&health_detail.join("; ")),
            Some(&judge_detail),
            Some(&exploit_detail),
            None,
            None,
        )
        .await?;
        Ok(S::Vulnerable)
    } else {
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
            Some(&health_detail.join("; ")),
            Some(&judge_detail),
            Some(&exploit_detail),
            None,
            None,
        )
        .await?;
        Ok(S::Patched)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Practice 提前 Check（练习模式）：一次 check 成功 → 从该轮起全部回合自动计分
// ────────────────────────────────────────────────────────────────────────────

/// 提前 Check 结果（练习模式；status=Patched 时触发自动计分）。
#[derive(Debug, Clone)]
pub struct EarlyCheckResult {
    pub status: crate::entity::sea_orm_active_enums::AwdpEvaluationStatus,
    /// status == Patched（本轮起自动计分）。
    pub swept: bool,
    /// 自动计分的回合数（当前轮起含：total_rounds - target_round + 1）。
    pub swept_rounds: i32,
    pub target_round: i32,
    pub healthcheck_result: Option<String>,
    pub judge_result: Option<String>,
    pub exploit_result: Option<String>,
}

/// 练习提前 Check：对下一未完成回合立即执行官方评估管线（healthcheck→judge→exploit）。
/// PATCHED（修复成功）→ 从该轮起（含）全部剩余回合直接计分 + 结算评估（幂等）。
/// 仅练习 run（gamebox_id 非空）且 Fix 阶段可用；exploit 全程平台侧执行，不下发给玩家。
pub async fn early_check(
    db: &DatabaseConnection,
    docker: &Docker,
    run_id: Uuid,
    instance_id: Uuid,
    subject: Subject,
) -> AwdpResult<EarlyCheckResult> {
    use crate::entity::sea_orm_active_enums::AwdpPhase;
    let run = run_repo::require_by_id(db, run_id).await?;
    if run.gamebox_id.is_none() {
        return Err(AwdpError::InvalidState("提前 Check 仅练习模式可用".into()));
    }
    if run.phase != AwdpPhase::Fix {
        return Err(AwdpError::InvalidState(
            "提前 Check 仅在 Fix 阶段可用".into(),
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
    let result = early_check_locked(db, docker, &run, instance_id).await;
    lock.release().await;
    result
}

async fn early_check_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    run: &crate::entity::awdp_runs::Model,
    instance_id: Uuid,
) -> AwdpResult<EarlyCheckResult> {
    use crate::entity::sea_orm_active_enums::AwdpEvaluationStatus as S;

    // 目标回合：下一个未完成回合（pending 或 evaluating）——即「当前 turn」。
    let round = round_repo::next_due_round(db, run.id)
        .await?
        .ok_or_else(|| AwdpError::InvalidState("所有回合均已评估/结束".into()))?;
    let (gamebox, ext) = {
        let (_inst, ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
        (
            event_gamebox_repo::find_gamebox_identity(db, ext.gamebox_id).await?,
            ext,
        )
    };

    // 物化 official 评估（round × instance 唯一；幂等返回既有行）。
    let ev = evaluation_repo::create_official(db, run.id, instance_id, round.id).await?;

    // 若该轮评估已是终态：PATCHED 直接复用（避免 NO_PATCH 覆盖已确认修复），
    // 其余终态（VULNERABLE/BROKEN/DOWN）重跑给玩家二次机会；pending/running 则执行。
    let terminal = match &ev.status {
        S::Pending | S::Running => None,
        s => Some(s.clone()),
    };
    if terminal.is_none() || terminal != Some(S::Patched) {
        run_official_pipeline(db, docker, run, &gamebox, &ev, &round, "", "", 0).await?;
    }
    // 重取终态（finish 之后 ev 内存态过期）。
    let fresh = evaluation_repo::find_by_id(db, ev.id).await?;
    let status = fresh.status;

    let mut result = EarlyCheckResult {
        status: status.clone(),
        swept: false,
        swept_rounds: 0,
        target_round: round.sequence,
        healthcheck_result: fresh.healthcheck_result.clone(),
        judge_result: fresh.judge_result.clone(),
        exploit_result: fresh.exploit_result.clone(),
    };

    if status == S::Patched {
        // 从当前轮起（含）全部剩余回合自动计分（幂等账本 + 幂等评估结算）。
        run_repo::set_early_patched(db, run.id, round.sequence).await?;
        let mut swept = 0i32;
        for seq in round.sequence..=run.total_rounds {
            let Some(r) = round_repo::find_by_sequence(db, run.id, seq).await? else {
                continue;
            };
            if seq != round.sequence {
                let eval = evaluation_repo::create_official(db, run.id, instance_id, r.id).await?;
                evaluation_repo::finish(
                    db,
                    eval.id,
                    S::Patched,
                    Some("early check: 修复已确认，自动计分"),
                    None,
                    None,
                    None,
                    None,
                )
                .await?;
            }
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
    }
    Ok(result)
}
