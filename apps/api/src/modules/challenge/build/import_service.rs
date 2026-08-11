//! 题目包导入服务。
//!
//! 流程与 GameBox 导入对称：安全解压 → 规格规范化 → 构建/推送 → 登记身份。

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use bollard::Docker;
use fcmc::{
    ArtifactKind, ChallengeMetaError, DockerContainerRuntime, ImageBuildRequest, ImageError,
    ImageRuntime, RegistryAuth, build_artifact_image_ref,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    TransactionTrait,
};
use tracing::{error, info, warn};

use crate::api::AppError;
use crate::core::config::RegistryConfig;
use crate::entity::{challenges, prelude::Challenges};
use crate::infrastructure::package::{
    self, compute_package_digest, compute_spec_digest, discover_package_root, extract_package_zip,
    read_meta_toml, read_package_file, require_package_layout, sanitize_build_error, sha256_hex,
    version_gate_reason,
};
use crate::infrastructure::settings::get_setting;

pub const BUILD_STATUS_BUILDING: &str = "building";
pub const BUILD_STATUS_READY: &str = "ready";
pub const BUILD_STATUS_FAILED: &str = "failed";

/// 附件大小上限（有界；与包内单文件限制留余量一致）。
const MAX_ATTACHMENT_BYTES: u64 = 64 * 1024 * 1024;

/// import (returned to admin API)的结果。
#[derive(Debug, Clone)]
pub struct ImportChallengeResult {
    pub challenge: challenges::Model,
}

/// 导入 Challenge 包 zip（multipart 临时文件路径）。
///
/// 单版本模型：identity 直接承载当前版本全部 package 字段；导入要求 version 严格递增。
/// 使用平台 `RegistryConfig` 作为镜像前缀 / 推送模式 / 凭证。
pub async fn import_challenge_package(
    db: &DatabaseConnection,
    docker: &Docker,
    registry: &RegistryConfig,
    zip_path: &Path,
) -> Result<ImportChallengeResult, AppError> {
    // ── 1. Extract + discover ──────────────────────────────────────────────
    let tmp = tempfile::tempdir()
        .map_err(|e| AppError::Internal(format!("tempdir for challenge import: {e}")))?;
    extract_package_zip(zip_path, tmp.path()).map_err(map_package_error)?;
    let package_root = discover_package_root(tmp.path()).map_err(map_package_error)?;
    require_package_layout(&package_root).map_err(map_package_error)?;

    let source_toml = read_meta_toml(&package_root).map_err(map_package_error)?;
    let meta = fcmc::ChallengeMeta::parse_and_validate(&source_toml).map_err(map_meta_error)?;
    let safe_name = meta.resolved_safe_name().map_err(map_meta_error)?;
    let version = meta.version.clone();
    let normalized = meta.normalize().map_err(map_meta_error)?;

    // ── 2. 版本门禁（先比对，任何写操作之前）─────────────────────────────
    let existing = Challenges::find()
        .filter(challenges::Column::SafeName.eq(&safe_name))
        .one(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(reason) = version_gate_reason(
        &version,
        existing.as_ref().and_then(|c| c.version.as_deref()),
    ) {
        warn!(
            safe_name = %safe_name,
            incoming_version = %version,
            current_version = ?existing.as_ref().and_then(|c| c.version.as_deref()),
            reason = %reason,
            "Challenge import rejected by version gate"
        );
        return Err(AppError::Conflict(reason));
    }
    info!(
        safe_name = %safe_name,
        version = %version,
        "Challenge package import started"
    );

    let package_digest =
        compute_package_digest(&package_root, &["src", "attachment"]).map_err(map_package_error)?;
    let spec_digest = compute_spec_digest(&normalized).map_err(map_package_error)?;
    let spec_json = serde_json::to_value(&normalized)
        .map_err(|e| AppError::Internal(format!("spec_json: {e}")))?;

    // Flag semantics (explicit tagged union).
    let flag_type = normalized.flag_type.clone();
    let static_flag_value = meta.static_flag_value().map(str::to_string);

    // Dynamic flag 必须能注入容器（无 [docker] 的题目无法交付动态 flag）。
    if flag_type == "dynamic" && normalized.container_port.is_none() {
        return Err(AppError::Validation(
            "CHALLENGE_INVALID_FLAG_CONFIG: dynamic flag requires [docker] section".into(),
        ));
    }

    // Attachment metadata (never part of docker context).
    let (attachment_path, attachment_name, attachment_size, attachment_sha) = match meta.attachment
    {
        Some(ref rel) => {
            let bytes = read_package_file(&package_root, rel, MAX_ATTACHMENT_BYTES)
                .map_err(map_package_error)?;
            let name = Path::new(rel)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(rel)
                .to_string();
            (
                Some(rel.clone()),
                Some(name),
                Some(bytes.len() as i64),
                Some(sha256_hex(&bytes)),
            )
        }
        None => (None, None, None, None),
    };

    let image_ref = build_artifact_image_ref(
        ArtifactKind::Challenge,
        &registry.image_prefix,
        &safe_name,
        &version,
    );
    let resources = &normalized.recommended_resources;

    // ── 3. 单版本 upsert：identity + building 状态（事务）──────────────────
    let challenge = {
        let txn = db
            .begin()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let challenge = match existing {
            Some(existing) => existing,
            None => challenges::ActiveModel {
                name: Set(normalized.name.clone()),
                safe_name: Set(safe_name.clone()),
                category: Set(normalized.category.clone()),
                description: Set(normalized.description.clone()),
                hidden: Set(false),
                ..Default::default()
            }
            .insert(&txn)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?,
        };

        // 覆盖全部 package 字段（identity 字段 name/category/description 保持 admin 可编辑，导入不重写）。
        let mut am: challenges::ActiveModel = challenge.clone().into();
        am.version = Set(Some(version.clone()));
        am.source_toml = Set(Some(source_toml.clone()));
        am.spec_json = Set(Some(spec_json.clone()));
        am.spec_digest = Set(Some(spec_digest.clone()));
        am.package_digest = Set(Some(package_digest.clone()));
        am.flag_type = Set(Some(flag_type.clone()));
        am.static_flag_value = Set(static_flag_value.clone());
        am.container_port = Set(normalized.container_port.map(|p| p as i32));
        am.recommended_cpu_millis = Set(resources.cpu_millis);
        am.recommended_memory_bytes = Set(resources.memory_bytes);
        am.recommended_pids_limit = Set(resources.pids_limit);
        am.attachment_path = Set(attachment_path.clone());
        am.attachment_name = Set(attachment_name.clone());
        am.attachment_size = Set(attachment_size);
        am.attachment_sha256 = Set(attachment_sha.clone());
        am.image_ref = Set(Some(image_ref.clone()));
        am.image_id = Set(None);
        am.image_repo_digest = Set(None);
        am.build_status = Set(Some(BUILD_STATUS_BUILDING.to_string()));
        am.build_error = Set(None);
        am.updated_at = Set(chrono::Utc::now().into());
        let challenge = am
            .update(&txn)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        txn.commit()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        challenge
    };

    // ── 4. Build outside txn (synchronous v1) ──────────────────────────────
    let context_dir = package_root.join("src");
    let short_id = &challenge.id.to_string().replace('-', "")[..8];
    let temp_tag = format!("{image_ref}-import-{short_id}");

    let mut labels = HashMap::new();
    labels.insert("io.floatctf.managed".into(), "true".into());
    labels.insert("io.floatctf.resource".into(), "challenge-image".into());
    labels.insert("io.floatctf.safe_name".into(), safe_name.clone());
    labels.insert("io.floatctf.version".into(), version.clone());
    labels.insert("io.floatctf.package.digest".into(), package_digest.clone());

    let runtime = DockerContainerRuntime::new(docker.clone());
    let build_req = ImageBuildRequest {
        context_dir: context_dir.clone(),
        dockerfile: "Dockerfile".into(),
        target_ref: temp_tag.clone(),
        labels,
        timeout: Duration::from_secs(registry.build_timeout_secs),
        verbose: false,
        build_proxy: None,
    };

    let build_outcome = run_build_and_pin(&runtime, registry, &build_req, &image_ref, &temp_tag)
        .await
        .map_err(map_image_error);

    // ── 5. ready or failed ─────────────────────────────────────────────────
    match build_outcome {
        Ok((image_id, image_repo_digest)) => {
            let mut am: challenges::ActiveModel = challenge.clone().into();
            am.image_ref = Set(Some(image_ref.clone()));
            am.image_id = Set(Some(image_id.clone()));
            am.image_repo_digest = Set(image_repo_digest.clone());
            am.build_status = Set(Some(BUILD_STATUS_READY.to_string()));
            am.build_error = Set(None);
            am.updated_at = Set(chrono::Utc::now().into());
            let challenge = am
                .update(db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;

            // Mirror package (src/ + attachment/) into CHALLENGES_DIR for static serving.
            mirror_to_challenges_dir(db, &safe_name, &package_root).await;

            info!(
                challenge_id = %challenge.id,
                safe_name = %safe_name,
                version = %version,
                image_ref = %image_ref,
                image_repo_digest = ?image_repo_digest,
                package_digest = %package_digest,
                "Challenge package import ready"
            );
            // 尽力清理临时 tag（规范 image_ref 仍保留 tag）。
            let _ = ImageRuntime::remove_image(&runtime, &temp_tag, true).await;
            Ok(ImportChallengeResult { challenge })
        }
        Err(e) => {
            let sanitized = sanitize_build_error(&e.to_string());
            error!(
                challenge_id = %challenge.id,
                safe_name = %safe_name,
                version = %version,
                error = %sanitized,
                "Challenge package import build failed"
            );
            let mut am: challenges::ActiveModel = challenge.clone().into();
            am.build_status = Set(Some(BUILD_STATUS_FAILED.to_string()));
            am.build_error = Set(Some(sanitized.clone()));
            am.updated_at = Set(chrono::Utc::now().into());
            let _ = am.update(db).await;
            let _ = ImageRuntime::remove_image(&runtime, &temp_tag, true).await;
            Err(e)
        }
    }
}

/// `CHALLENGES_DIR` 下单个目录的扫描结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChallengeScanItem {
    pub safe_name: String,
    pub name: Option<String>,
    pub version: Option<String>,
    /// "added" | "skipped" | "error"
    pub status: String,
    pub message: String,
}

/// 扫描 `CHALLENGES_DIR/{safe_name}`，登记尚未入库的包。
///
/// 场景：DB 清空/换库后，磁盘目录（mirror 产物）与本地镜像仍在。逐目录解析 meta.toml，
/// 若 safe_name 未入库则登记 identity + package 字段；镜像（image_ref tag）本地存在 → ready，
/// 否则 → failed（build_error 提示需重新 Import）。已存在的跳过。
pub async fn scan_challenges_dir(
    db: &DatabaseConnection,
    docker: &Docker,
    registry: &RegistryConfig,
) -> Result<Vec<ChallengeScanItem>, AppError> {
    use crate::infrastructure::settings::resolve_dir_path;

    let dir_str = get_setting(db, "CHALLENGES_DIR")
        .await
        .map_err(|e| AppError::Internal(format!("get setting CHALLENGES_DIR: {e}")))?;
    let root = resolve_dir_path(&dir_str);
    if !root.is_dir() {
        info!(dir = %root.display(), "CHALLENGES_DIR not found, scan returns empty");
        return Ok(Vec::new());
    }

    let runtime = DockerContainerRuntime::new(docker.clone());
    let mut items = Vec::new();
    let entries = std::fs::read_dir(&root)
        .map_err(|e| AppError::Internal(format!("read CHALLENGES_DIR {}: {e}", root.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| AppError::Internal(format!("read_dir entry: {e}")))?;
        if !entry.path().is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let package_root = entry.path();

        let source_toml = match read_meta_toml(&package_root) {
            Ok(t) => t,
            Err(e) => {
                items.push(ChallengeScanItem {
                    safe_name: dir_name,
                    name: None,
                    version: None,
                    status: "error".into(),
                    message: format!("meta.toml 读取失败: {e}"),
                });
                continue;
            }
        };
        let meta = match fcmc::ChallengeMeta::parse_and_validate(&source_toml) {
            Ok(m) => m,
            Err(e) => {
                items.push(ChallengeScanItem {
                    safe_name: dir_name,
                    name: None,
                    version: None,
                    status: "error".into(),
                    message: map_meta_error(e).to_string(),
                });
                continue;
            }
        };
        let safe_name = match meta.resolved_safe_name() {
            Ok(s) => s,
            Err(e) => {
                items.push(ChallengeScanItem {
                    safe_name: dir_name,
                    name: None,
                    version: None,
                    status: "error".into(),
                    message: map_meta_error(e).to_string(),
                });
                continue;
            }
        };
        let version = meta.version.clone();
        let normalized = match meta.normalize() {
            Ok(n) => n,
            Err(e) => {
                items.push(ChallengeScanItem {
                    safe_name: safe_name.clone(),
                    name: None,
                    version: Some(version),
                    status: "error".into(),
                    message: map_meta_error(e).to_string(),
                });
                continue;
            }
        };

        // 已入库 → 跳过（scan 只补录缺的）
        let existing = Challenges::find()
            .filter(challenges::Column::SafeName.eq(&safe_name))
            .one(db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        if existing.is_some() {
            items.push(ChallengeScanItem {
                safe_name,
                name: Some(normalized.name.clone()),
                version: Some(version),
                status: "skipped".into(),
                message: "已在数据库中".into(),
            });
            continue;
        }

        let package_digest = match compute_package_digest(&package_root, &["src", "attachment"]) {
            Ok(d) => d,
            Err(e) => {
                items.push(ChallengeScanItem {
                    safe_name,
                    name: Some(normalized.name.clone()),
                    version: Some(version),
                    status: "error".into(),
                    message: format!("package_digest 计算失败: {e}"),
                });
                continue;
            }
        };
        let spec_digest = match compute_spec_digest(&normalized) {
            Ok(d) => d,
            Err(e) => {
                items.push(ChallengeScanItem {
                    safe_name,
                    name: Some(normalized.name.clone()),
                    version: Some(version),
                    status: "error".into(),
                    message: format!("spec_digest 计算失败: {e}"),
                });
                continue;
            }
        };
        let spec_json = match serde_json::to_value(&normalized) {
            Ok(v) => v,
            Err(e) => {
                items.push(ChallengeScanItem {
                    safe_name,
                    name: Some(normalized.name.clone()),
                    version: Some(version),
                    status: "error".into(),
                    message: format!("spec_json 序列化失败: {e}"),
                });
                continue;
            }
        };
        let flag_type = normalized.flag_type.clone();
        let static_flag_value = meta.static_flag_value().map(str::to_string);

        let (attachment_path, attachment_name, attachment_size, attachment_sha) = match meta
            .attachment
        {
            Some(ref rel) => match read_package_file(&package_root, rel, MAX_ATTACHMENT_BYTES) {
                Ok(bytes) => {
                    let name = Path::new(rel)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(rel)
                        .to_string();
                    (
                        Some(rel.clone()),
                        Some(name),
                        Some(bytes.len() as i64),
                        Some(sha256_hex(&bytes)),
                    )
                }
                Err(e) => {
                    items.push(ChallengeScanItem {
                        safe_name,
                        name: Some(normalized.name.clone()),
                        version: Some(version),
                        status: "error".into(),
                        message: format!("附件读取失败: {e}"),
                    });
                    continue;
                }
            },
            None => (None, None, None, None),
        };

        // 镜像存在性：image_ref tag 本地可 inspect → ready；否则 failed（提示重新 Import）
        let image_ref = build_artifact_image_ref(
            ArtifactKind::Challenge,
            &registry.image_prefix,
            &safe_name,
            &version,
        );
        let (image_id, build_status, build_error) =
            match ImageRuntime::inspect_image(&runtime, &image_ref).await {
                Ok(insp) => (
                    if insp.image_id.is_empty() {
                        None
                    } else {
                        Some(insp.image_id)
                    },
                    BUILD_STATUS_READY,
                    None,
                ),
                Err(_) => (
                    None,
                    BUILD_STATUS_FAILED,
                    Some("镜像不存在本地，请用 Import 重新构建".to_string()),
                ),
            };

        let resources = &normalized.recommended_resources;
        let model = challenges::ActiveModel {
            name: Set(normalized.name.clone()),
            safe_name: Set(safe_name.clone()),
            category: Set(normalized.category.clone()),
            description: Set(normalized.description.clone()),
            hidden: Set(false),
            version: Set(Some(version.clone())),
            source_toml: Set(Some(source_toml.clone())),
            spec_json: Set(Some(spec_json)),
            spec_digest: Set(Some(spec_digest)),
            package_digest: Set(Some(package_digest)),
            flag_type: Set(Some(flag_type)),
            static_flag_value: Set(static_flag_value),
            container_port: Set(normalized.container_port.map(|p| p as i32)),
            recommended_cpu_millis: Set(resources.cpu_millis),
            recommended_memory_bytes: Set(resources.memory_bytes),
            recommended_pids_limit: Set(resources.pids_limit),
            attachment_path: Set(attachment_path),
            attachment_name: Set(attachment_name),
            attachment_size: Set(attachment_size),
            attachment_sha256: Set(attachment_sha),
            image_ref: Set(Some(image_ref.clone())),
            image_id: Set(image_id.clone()),
            image_repo_digest: Set(None),
            build_status: Set(Some(build_status.to_string())),
            build_error: Set(build_error),
            ..Default::default()
        };
        let c = match model.insert(db).await {
            Ok(m) => m,
            Err(e) => {
                items.push(ChallengeScanItem {
                    safe_name,
                    name: Some(normalized.name),
                    version: Some(version),
                    status: "error".into(),
                    message: format!("写入数据库失败: {e}"),
                });
                continue;
            }
        };

        info!(
            challenge_id = %c.id,
            safe_name = %safe_name,
            version = %version,
            build_status = %build_status,
            "Challenge registered from CHALLENGES_DIR scan"
        );
        items.push(ChallengeScanItem {
            safe_name,
            name: Some(c.name),
            version: c.version.clone(),
            status: "added".into(),
            message: format!("build_status={build_status}"),
        });
    }
    Ok(items)
}

async fn run_build_and_pin(
    runtime: &DockerContainerRuntime,
    registry: &RegistryConfig,
    build_req: &ImageBuildRequest,
    canonical_ref: &str,
    temp_tag: &str,
) -> Result<(String, Option<String>), ImageError> {
    let built = ImageRuntime::build_image(runtime, build_req.clone()).await?;

    // 用构建得到的 image id 打上规范 ref tag（回退：临时 tag 名）。
    if let Err(e) = ImageRuntime::tag_image(runtime, &built.image_id, canonical_ref).await {
        ImageRuntime::tag_image(runtime, temp_tag, canonical_ref)
            .await
            .map_err(|e2| {
                let _ = e;
                e2
            })?;
    }

    let inspected = ImageRuntime::inspect_image(runtime, canonical_ref).await?;
    let image_id = if inspected.image_id.is_empty() {
        built.image_id
    } else {
        inspected.image_id
    };

    if registry.push {
        let auth = registry_auth(registry);
        let digest = ImageRuntime::push_image(runtime, canonical_ref, auth.as_ref()).await?;
        if digest.is_empty() {
            return Err(ImageError::DigestUnavailable(
                "push succeeded but RepoDigest empty".into(),
            ));
        }
        Ok((image_id, Some(digest)))
    } else {
        // LocalOnly: image_repo_digest stays NULL; runtime pins image_id.
        Ok((image_id, None))
    }
}

fn registry_auth(registry: &RegistryConfig) -> Option<RegistryAuth> {
    if registry.username.is_none()
        && registry.password.is_none()
        && registry.server_address.is_none()
    {
        return None;
    }
    Some(RegistryAuth {
        username: registry.username.clone(),
        // Never log password; only pass through to bollard.
        password: registry.password.as_ref().map(|s| s.expose().to_string()),
        server_address: registry.server_address.clone(),
    })
}

/// 将已导入包（`src/`、`attachment/` 及根目录文件）复制到
/// `CHALLENGES_DIR/{safe_name}`，以便既有静态文件服务（附件链接）
/// 以保持可用。尽力而为：失败只记日志，不致命。
async fn mirror_to_challenges_dir(db: &DatabaseConnection, safe_name: &str, package_root: &Path) {
    let challenges_dir = match get_setting(db, "CHALLENGES_DIR").await {
        Ok(d) => d,
        Err(e) => {
            error!(error = %e, "mirror: cannot resolve CHALLENGES_DIR");
            return;
        }
    };
    let dest = crate::infrastructure::settings::resolve_dir_path(&challenges_dir).join(safe_name);
    let res = (|| -> std::io::Result<()> {
        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        copy_dir_all(package_root, &dest)
    })();
    if let Err(e) = res {
        error!(safe_name = %safe_name, error = %e, "mirror package to CHALLENGES_DIR failed");
    } else {
        info!(safe_name = %safe_name, dest = %dest.display(), "package mirrored to CHALLENGES_DIR");
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn map_package_error(e: package::PackageError) -> AppError {
    match e {
        package::PackageError::Validation(m) => AppError::Validation(m),
        package::PackageError::Internal(m) => AppError::Internal(m),
    }
}

fn map_meta_error(e: ChallengeMetaError) -> AppError {
    use fcmc::ChallengeMetaError::*;
    match &e {
        UnknownField(_) => AppError::Validation(format!("CHALLENGE_MANIFEST_UNKNOWN_FIELD: {e}")),
        Parse(_) => AppError::Validation(format!("CHALLENGE_INVALID_MANIFEST: {e}")),
        EmptyName | EmptyAuthor | EmptyCategory => {
            AppError::Validation(format!("CHALLENGE_INVALID_MANIFEST: {e}"))
        }
        InvalidVersion { .. } | VersionBuildMetadata(_) => {
            AppError::Validation(format!("CHALLENGE_INVALID_VERSION: {e}"))
        }
        InvalidSafeName(_) => AppError::Validation(format!("CHALLENGE_INVALID_SAFE_NAME: {e}")),
        SafeNameRequired => AppError::Validation(format!("CHALLENGE_SAFE_NAME_REQUIRED: {e}")),
        InvalidFlagConfig(_) | StaticFlagRequired => {
            AppError::Validation(format!("CHALLENGE_INVALID_FLAG_CONFIG: {e}"))
        }
        InvalidPort(_) => AppError::Validation(format!("CHALLENGE_INVALID_PORT: {e}")),
        InvalidResource(_) => AppError::Validation(format!("CHALLENGE_INVALID_RESOURCES: {e}")),
        InvalidAttachmentPath(_, _) => {
            AppError::Validation(format!("CHALLENGE_INVALID_ATTACHMENT_PATH: {e}"))
        }
    }
}

fn map_image_error(e: ImageError) -> AppError {
    match e {
        ImageError::BuildTimeout => {
            AppError::Internal("CHALLENGE_BUILD_TIMEOUT: image build timed out".into())
        }
        ImageError::BuildFailed(m) => AppError::Internal(format!(
            "CHALLENGE_BUILD_FAILED: {}",
            sanitize_build_error(&m)
        )),
        ImageError::PushFailed(m) => AppError::Internal(format!(
            "CHALLENGE_REGISTRY_PUSH_FAILED: {}",
            sanitize_build_error(&m)
        )),
        ImageError::DigestUnavailable(m) => {
            AppError::Internal(format!("CHALLENGE_REGISTRY_DIGEST_UNAVAILABLE: {m}"))
        }
        ImageError::RegistryAuthFailed(m) => AppError::Internal(format!(
            "CHALLENGE_REGISTRY_PUSH_FAILED: auth: {}",
            sanitize_build_error(&m)
        )),
        other => AppError::Internal(format!(
            "CHALLENGE_BUILD_FAILED: {}",
            sanitize_build_error(&other.to_string())
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_meta_unknown_field_code() {
        let err = map_meta_error(fcmc::ChallengeMetaError::UnknownField("image_tag".into()));
        assert!(err.to_string().contains("CHALLENGE_MANIFEST_UNKNOWN_FIELD"));
    }

    #[test]
    fn map_meta_static_flag_required_code() {
        let err = map_meta_error(fcmc::ChallengeMetaError::StaticFlagRequired);
        assert!(err.to_string().contains("CHALLENGE_INVALID_FLAG_CONFIG"));
    }

    #[test]
    fn image_ref_shared_with_gamebox() {
        assert_eq!(
            fcmc::build_artifact_image_ref(
                fcmc::ArtifactKind::Challenge,
                "registry.example.com",
                "easy-web",
                "1.0.0"
            ),
            "registry.example.com/challenges/easy-web:1.0.0"
        );
        assert_eq!(
            fcmc::build_artifact_image_ref(
                fcmc::ArtifactKind::GameBox,
                "registry.example.com",
                "easy-awd-web",
                "2.1.0"
            ),
            "registry.example.com/gameboxes/easy-awd-web:2.1.0"
        );
    }

    #[test]
    fn manifest_roundtrip_v1() {
        let toml = r#"
name = "Easy Web 01"
version = "1.0.0"
author = "a@b.c"
category = "web"
description = "hello"

[flag]
type = "dynamic"

[docker]
port = 80

[docker.recommended_resources]
cpu_millis = 500
memory_bytes = 268435456
pids_limit = 100
"#;
        let meta = fcmc::ChallengeMeta::parse_and_validate(toml).unwrap();
        assert_eq!(meta.resolved_safe_name().unwrap(), "easy-web-01");
        assert!(meta.static_flag_value().is_none());
        let normalized = meta.normalize().unwrap();
        assert_eq!(normalized.flag_type, "dynamic");
        assert_eq!(normalized.container_port, Some(80));
    }

    #[test]
    fn static_manifest_exposes_secret_but_normalized_spec_does_not() {
        let toml = r#"
name = "Static"
version = "1.0.0"
author = "a@b.c"
category = "misc"
description = "d"

[flag]
type = "static"
value = "flag{supersecret}"
"#;
        let meta = fcmc::ChallengeMeta::parse_and_validate(toml).unwrap();
        assert_eq!(meta.static_flag_value(), Some("flag{supersecret}"));
        let normalized = meta.normalize().unwrap();
        let spec_json = serde_json::to_string(&normalized).unwrap();
        assert!(
            !spec_json.contains("supersecret"),
            "NormalizedChallengeSpec must not contain the static flag value"
        );
    }
}
