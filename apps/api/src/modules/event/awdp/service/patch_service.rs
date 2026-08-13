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

/// 从上传文件字节中提取 patch.sh 脚本内容。
///
/// - gzip（`patch.tar.gz`）：解压 tar 归档，取根目录/任意层级的 `patch.sh`；
/// - 其它字节：按旧格式兼容（裸 .sh 文本，UTF-8）。
///
/// 返回错误消息面向玩家（Upload 层转 Validation/BadRequest）。
pub fn extract_patch_script(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        // gzip tar：patch.tar.gz
        let gz = flate2::read::GzDecoder::new(bytes);
        let mut archive = tar::Archive::new(gz);
        let entries = archive
            .entries()
            .map_err(|e| format!("patch.tar.gz 解压失败: {e}"))?;
        let mut found: Option<String> = None;
        for entry in entries {
            let mut entry = entry.map_err(|e| format!("patch.tar.gz 读取失败: {e}"))?;
            let path = entry
                .path()
                .map_err(|e| format!("patch.tar.gz 路径解析失败: {e}"))?
                .to_string_lossy()
                .into_owned();
            if path.ends_with("/pax_global_header") || path.starts_with("pax_global_header") {
                continue;
            }
            let fname = path.rsplit('/').next().unwrap_or("");
            if fname == "patch.sh" && entry.header().entry_type().is_file() {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut entry, &mut s)
                    .map_err(|_| "patch.sh 必须是 UTF-8 文本脚本".to_string())?;
                found = Some(s);
                break;
            }
        }
        found.ok_or_else(|| "patch.tar.gz 内缺少 patch.sh 文件".to_string())
    } else {
        // 旧格式：裸 .sh 文本
        String::from_utf8(bytes.to_vec()).map_err(|_| "patch.sh 必须是 UTF-8 文本脚本".to_string())
    }
}

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

    // 0. 回收 stale applying（plan §43）：平台崩溃残留的 applying（apply_started_at 超过
    //    exec 超时 + 裕量）→ failed + reason；绝不静默视为 APPLIED，允许重新上传。
    let stale_before = now - chrono::Duration::seconds((PATCH_EXEC_TIMEOUT_SECS + 30) as i64);
    let recovered = patch_repo::recover_stale_applying(db, instance_id, stale_before).await?;
    if recovered > 0 {
        tracing::warn!(
            instance_id = %instance_id,
            recovered,
            "[Patch] stale applying recovered"
        );
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn build_targz(patch: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        let mut h = tar::Header::new_gnu();
        h.set_size(patch.len() as u64);
        h.set_mode(0o755);
        h.set_cksum();
        builder
            .append_data(&mut h, "patch.sh", patch.as_bytes())
            .unwrap();
        for (name, content) in extra {
            let mut h = tar::Header::new_gnu();
            h.set_size(content.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            builder.append_data(&mut h, name, &content[..]).unwrap();
        }
        let gz = builder.into_inner().unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn extracts_patch_sh_from_targz() {
        let script = "#!/bin/sh\necho patch\n";
        let bytes = build_targz(script, &[("src/index.php", &b"<?php"[..])]);
        assert_eq!(extract_patch_script(&bytes).unwrap(), script);
    }

    #[test]
    fn missing_patch_sh_reports_clear_error() {
        // 构造不含 patch.sh 的 tar.gz
        let no_patch = {
            let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            let mut builder = tar::Builder::new(gz);
            let mut h = tar::Header::new_gnu();
            h.set_size(4);
            h.set_cksum();
            builder
                .append_data(&mut h, "src/x.txt", &b"data"[..])
                .unwrap();
            let gz = builder.into_inner().unwrap();
            gz.finish().unwrap()
        };
        let err = extract_patch_script(&no_patch).unwrap_err();
        assert!(err.contains("缺少 patch.sh"), "{err}");
    }

    #[test]
    fn plain_sh_text_passthrough() {
        let script = "#!/bin/sh\nexit 0\n";
        assert_eq!(extract_patch_script(script.as_bytes()).unwrap(), script);
    }

    #[test]
    fn non_utf8_sh_rejected() {
        let err = extract_patch_script(&[0xff, 0xfe, 0x00, 0x01]).unwrap_err();
        assert!(err.contains("UTF-8"), "{err}");
    }
}
