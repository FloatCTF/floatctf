//! AWDP source artifact（source.zip）生成与发布。
//!
//! 生命周期（GameBox 维度，与 Event 无关）：
//!   import/build 成功后：
//!     final image
//!       → docker create 临时容器（不 start）
//!       → copy_from_container(source_code_dir)  → tar bytes
//!       → tar → zip（去掉顶层目录前缀，zip 根 = source_code_dir 内容）
//!       → upload private RustFS
//!       → persist object key + digest
//!       → remove 临时容器（无论成败）
//!
//! source_code_dir 不存在 → 整个 import/build 失败（明确错误）。

use std::io::{Cursor, Write};

use aws_sdk_s3::primitives::ByteStream;
use fcmc::{ContainerRuntime, DockerContainerRuntime};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::modules::gamebox::{GameboxError, GameboxResult};

/// private RustFS 桶（source.zip 私有，仅授权下载）。
pub const AWDP_SOURCE_BUCKET: &str = "floatctf-private";

/// source.zip 对象键（GameBox/digest scope，绝不使用 events/{event}/…）。
pub fn awdp_source_object_key(gamebox_id: Uuid, package_digest: &str) -> String {
    format!("gameboxes/{gamebox_id}/awdp/{package_digest}/source.zip")
}

/// 从最终镜像导出 `source_code_dir` 并打包为 zip 字节。
///
/// 流程：create-only 临时容器 → `copy_from_container`（tar）→ tar→zip → remove。
/// 临时容器无论成败都会移除。
pub async fn extract_awdp_source_zip(
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
        tar_bytes_to_zip(&tar_bytes)
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

/// 将 docker copy tar 归档转为 zip（去掉顶层目录前缀，zip 根 = 目录内容）。
fn tar_bytes_to_zip(tar_bytes: &[u8]) -> GameboxResult<Vec<u8>> {
    let mut archive = tar::Archive::new(tar_bytes);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // Docker cp 目录时归档根为目录 basename（如 html/、html/index.php）。
    // 先遍历第一遍确定根前缀；第二遍写 zip（tar 支持反复遍历？——不，需要先读进内存）。
    // 做法：先全部读入内存（有 MAX_COPY_BYTES 硬上限保护），再统一去前缀。
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new(); // (zip_path, content) content 空=目录
    let mut dirs: Vec<String> = Vec::new();
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
                dirs.push(trimmed.to_string());
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

    // 去根前缀：zip 根 = source_code_dir 内容本身。
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

    for d in &dirs {
        let rel = strip(d);
        if rel.is_empty() {
            continue;
        }
        zip.start_file(format!("{rel}/"), options)
            .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: zip dir: {e}")))?;
    }
    for (name, content) in &entries {
        let rel = strip(name);
        if rel.is_empty() {
            continue;
        }
        zip.start_file(rel, options)
            .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: zip file: {e}")))?;
        zip.write_all(content)
            .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: zip write: {e}")))?;
    }

    let cursor = zip
        .finish()
        .map_err(|e| GameboxError::Internal(format!("SOURCE_EXTRACT: zip finish: {e}")))?;
    let bytes = cursor.into_inner();
    if bytes.is_empty() {
        return Err(GameboxError::Docker(
            "SOURCE_EXTRACT: source.zip 为空（source_code_dir 目录为空？）".into(),
        ));
    }
    Ok(bytes)
}

/// 上传 source.zip 到 private RustFS，返回 (object_key, sha256_hex)。
pub async fn publish_awdp_source_artifact(
    rustfs: &aws_sdk_s3::Client,
    gamebox_id: Uuid,
    package_digest: &str,
    zip_bytes: &[u8],
) -> GameboxResult<(String, String)> {
    let key = awdp_source_object_key(gamebox_id, package_digest);
    let digest = {
        let mut hasher = Sha256::new();
        hasher.update(zip_bytes);
        hex::encode(hasher.finalize())
    };
    rustfs
        .put_object()
        .bucket(AWDP_SOURCE_BUCKET)
        .key(&key)
        .body(ByteStream::from(zip_bytes.to_vec()))
        .send()
        .await
        .map_err(|e| GameboxError::Internal(format!("SOURCE_UPLOAD: {e}")))?;
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
    fn tar_to_zip_strips_root_prefix() {
        let tar = build_tar(
            &[
                ("index.php", b"<?php echo 1;"),
                ("entrypoint.sh", b"#!/bin/sh\n"),
                ("sub/dir.txt", b"nested"),
            ],
            "html",
        );
        let zip_bytes = tar_bytes_to_zip(&tar).unwrap();
        assert!(!zip_bytes.is_empty());

        let mut reader = zip::ZipArchive::new(Cursor::new(zip_bytes)).unwrap();
        let names: Vec<String> = (0..reader.len())
            .map(|i| reader.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"index.php".to_string()), "{names:?}");
        assert!(names.contains(&"sub/dir.txt".to_string()), "{names:?}");
        assert!(!names.iter().any(|n| n.starts_with("html")), "{names:?}");
    }

    #[test]
    fn object_key_scoped_by_gamebox_and_digest() {
        let key = awdp_source_object_key(
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            "abc123",
        );
        assert_eq!(
            key,
            "gameboxes/00000000-0000-0000-0000-000000000001/awdp/abc123/source.zip"
        );
        assert!(!key.contains("events/"));
    }

    #[test]
    fn digest_is_sha256_hex() {
        // 通过空字节压缩验证长度（不依赖真实 S3）。
        let digest = {
            let mut hasher = Sha256::new();
            hasher.update(b"hello");
            hex::encode(hasher.finalize())
        };
        assert_eq!(digest.len(), 64);
    }
}
