//! AWDP source artifact（source.tar.gz）生成与发布。
//!
//! 生命周期（GameBox 维度，与 Event 无关）：
//!   import/build 成功后：
//!     final image
//!       → docker create 临时容器（不 start）
//!       → copy_from_container(source_code_dir)  → tar bytes
//!       → 重打包为 tar.gz（结构：`src/` = SourcePath 源码 + 根 `patch.sh` 通用模板）
//!       → upload private RustFS
//!       → persist object key + digest
//!       → remove 临时容器（无论成败）
//!
//! source_code_dir 不存在 → 整个 import/build 失败（明确错误）。
//!
//! 玩家 Fix 阶段下载该包后，可原样重新打成 patch.tar.gz 上传（平台解压后执行根
//! `patch.sh`；模板为全注释空操作 → exit 0 → APPLIED），实现「不真正写 patch」的练习闭环。

use std::io::{Cursor, Write};

use aws_sdk_s3::primitives::ByteStream;
use fcmc::{ContainerRuntime, DockerContainerRuntime};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::modules::gamebox::{GameboxError, GameboxResult};

/// 通用 patch.sh 模板（全注释空操作；打包进 source.tar.gz，玩家可原样传回）。
pub const PATCH_TEMPLATE: &str = r##"#!/bin/sh
# ============================================================
# FloatCTF AWDP 练习 —— 通用 Patch 模板
# ------------------------------------------------------------
# 本文件是练习场景的默认 patch：全部为注释，不执行任何真实修改。
#
# 上传后平台会把 patch.sh 在你的 GameBox 容器内以 /bin/sh 执行，
# 并注入 FLOATCTF_SOURCE_DIR（源码在容器内的绝对路径）；
# 执行退出码为 0 且无超时即视为 APPLIED（本轮正式评分资格）。
#
# 如需真实修复：把修改逻辑写在本文件下方（或引用本包内其它文件），
# 再重新打成 tar.gz 上传。练习模式可直接原样传回本包。
# ============================================================
"##;

/// private RustFS 桶（source.tar.gz 私有，仅授权下载）。
pub const AWDP_SOURCE_BUCKET: &str = "floatctf-private";

/// source.tar.gz 对象键（GameBox/digest scope，绝不使用 events/{event}/…）。
pub fn awdp_source_object_key(gamebox_id: Uuid, package_digest: &str) -> String {
    format!("gameboxes/{gamebox_id}/awdp/{package_digest}/source.tar.gz")
}

/// 从最终镜像导出 `source_code_dir` 并重打包为 tar.gz（`src/` + `patch.sh`）。
///
/// 流程：create-only 临时容器 → `copy_from_container`（tar）→ 重打包 tar.gz → remove。
/// 临时容器无论成败都会移除。
pub async fn extract_awdp_source_targz(
    runtime: &DockerContainerRuntime,
    image_ref: &str,
    temp_container_name: &str,
    source_code_dir: &str,
) -> GameboxResult<Vec<u8>> {
    let spec = fcmc::ContainerSpec {
        name: temp_container_name.to_string(),
        image: image_ref.to_string(),
        env: vec![],
        labels: Default::default(),
        network_name: None,
        fixed_ip: None,
        network_aliases: vec![],
        port_bindings: vec![],
        auto_remove: true,
        resources: fcmc::ResourceLimits::default(),
        network_mode: None,
        healthcheck: None,
    };
    let handle = runtime
        .create_container(spec)
        .await
        .map_err(|e| GameboxError::Docker(format!("SOURCE_EXTRACT: create temp container: {e}")))?;

    let result = async {
        let tar_bytes = runtime
            .copy_from_container(&handle.container_id, source_code_dir)
            .await
            .map_err(|e| {
                GameboxError::Docker(format!(
                    "SOURCE_EXTRACT: copy_from_container({source_code_dir}) 失败（镜像内目录可能不存在）: {e}"
                ))
            })?;
        tar_bytes_to_targz_with_src(&tar_bytes)
    }
    .await;

    // 无论成败都移除临时容器。
    if let Err(remove_err) = runtime.remove_container(&handle.container_id, true).await {
        tracing::error!(
            container = %handle.container_id,
            error = %remove_err,
            "SOURCE_EXTRACT: remove temp container failed"
        );
    }
    result
}

/// 将 docker copy tar 归档重打包为 tar.gz。
///
/// 包结构：
/// ```text
/// source.tar.gz
/// ├── src/                     # SourcePath 源码（去 docker cp 顶层目录前缀后放入）
/// │   ├── index.php
/// │   └── ...
/// └── patch.sh                 # 通用 patch 模板（全注释；玩家可原样传回）
/// ```
fn tar_bytes_to_targz_with_src(tar_bytes: &[u8]) -> GameboxResult<Vec<u8>> {
    let mut archive = tar::Archive::new(tar_bytes);

    // Docker cp 目录时归档根为目录 basename（如 html/、html/index.php）。
    // 全部读入内存（有 MAX_COPY_BYTES 硬上限保护），再统一去前缀后放入 src/。
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new(); // (src 内相对路径, content)
    let mut root_prefix: Option<String> = None;

    {
        let entries_iter = archive
            .entries()
            .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: tar entries: {e}")))?;
        for entry in entries_iter {
            let mut entry = entry
                .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: tar entry: {e}")))?;
            let name = entry
                .path()
                .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: tar path: {e}")))?
                .to_string_lossy()
                .into_owned();
            if name.ends_with("/pax_global_header") || name.starts_with("pax_global_header") {
                continue;
            }
            let trimmed = name.trim_end_matches('/');
            if root_prefix.is_none() && !trimmed.is_empty() {
                root_prefix = Some(trimmed.to_string());
            }
            let entry_type = entry.header().entry_type();
            if entry_type.is_dir() {
                continue;
            }
            let mut content = Vec::new();
            if entry_type.is_file() || entry_type.is_hard_link() || entry_type.is_symlink() {
                // 符号链接：把 target 作为内容写入（简单可用的分发形式）。
                if entry_type.is_symlink() {
                    if let Ok(target) = entry.link_name() {
                        if let Some(t) = target {
                            content = t.to_string_lossy().into_owned().into_bytes();
                        }
                    }
                    if content.is_empty() {
                        continue;
                    }
                } else {
                    std::io::Read::read_to_end(&mut entry, &mut content).map_err(|e| {
                        GameboxError::Internal(format!("SOURCE_EXTRACT: read tar entry: {e}"))
                    })?;
                }
            } else {
                // 其它类型（fifo/char/block）跳过。
                continue;
            }
            entries.push((trimmed.to_string(), content));
        }
    }

    // 去根前缀 → src/ 下相对路径。
    let prefix_len = root_prefix
        .as_ref()
        .map(|p| p.len() + 1) // "html" + '/'
        .unwrap_or(0);
    let strip = |name: &str| -> String {
        if prefix_len > 0 && name.len() > prefix_len && name.as_bytes()[prefix_len - 1] == b'/' {
            name[prefix_len..].to_string()
        } else if prefix_len > 0 && name == root_prefix.as_deref().unwrap_or("") {
            String::new()
        } else {
            name.to_string()
        }
    };

    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(gz);

    let mut wrote_src = false;
    for (name, content) in &entries {
        let rel = strip(name);
        if rel.is_empty() {
            continue;
        }
        if !wrote_src {
            // src/ 目录条目（保持源码层级）。
            append_tar_dir(&mut builder, "src/")
                .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: tar dir: {e}")))?;
            wrote_src = true;
        }
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("src/{rel}"), &content[..])
            .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: tar write: {e}")))?;
    }

    // 根目录 patch.sh 通用模板（全注释空操作；玩家可直接原样传回）。
    {
        let mut header = tar::Header::new_gnu();
        header.set_size(PATCH_TEMPLATE.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "patch.sh", PATCH_TEMPLATE.as_bytes())
            .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: tar patch.sh: {e}")))?;
    }

    let gz = builder
        .into_inner()
        .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: tar finish: {e}")))?;
    let bytes = gz
        .finish()
        .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: gz finish: {e}")))?;
    if bytes.is_empty() {
        return Err(GameboxError::Docker(
            "SOURCE_EXTRACT: source.tar.gz 为空（source_code_dir 目录为空？）".into(),
        ));
    }
    Ok(bytes)
}

/// tar Builder 写目录条目（tar 目录名以 '/' 结尾）。
fn append_tar_dir<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    dir: &str,
) -> std::io::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_mode(0o755);
    header.set_size(0);
    header.set_cksum();
    builder.append_data(&mut header, dir, std::io::empty())
}

/// 上传 source.tar.gz 到 private RustFS，返回 (object_key, sha256_hex)。
pub async fn publish_awdp_source_artifact(
    rustfs: &aws_sdk_s3::Client,
    gamebox_id: Uuid,
    package_digest: &str,
    payload: &[u8],
) -> GameboxResult<(String, String)> {
    let key = awdp_source_object_key(gamebox_id, package_digest);
    let digest = hex::encode(Sha256::digest(payload));
    rustfs
        .put_object()
        .bucket(AWDP_SOURCE_BUCKET)
        .key(&key)
        .body(ByteStream::from(payload.to_vec()))
        .send()
        .await
        .map_err(|e| GameboxError::Internal(format!("SOURCE_UPLOAD: upload source.tar.gz: {e}")))?;
    Ok((key, digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_tar(entries: &[(&str, &[u8])], root: &str) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        let header = |name: &str| {
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Directory);
            h.set_size(0);
            h.set_mode(0o755);
            h.set_uid(0);
            h.set_gid(0);
            h.set_mtime(0);
            h.set_path(name).unwrap();
            h.set_cksum();
            h
        };
        builder.append(&header(root), &b""[..]).unwrap();
        for (name, content) in entries {
            let path = format!("{root}/{name}");
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Regular);
            h.set_size(content.len() as u64);
            h.set_mode(0o644);
            h.set_uid(0);
            h.set_gid(0);
            h.set_mtime(0);
            h.set_path(&path).unwrap();
            h.set_cksum();
            builder.append(&h, *content).unwrap();
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn targz_contains_src_and_patch_template() {
        let tar = build_tar(
            &[
                ("index.php", b"<?php echo 1;"),
                ("entrypoint.sh", b"#!/bin/sh\n"),
                ("sub/dir.txt", b"nested"),
            ],
            "html",
        );
        let bytes = tar_bytes_to_targz_with_src(&tar).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[..2], &[0x1f, 0x8b], "gzip magic");

        let gz = flate2::read::GzDecoder::new(&bytes[..]);
        let mut archive = tar::Archive::new(gz);
        let mut names: Vec<String> = Vec::new();
        let mut patch: Option<String> = None;
        for entry in archive.entries().unwrap() {
            let mut e = entry.unwrap();
            let name = e.path().unwrap().to_string_lossy().into_owned();
            names.push(name.clone());
            if name == "patch.sh" {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut e, &mut s).unwrap();
                patch = Some(s);
            }
        }
        assert!(names.contains(&"src/index.php".to_string()), "{names:?}");
        assert!(names.contains(&"src/sub/dir.txt".to_string()), "{names:?}");
        assert!(!names.iter().any(|n| n.starts_with("html")), "{names:?}");
        assert!(names.contains(&"patch.sh".to_string()), "{names:?}");
        let patch = patch.expect("patch.sh present");
        assert!(patch.contains("通用 Patch 模板"), "template content");
        assert!(
            patch.lines().all(|l| l.trim().is_empty()
                || l.trim_start().starts_with('#')
                || l.trim_start().starts_with("#!/")),
            "patch.sh must be comment-only"
        );
    }

    #[test]
    fn object_key_scoped_by_gamebox_and_digest() {
        let key = awdp_source_object_key(
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            "abc123",
        );
        assert_eq!(
            key,
            "gameboxes/00000000-0000-0000-0000-000000000001/awdp/abc123/source.tar.gz"
        );
        assert!(!key.contains("events/"));
    }

    #[test]
    fn digest_is_sha256_hex() {
        let digest = {
            let mut hasher = Sha256::new();
            hasher.update(b"hello");
            hex::encode(hasher.finalize())
        };
        assert_eq!(digest.len(), 64);
    }
}
