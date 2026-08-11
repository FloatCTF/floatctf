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
    repo::{evaluation_repo, event_gamebox_repo, instance_repo, patch_repo, run_repo, score_repo},
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
    instance: &crate::entity::instances::Model,
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

/// worker 单轮：领取 pending 评估（SKIP LOCKED）并逐一执行。
pub async fn worker_round(
    db: &DatabaseConnection,
    docker: &Docker,
    concurrency: u64,
) -> AwdpResult<usize> {
    let claimed = evaluation_repo::claim_pending(db, concurrency).await?;
    let mut n = 0usize;
    for ev in claimed {
        match process_official(db, docker, &ev).await {
            Ok(()) => n += 1,
            Err(e) => {
                // 平台内部失败：PLATFORM_ERROR，可 retry（与 VULNERABLE/BROKEN 区分）。
                let _ = evaluation_repo::finish(
                    db,
                    ev.id,
                    crate::entity::sea_orm_active_enums::AwdpEvaluationStatus::PlatformError,
                    None,
                    None,
                    None,
                    None,
                    Some(&format!("{e}")),
                )
                .await;
                tracing::warn!(evaluation_id = %ev.id, error = %e, "AWDP evaluation failed");
            }
        }
    }
    Ok(n)
}

/// 执行单条 official 评估：NO_PATCH → Healthcheck → Judge → Exploit → Score。
async fn process_official(
    db: &DatabaseConnection,
    docker: &Docker,
    ev: &crate::entity::awdp_evaluations::Model,
) -> AwdpResult<()> {
    let lock = InstanceAdvisoryLock::acquire(db, ev.instance_id).await?;
    let result = process_official_locked(db, docker, ev).await;
    lock.release().await;
    result
}

async fn process_official_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    ev: &crate::entity::awdp_evaluations::Model,
) -> AwdpResult<()> {
    use crate::entity::sea_orm_active_enums::AwdpEvaluationStatus as S;

    let (_instance, ext) = instance_repo::find_by_instance_id(db, ev.instance_id).await?;
    let run = run_repo::require_by_id(db, ext.run_id).await?;
    let gamebox = event_gamebox_repo::find_gamebox_identity(db, ext.gamebox_id).await?;
    let round_id = ev
        .fix_round_id
        .ok_or_else(|| AwdpError::Internal("official evaluation missing fix_round_id".into()))?;

    // 1. NO_PATCH 短路（plan §21/§33：本轮无 APPLIED patch → +0，不浪费资源）。
    if !patch_repo::has_applied_patch(db, ev.instance_id, round_id).await? {
        evaluation_repo::finish(
            db,
            ev.id,
            S::NoPatch,
            Some("no applied patch this round"),
            None,
            None,
            None,
            None,
        )
        .await?;
        return Ok(());
    }

    let instance = {
        // 容器可能已停 → 评估前确保运行（评估官方语义：对当前 instance 状态判）。
        let (inst, _) = instance_repo::find_by_instance_id(db, ev.instance_id).await?;
        inst
    };
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
        evaluation_repo::finish(
            db,
            ev.id,
            S::ServiceDown,
            Some(&health_detail.join("; ")),
            None,
            None,
            None,
            None,
        )
        .await?;
        return Ok(());
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
        evaluation_repo::finish(
            db,
            ev.id,
            S::FunctionalBroken,
            Some(&health_detail.join("; ")),
            Some(&judge_detail),
            None,
            None,
            None,
        )
        .await?;
        return Ok(());
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
        evaluation_repo::finish(
            db,
            ev.id,
            S::Vulnerable,
            Some(&health_detail.join("; ")),
            Some(&judge_detail),
            Some(&exploit_detail),
            None,
            None,
        )
        .await?;
    } else {
        // PATCHED → +fix_round_score（幂等键 awdp:fix:{run}:{round}:{instance}）。
        let key = fix_idempotency_key(ext.run_id, round_id, ev.instance_id);
        let _scored = score_repo::create_score_event(
            db,
            ext.run_id,
            ext.owner_user_id,
            ext.owner_team_id,
            ext.gamebox_id,
            "fix",
            Some(round_id),
            run.fix_round_score,
            &key,
        )
        .await?;
        evaluation_repo::finish(
            db,
            ev.id,
            S::Patched,
            Some(&health_detail.join("; ")),
            Some(&judge_detail),
            Some(&exploit_detail),
            None,
            None,
        )
        .await?;
    }
    Ok(())
}
