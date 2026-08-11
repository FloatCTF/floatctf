//! AWDP Patch 提交与执行（plan §21/§22/§23，run 中心化）。
//!
//! 流程（全程持有 instance advisory lock）：
//!   upload patch.sh（Fix only）
//!     → 校验当前 open round + prior-round 评估未完成 → 409
//!     → 持久化 submission=applying（sha256 + 内容）
//!     → 容器内 exec：/bin/sh -s（stdin=脚本），注入 FLOATCTF_SOURCE_DIR
//!     → exit 0 → restart 同一容器 → APPLIED（本轮 eligible）
//!     → exit != 0 / 超时 → FAILED（不算本轮 patch）
//!
//! 竞态纪律（plan §23）：Patch / Official evaluation / Reset / Manual check
//! 全部使用同一把 instance advisory lock；本轮 cutoff 已过但评估未完成时拒绝修改容器。

use std::time::Duration;

use bollard::Docker;
use chrono::Utc;
use fcmc::{ContainerRuntime, DockerContainerRuntime, ExecOptions};
use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::AwdpPhase;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    repo::{evaluation_repo, event_gamebox_repo, instance_repo, patch_repo, round_repo, run_repo},
    service::{lock::InstanceAdvisoryLock, runtime::Subject},
};

/// patch.sh 大小上限（256 KiB）。
pub const MAX_PATCH_BYTES: usize = 256 * 1024;
/// 容器内 exec 超时。
pub const PATCH_EXEC_TIMEOUT_SECS: u64 = 60;
/// stdout/stderr 截断上限。
pub const PATCH_OUTPUT_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchResult {
    Applied,
    Failed,
}

/// 提交并应用 patch（幂等入口：加锁 → 执行 → 解锁）。
pub async fn apply_patch(
    db: &DatabaseConnection,
    docker: &Docker,
    run_id: Uuid,
    instance_id: Uuid,
    script: &str,
    subject: Subject,
) -> AwdpResult<PatchResult> {
    let run = run_repo::require_by_id(db, run_id).await?;
    if run.phase != AwdpPhase::Fix {
        return Err(AwdpError::InvalidState(format!(
            "patch upload only allowed during Fix (phase={:?})",
            run.phase
        )));
    }
    if script.len() > MAX_PATCH_BYTES {
        return Err(AwdpError::Validation(format!(
            "patch.sh 超过 {} KiB 上限",
            MAX_PATCH_BYTES / 1024
        )));
    }

    let lock = InstanceAdvisoryLock::acquire(db, instance_id).await?;
    let result = apply_patch_locked(db, docker, run_id, instance_id, script, subject).await;
    lock.release().await;
    result
}

async fn apply_patch_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    run_id: Uuid,
    instance_id: Uuid,
    script: &str,
    subject: Subject,
) -> AwdpResult<PatchResult> {
    let now = Utc::now();

    // 1. 当前 open round。
    let Some(round) = round_repo::current_open_round(db, run_id, now).await? else {
        return Err(AwdpError::InvalidState(
            "当前没有进行中的 Fix 回合（可能正处于评估窗口）".into(),
        ));
    };

    // 2. prior-round 评估未完成 → 拒绝修改容器（plan §23 竞态）。
    if let Some(prior) = round_repo::next_due_round(db, run_id).await? {
        if prior.id != round.id
            && prior.status != "completed"
            && evaluation_repo::has_unfinished_for_instance(db, instance_id, prior.id).await?
        {
            return Err(AwdpError::Conflict("Round evaluation in progress".into()));
        }
    }

    // 3. 持久化 submission（applying）。
    let script_sha = {
        let mut h = Sha256::new();
        h.update(script.as_bytes());
        hex::encode(h.finalize())
    };
    let submission = patch_repo::create_submission(
        db,
        run_id,
        instance_id,
        round.id,
        subject.user_id,
        subject.team_id,
        &script_sha,
        script,
    )
    .await?;

    // 4. 容器必须 running。
    let (instance, ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
    if instance.runtime_state != "running" {
        patch_repo::finish_apply(
            db,
            submission.id,
            false,
            None,
            "",
            "",
            Some("instance 未运行，无法应用 patch"),
        )
        .await?;
        return Err(AwdpError::InvalidState("instance is not running".into()));
    }
    let gamebox = event_gamebox_repo::find_gamebox_identity(db, ext.gamebox_id).await?;
    let source_dir = gamebox
        .awdp_source_code_dir
        .clone()
        .unwrap_or_else(|| "/app".to_string());

    // 5. 容器内 exec：/bin/sh -s（stdin=patch.sh），注入 FLOATCTF_SOURCE_DIR。
    let runtime = DockerContainerRuntime::new(docker.clone());
    let outcome = runtime
        .exec(
            &instance.container_name,
            ExecOptions {
                cmd: vec!["/bin/sh".into(), "-s".into()],
                env: vec![format!("FLOATCTF_SOURCE_DIR={source_dir}")],
                workdir: None,
                timeout: Duration::from_secs(PATCH_EXEC_TIMEOUT_SECS),
                stdout_limit: PATCH_OUTPUT_LIMIT,
                stderr_limit: PATCH_OUTPUT_LIMIT,
                stdin: Some(script.as_bytes().to_vec()),
            },
        )
        .await
        .map_err(|e| AwdpError::Docker(format!("patch exec: {e}")))?;

    let exit_ok = outcome.exit_code == Some(0) && !outcome.timed_out;
    if exit_ok {
        // 6. restart 同一容器（保留 writable layer：patch 生效）。
        runtime
            .restart_container(&instance.container_name, Duration::from_secs(10))
            .await
            .map_err(|e| AwdpError::Docker(format!("patch restart: {e}")))?;
        patch_repo::finish_apply(
            db,
            submission.id,
            true,
            outcome.exit_code.map(|c| c as i32),
            &outcome.stdout,
            &outcome.stderr,
            None,
        )
        .await?;
        Ok(PatchResult::Applied)
    } else {
        patch_repo::finish_apply(
            db,
            submission.id,
            false,
            outcome.exit_code.map(|c| c as i32),
            &outcome.stdout,
            &outcome.stderr,
            Some(if outcome.timed_out {
                "patch script timed out"
            } else {
                "patch script exited nonzero"
            }),
        )
        .await?;
        Ok(PatchResult::Failed)
    }
}

/// 当前主体的最近 patch 提交（前端展示 applying/applied/failed）。
pub async fn latest_for_subject_instance(
    db: &DatabaseConnection,
    run_id: Uuid,
    instance_id: Uuid,
    subject: Subject,
) -> AwdpResult<Option<crate::entity::awdp_patch_submissions::Model>> {
    let _ = run_id;
    let Some(row) = patch_repo::latest_for_instance(db, instance_id).await? else {
        return Ok(None);
    };
    let owned = match subject {
        Subject {
            user_id: Some(u),
            team_id: None,
        } => row.user_id == Some(u),
        Subject {
            user_id: None,
            team_id: Some(t),
        } => row.team_id == Some(t),
        _ => false,
    };
    if !owned {
        return Ok(None);
    }
    Ok(Some(row))
}
