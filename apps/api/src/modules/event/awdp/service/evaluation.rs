//! AWDP 手动 Test Check（plan §24）：Healthcheck + Judge，绝不执行 exploit / 计分。

use std::time::Duration;

use bollard::Docker;
use chrono::Utc;
use fcmc::{ContainerRuntime, DockerContainerRuntime};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::infrastructure::script_runner::{parse_batch_results, run_script};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    repo::{evaluation_repo, event_gamebox_repo, event_repo, instance_repo},
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
    event_id: Uuid,
    instance_id: Uuid,
    subject: Subject,
) -> AwdpResult<ManualCheckResult> {
    let awdp = event_repo::require_by_event_id(db, event_id).await?;
    if awdp.phase != crate::entity::sea_orm_active_enums::AwdpPhase::Fix {
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
    let result = manual_check_locked(db, docker, event_id, instance_id, &instance).await;
    lock.release().await;
    result
}

async fn manual_check_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    event_id: Uuid,
    instance_id: Uuid,
    instance: &crate::entity::instances::Model,
) -> AwdpResult<ManualCheckResult> {
    let evaluation = evaluation_repo::create_manual(db, event_id, instance_id).await?;

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
    let (_eg, gamebox) =
        event_gamebox_repo::effective_gamebox_spec(db, ext.event_gamebox_id).await?;

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
