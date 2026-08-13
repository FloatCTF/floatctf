//! AWDP Patch 提交与应用（用户验证的 docker 可写层模型）。
//!
//! 流程（全程持有 instance advisory lock）：
//!   upload patch.tar.gz（Fix only）
//!     → 平台端只校验根 `patch.sh` 存在并提取；包内其余文件原样解压（结构由用户自定）
//!     → 校验当前 open round + prior-round 评估未完成 → 409
//!     → 持久化 submission=applying（archive sha256 + 文件清单）
//!     → docker cp（PUT /containers/{id}/archive）：整个包解压到容器内 /tmp/patch-<id>/，
//!       注入 FLOATCTF_SOURCE_DIR（源码目录）；解压目录固定为容器内 /tmp/patch/
//!     → 容器内 exec /bin/sh patch.sh（60s 超时）
//!     → exit 0 且未超时 → docker restart 同一容器（保留 writable layer）→ APPLIED（本轮 eligible）
//!     → 非 0 / 超时 → FAILED（不算本轮 patch）
//!
//! 玩家「不真正写 patch」的闭环：下载 source.tar.gz → 原样重新打包上传 →
//! 模板 patch.sh 为全注释空操作（exit 0）→ restart → APPLIED。
//! 不限制包大小；patch.sh 内可直接引用固定路径 /tmp/patch/ 下的任意文件。
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

/// 容器内 exec 超时。
pub const PATCH_EXEC_TIMEOUT_SECS: u64 = 60;
/// 容器内 exec 超时（stale applying 判定基准）。
pub const PATCH_APPLY_TIMEOUT_SECS: u64 = PATCH_EXEC_TIMEOUT_SECS;
/// stdout/stderr 截断上限。
pub const PATCH_OUTPUT_LIMIT: usize = 64 * 1024;

/// 一个将随包解压进容器 /tmp/patch-<id>/ 的辅助文件（relative_path 相对该目录，路径由用户包自定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchFile {
    pub relative_path: String,
    pub content: Vec<u8>,
}

/// 平台端解压后的 patch 内容（上传层产物，apply 层消费）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchPayload {
    /// 根 patch.sh 内容（容器内 /bin/sh 执行的入口脚本）。
    pub script: String,
    /// 包内其余文件（原样保留路径，解压到容器内 /tmp/patch/，供脚本直接引用）。
    pub files: Vec<PatchFile>,
    /// 原始 patch.tar.gz 字节的 sha256（作为提交内容标识）。
    pub archive_sha256: String,
}

/// 从上传字节提取 patch 内容：必须为 gzip tar（patch.tar.gz）。
///
/// 只校验根 `patch.sh` 存在并提取为入口脚本；包内其余文件原样保留路径
/// （不限制 src/ 结构、不限制大小）。安全底线：拒绝绝对路径与 `..` 段
/// （tar 条目不可信），拒绝 fifo/char 等非普通条目。
pub fn extract_patch_payload(bytes: &[u8]) -> Result<PatchPayload, String> {
    if !bytes.starts_with(&[0x1f, 0x8b]) {
        return Err("请上传 patch.tar.gz（gzip 压缩的 tar 归档，内含根 patch.sh）".to_string());
    }
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(gz);
    let entries = archive
        .entries()
        .map_err(|e| format!("patch.tar.gz 解压失败: {e}"))?;

    let mut script: Option<String> = None;
    let mut files: Vec<PatchFile> = Vec::new();
    for entry in entries {
        let mut entry = entry.map_err(|e| format!("patch.tar.gz 读取失败: {e}"))?;
        let raw = entry
            .path()
            .map_err(|e| format!("patch.tar.gz 路径解析失败: {e}"))?
            .to_string_lossy()
            .into_owned();
        if raw.ends_with("/pax_global_header") || raw.starts_with("pax_global_header") {
            continue;
        }
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            continue;
        }
        if entry_type.is_symlink() {
            // 符号链接：内容为 link target，原样写入（容器内创建，无平台风险）。
            let mut content = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut content)
                .map_err(|e| format!("patch.tar.gz 读取 {raw} 失败: {e}"))?;
            files.push(PatchFile {
                relative_path: raw,
                content,
            });
            continue;
        }
        if !entry_type.is_file() {
            return Err(format!("patch.tar.gz 不支持非普通文件: {raw}"));
        }
        if raw.starts_with('/') || raw.contains("..") || raw.split('/').any(|s| s.is_empty()) {
            return Err(format!("patch.tar.gz 路径不合法: {raw}"));
        }
        let mut content = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut content)
            .map_err(|e| format!("patch.tar.gz 读取 {raw} 失败: {e}"))?;
        if raw == "patch.sh" {
            let s = String::from_utf8(content)
                .map_err(|_| "patch.sh 必须是 UTF-8 文本脚本".to_string())?;
            script = Some(s);
        } else {
            files.push(PatchFile {
                relative_path: raw,
                content,
            });
        }
    }
    let script = script.ok_or_else(|| "patch.tar.gz 内缺少根 patch.sh 文件".to_string())?;
    let archive_sha256 = hex::encode(Sha256::digest(bytes));
    Ok(PatchPayload {
        script,
        files,
        archive_sha256,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchResult {
    Applied,
    Failed,
}

/// 提交并应用 patch（幂等入口：加锁 → cp+restart → 解锁）。
pub async fn apply_patch(
    db: &DatabaseConnection,
    docker: &Docker,
    run_id: Uuid,
    instance_id: Uuid,
    payload: &PatchPayload,
    subject: Subject,
) -> AwdpResult<PatchResult> {
    let run = run_repo::require_by_id(db, run_id).await?;
    if run.phase != AwdpPhase::Fix {
        return Err(AwdpError::InvalidState(format!(
            "patch upload only allowed during Fix (phase={:?})",
            run.phase
        )));
    }

    let lock = InstanceAdvisoryLock::acquire(db, instance_id).await?;
    let result = apply_patch_locked(db, docker, run_id, instance_id, payload, subject).await;
    lock.release().await;
    result
}

async fn apply_patch_locked(
    db: &DatabaseConnection,
    docker: &Docker,
    run_id: Uuid,
    instance_id: Uuid,
    payload: &PatchPayload,
    subject: Subject,
) -> AwdpResult<PatchResult> {
    let now = Utc::now();

    // 0. 回收 stale applying（plan §43）：平台崩溃残留的 applying（apply_started_at 超过
    //    超时 + 裕量）→ failed + reason；绝不静默视为 APPLIED，允许重新上传。
    let stale_before = now - chrono::Duration::seconds((PATCH_APPLY_TIMEOUT_SECS + 30) as i64);
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

    // 3. 持久化 submission（applying）；script_content 存文件清单（诊断/展示用）。
    let manifest: Vec<String> = vec!["patch.sh".to_string()]
        .into_iter()
        .chain(
            payload
                .files
                .iter()
                .map(|f| format!("{} ({}B)", f.relative_path, f.content.len())),
        )
        .collect();
    let submission = patch_repo::create_submission(
        db,
        run_id,
        instance_id,
        round.id,
        subject.user_id,
        subject.team_id,
        &payload.archive_sha256,
        &manifest.join(", "),
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

    let runtime = DockerContainerRuntime::new(docker.clone());
    // 解压目录固定为容器内 /tmp/patch/（每个容器独立，天然隔离）。
    const PATCH_DIR: &str = "/tmp/patch";

    // 5. 清理上次残留（同一实例多次上传复用同一目录）：先删目录再重建，
    //    避免旧 patch 的辅助文件泄漏到本次。best-effort——失败不阻断后续。
    let _ = runtime
        .exec(
            &instance.container_name,
            ExecOptions {
                cmd: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    format!("rm -rf '{PATCH_DIR}' && mkdir -p '{PATCH_DIR}'"),
                ],
                env: vec![],
                workdir: None,
                timeout: Duration::from_secs(10),
                stdout_limit: 4096,
                stderr_limit: 4096,
                stdin: None,
            },
        )
        .await;

    // 6. 把 patch 内容（patch.sh + 辅助文件）重建为 tar（相对 /tmp/patch/）。
    let tar_bytes = build_patch_tar(payload);

    // 7. docker cp：解压到容器内 /tmp/patch/（writable layer）。
    if let Err(e) = runtime
        .copy_into_container(&instance.container_name, PATCH_DIR, tar_bytes)
        .await
    {
        patch_repo::finish_apply(
            db,
            submission.id,
            false,
            None,
            "",
            "",
            Some(&format!("patch 解压到容器失败: {e}")),
        )
        .await?;
        return Err(AwdpError::Docker(format!("patch copy: {e}")));
    }

    // 8. 容器内执行 patch.sh（注入源码目录；解压目录固定 /tmp/patch/）。
    let outcome = runtime
        .exec(
            &instance.container_name,
            ExecOptions {
                cmd: vec!["/bin/sh".into(), "/tmp/patch/patch.sh".into()],
                env: vec![format!("FLOATCTF_SOURCE_DIR={source_dir}")],
                workdir: None,
                timeout: Duration::from_secs(PATCH_EXEC_TIMEOUT_SECS),
                stdout_limit: PATCH_OUTPUT_LIMIT,
                stderr_limit: PATCH_OUTPUT_LIMIT,
                stdin: None,
            },
        )
        .await
        .map_err(|e| AwdpError::Docker(format!("patch exec: {e}")))?;

    let exit_ok = outcome.exit_code == Some(0) && !outcome.timed_out;
    if !exit_ok {
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
        return Ok(PatchResult::Failed);
    }

    // 8. restart 同一容器（保留 writable layer：patch 生效）。
    if let Err(e) = runtime
        .restart_container(&instance.container_name, Duration::from_secs(10))
        .await
    {
        patch_repo::finish_apply(
            db,
            submission.id,
            false,
            None,
            &outcome.stdout,
            &outcome.stderr,
            Some(&format!("patch restart 失败: {e}")),
        )
        .await?;
        return Err(AwdpError::Docker(format!("patch restart: {e}")));
    }

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
}

/// 重建相对 /tmp/patch-<id>/ 的 tar 归档：根 patch.sh + src/ 辅助文件（保留前缀）。
fn build_patch_tar(payload: &PatchPayload) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut dirs: std::collections::HashSet<String> = std::collections::HashSet::new();

    // patch.sh 入口脚本。
    {
        let mut h = tar::Header::new_gnu();
        h.set_size(payload.script.len() as u64);
        h.set_mode(0o755);
        h.set_cksum();
        builder
            .append_data(&mut h, "patch.sh", payload.script.as_bytes())
            .unwrap();
    }
    for file in &payload.files {
        // 逐级父目录条目。
        let mut parts: Vec<&str> = file.relative_path.rsplit('/').collect();
        parts.remove(0);
        let mut acc = String::new();
        for part in parts.iter().rev() {
            if part.is_empty() {
                continue;
            }
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(part);
            let dir_name = format!("{acc}/");
            if dirs.insert(dir_name.clone()) {
                let mut h = tar::Header::new_gnu();
                h.set_entry_type(tar::EntryType::Directory);
                h.set_mode(0o755);
                h.set_size(0);
                h.set_cksum();
                builder
                    .append_data(&mut h, &dir_name, std::io::empty())
                    .unwrap();
            }
        }
        let mut h = tar::Header::new_gnu();
        h.set_size(file.content.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        builder
            .append_data(&mut h, &file.relative_path, &file.content[..])
            .unwrap();
    }
    builder.into_inner().unwrap()
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

    /// 构造 gzip tar（条目 = (path, content, entry_type)）。
    fn build_targz(entries: &[(&str, &[u8], tar::EntryType)]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        for (name, content, ty) in entries {
            let mut h = tar::Header::new_gnu();
            if *ty == tar::EntryType::Directory {
                h.set_entry_type(tar::EntryType::Directory);
                h.set_size(0);
                h.set_mode(0o755);
            } else {
                h.set_size(content.len() as u64);
                h.set_mode(0o644);
            }
            h.set_cksum();
            builder
                .append_data(&mut h, name, std::io::Cursor::new(content))
                .unwrap();
        }
        let gz = builder.into_inner().unwrap();
        gz.finish().unwrap()
    }

    fn file<'a>(name: &'a str, content: &'a str) -> (&'a str, &'a [u8], tar::EntryType) {
        (name, content.as_bytes(), tar::EntryType::Regular)
    }

    #[test]
    fn extracts_script_and_aux_files() {
        let script = "#!/bin/sh\ncp \"/tmp/patch/index.php\" \"$FLOATCTF_SOURCE_DIR/index.php\"\n";
        let bytes = build_targz(&[
            file("index.php", "<?php /* patched */"),
            file("sub/style.css", "body {}"),
            file("patch.sh", script),
        ]);
        let p = extract_patch_payload(&bytes).unwrap();
        assert_eq!(p.script, script, "根 patch.sh 作为入口脚本提取");
        let mut paths: Vec<&str> = p.files.iter().map(|f| f.relative_path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, ["index.php", "sub/style.css"]);
        assert_eq!(p.files[0].content, b"<?php /* patched */");
        assert_eq!(p.archive_sha256.len(), 64);
    }

    #[test]
    fn missing_patch_sh_reports_clear_error() {
        // 只有 src/ 文件、没有根 patch.sh
        let bytes = build_targz(&[file("src/index.php", "x")]);
        let err = extract_patch_payload(&bytes).unwrap_err();
        assert!(err.contains("缺少根 patch.sh"), "{err}");
    }

    #[test]
    fn non_utf8_patch_sh_rejected() {
        let bytes = build_targz(&[("patch.sh", &[0xff, 0xfe, 0x00][..], tar::EntryType::Regular)]);
        let err = extract_patch_payload(&bytes).unwrap_err();
        assert!(err.contains("UTF-8"), "{err}");
    }

    #[test]
    fn rejects_non_gzip() {
        let err = extract_patch_payload(b"#!/bin/sh\nexit 0\n").unwrap_err();
        assert!(err.contains("patch.tar.gz"), "{err}");
    }

    /// 手写原始 tar（不经过 tar::Builder，允许带 `..` 的路径与任意 typeflag 条目）。
    fn raw_tar_gz(entries: &[(&str, &[u8], u8)]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        for (name, content, typeflag) in entries {
            let mut h = [0u8; 512];
            let nb = name.as_bytes();
            h[..nb.len().min(100)].copy_from_slice(&nb[..nb.len().min(100)]);
            h[100..108].copy_from_slice(b"0000644\0");
            // checksum 计算时 cksum 字段位置视为 8 个空格。
            h[136..148].copy_from_slice(&[b'0'; 12]);
            h[148..156].copy_from_slice(b"        ");
            h[156] = *typeflag;
            let size = format!("{:011o}\0", content.len());
            h[124..136].copy_from_slice(size.as_bytes());
            let sum: u32 = h.iter().map(|&b| b as u32).sum();
            let cs = format!("{:06o}\0 ", sum);
            h[148..156].copy_from_slice(cs.as_bytes());
            out.extend_from_slice(&h);
            out.extend_from_slice(content);
            let rem = (512 - content.len() % 512) % 512;
            out.extend_from_slice(&vec![0u8; rem]);
        }
        out.extend_from_slice(&[0u8; 1024]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &out).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn rejects_path_traversal_and_absolute() {
        for bad in ["a/../evil.sh", "//etc/passwd", "../../x"] {
            let bytes = raw_tar_gz(&[(bad, b"x", 0)]);
            let err = extract_patch_payload(&bytes).unwrap_err();
            assert!(err.contains("路径不合法"), "{} -> {err}", bad);
        }
        // 正常路径仍通过
        let bytes = raw_tar_gz(&[
            ("index.php", b"x", 0),
            ("patch.sh", b"#!/bin/sh\nexit 0\n", 0),
        ]);
        assert!(extract_patch_payload(&bytes).is_ok());
    }

    #[test]
    fn symlink_kept_special_file_rejected() {
        // symlink（typeflag 2）：内容为 link target，原样保留（容器内创建，无平台风险）。
        let bytes = raw_tar_gz(&[
            ("link", b"index.php", b'2'),
            ("patch.sh", b"#!/bin/sh\nexit 0\n", 0),
        ]);
        let p = extract_patch_payload(&bytes).unwrap();
        assert!(
            p.files.iter().any(|f| f.relative_path == "link"),
            "{:?}",
            p.files
        );

        // fifo（typeflag 6）等特殊条目拒绝
        let bytes = raw_tar_gz(&[("fifo", b"", b'6'), ("patch.sh", b"#!/bin/sh\nexit 0\n", 0)]);
        let err = extract_patch_payload(&bytes).unwrap_err();
        assert!(err.contains("非普通文件"), "{err}");
    }

    #[test]
    fn requires_root_patch_sh_only() {
        // 无根 patch.sh → 拒绝
        let bytes = build_targz(&[file("index.php", "x")]);
        let err = extract_patch_payload(&bytes).unwrap_err();
        assert!(err.contains("缺少根 patch.sh"), "{err}");
        // 根 patch.sh 单独成包（无辅助文件）→ 允许
        let bytes = build_targz(&[file("patch.sh", "#!/bin/sh\nexit 0\n")]);
        let p = extract_patch_payload(&bytes).unwrap();
        assert!(p.files.is_empty(), "仅脚本也可成包");
        // 任意路径辅助文件 + 根 patch.sh → 允许
        let bytes = build_targz(&[
            file("any/dir/helper.txt", "x"),
            file("patch.sh", "#!/bin/sh\nexit 0\n"),
        ]);
        let p = extract_patch_payload(&bytes).unwrap();
        assert_eq!(p.files.len(), 1);
        assert_eq!(p.files[0].relative_path, "any/dir/helper.txt");
    }

    #[test]
    fn build_patch_tar_is_relative_archive() {
        let p = PatchPayload {
            archive_sha256: "0".repeat(64),
            script: "#!/bin/sh\nexit 0\n".to_string(),
            files: vec![
                PatchFile {
                    relative_path: "sub/index.php".into(),
                    content: b"p".to_vec(),
                },
                PatchFile {
                    relative_path: "top.txt".into(),
                    content: b"t".to_vec(),
                },
            ],
        };
        let tar = build_patch_tar(&p);
        let mut archive = tar::Archive::new(&tar[..]);
        let names: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"patch.sh".to_string()), "{names:?}");
        assert!(names.contains(&"sub/".to_string()), "{names:?}");
        assert!(names.contains(&"sub/index.php".to_string()), "{names:?}");
        assert!(names.contains(&"top.txt".to_string()), "{names:?}");
        assert!(!names.iter().any(|n| n.contains("..")), "{names:?}");
    }
}
