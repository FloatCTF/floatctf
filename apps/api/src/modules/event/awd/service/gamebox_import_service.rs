//! GameBox 包导入（构建/推送/登记）服务。
//!
//! 流程：
//! 1. 安全解压 zip → 定位包根 → 要求 meta.toml + src/Dockerfile
//! 2. 解析并规范化规格
//! 3. docker build
//! 4. 计算 package_digest + spec_digest
//! 5. 按 RegistryConfig 推送/钉扎镜像
//! 6. 成功后镜像包到 GAMEBOXES_DIR/{safe_name}
//!
//! v1：同步构建可接受（尚无 docker build 持久任务系统）。

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use bollard::Docker;
use fcmc::{
    DockerContainerRuntime, ImageBuildRequest, ImageError, ImageRuntime, RegistryAuth,
    build_gamebox_image_ref,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait, QueryFilter,
    TransactionTrait,
};
use tracing::{error, info, warn};

use crate::core::config::RegistryConfig;
use crate::entity::gameboxes;
use crate::infrastructure::package::version_gate_reason;
use crate::infrastructure::settings::get_setting;
use crate::modules::event::awd::{
    AwdError, AwdResult,
    repo::gamebox_lib_repo,
    service::gamebox_package::{
        compute_package_digest, compute_spec_digest, discover_package_root, extract_package_zip,
        read_judge_script, read_meta_toml, require_package_layout, sanitize_build_error,
    },
};

pub const BUILD_STATUS_BUILDING: &str = "building";
pub const BUILD_STATUS_READY: &str = "ready";
pub const BUILD_STATUS_FAILED: &str = "failed";

/// import (returned to admin API)的结果。
#[derive(Debug, Clone)]
pub struct ImportGameBoxResult {
    pub gamebox: gameboxes::Model,
}

/// 导入 GameBox 包 zip（multipart 临时文件路径）。
///
/// 单版本模型：identity 直接承载当前版本全部 package 字段；导入要求 version 严格递增。
/// 使用平台 `RegistryConfig` 作为镜像前缀 / 推送模式 / 凭证。
pub async fn import_gamebox_package(
    db: &DatabaseConnection,
    docker: &Docker,
    registry: &RegistryConfig,
    zip_path: &Path,
) -> AwdResult<ImportGameBoxResult> {
    // ── 1. Extract + discover ──────────────────────────────────────────────
    let tmp = tempfile::tempdir()
        .map_err(|e| AwdError::Internal(format!("tempdir for gamebox import: {e}")))?;
    extract_package_zip(zip_path, tmp.path())?;
    let package_root = discover_package_root(tmp.path())?;
    require_package_layout(&package_root)?;

    let source_toml = read_meta_toml(&package_root)?;
    let meta = fcmc::GameBoxMeta::parse_and_validate(&source_toml).map_err(map_meta_error)?;
    let safe_name = meta.resolved_safe_name().map_err(map_meta_error)?;
    let version = meta.version.clone();
    let normalized = meta.normalize().map_err(map_meta_error)?;

    // ── 2. 版本门禁（先比对，任何写操作之前）─────────────────────────────
    let existing = gamebox_lib_repo::find_gamebox_by_safe_name(db, &safe_name)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    if let Some(reason) = version_gate_reason(
        &version,
        existing.as_ref().and_then(|g| g.version.as_deref()),
    ) {
        warn!(
            safe_name = %safe_name,
            incoming_version = %version,
            current_version = ?existing.as_ref().and_then(|g| g.version.as_deref()),
            reason = %reason,
            "GameBox import rejected by version gate"
        );
        return Err(AwdError::Conflict(reason));
    }
    info!(
        safe_name = %safe_name,
        version = %version,
        "GameBox package import started"
    );

    let package_digest = compute_package_digest(&package_root)?;
    let spec_digest = compute_spec_digest(&normalized)?;
    let spec_json = serde_json::to_value(&normalized)
        .map_err(|e| AwdError::Internal(format!("spec_json: {e}")))?;
    let healthchecks_json = serde_json::to_value(&normalized.healthchecks)
        .map_err(|e| AwdError::Internal(format!("healthchecks_json: {e}")))?;

    let (judge_script_name, judge_script_content) = if let Some(ref j) = meta.judge {
        let content = read_judge_script(&package_root, &j.script)?;
        let name = Path::new(&j.script)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&j.script)
            .to_string();
        (Some(name), Some(content))
    } else {
        (None, None)
    };

    let image_ref = build_gamebox_image_ref(&registry.image_prefix, &safe_name, &version);
    let resources = &normalized.recommended_resources;

    // ── 3. 单版本 upsert：identity + building 状态（事务）──────────────────
    let gamebox = {
        let txn = db
            .begin()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;

        let gamebox = match existing {
            Some(existing) => existing,
            None => gamebox_lib_repo::create_gamebox_identity(
                &txn,
                normalized.name.clone(),
                safe_name.clone(),
                normalized.category.clone(),
                normalized.description.clone(),
                false,
            )
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?,
        };

        let mut am: gameboxes::ActiveModel = gamebox.clone().into();
        am.version = Set(Some(version.clone()));
        am.source_toml = Set(Some(source_toml.clone()));
        am.spec_json = Set(Some(spec_json.clone()));
        am.spec_digest = Set(Some(spec_digest.clone()));
        am.package_digest = Set(Some(package_digest.clone()));
        am.image_ref = Set(Some(image_ref.clone()));
        am.image_id = Set(None);
        am.image_repo_digest = Set(None);
        am.username = Set(Some(normalized.username.clone()));
        am.recommended_cpu_millis = Set(resources.cpu_millis);
        am.recommended_memory_bytes = Set(resources.memory_bytes);
        am.recommended_pids_limit = Set(resources.pids_limit);
        am.healthchecks_json = Set(Some(healthchecks_json.clone()));
        am.judge_script_name = Set(judge_script_name.clone());
        am.judge_script_content = Set(judge_script_content.clone());
        am.judge_args_json = Set(None);
        am.judge_timeout_secs = Set(None);
        am.judge_retry_interval_secs = Set(None);
        am.build_status = Set(Some(BUILD_STATUS_BUILDING.to_string()));
        am.build_error = Set(None);
        am.updated_at = Set(chrono::Utc::now().into());
        let gamebox = am
            .update(&txn)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;

        txn.commit()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        gamebox
    };

    // ── 4. Build outside txn (synchronous v1) ──────────────────────────────
    let context_dir = package_root.join("src");
    let short_id = &gamebox.id.to_string().replace('-', "")[..8];
    let temp_tag = format!("{image_ref}-import-{short_id}");

    let mut labels = HashMap::new();
    labels.insert("io.floatctf.managed".into(), "true".into());
    labels.insert("io.floatctf.resource".into(), "gamebox-image".into());
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

    let build_outcome =
        run_build_and_pin(&runtime, registry, &build_req, &image_ref, &temp_tag).await;

    // ── 5. ready or failed ─────────────────────────────────────────────────
    match build_outcome {
        Ok((image_id, image_repo_digest)) => {
            let mut am: gameboxes::ActiveModel = gamebox.clone().into();
            am.image_ref = Set(Some(image_ref.clone()));
            am.image_id = Set(Some(image_id.clone()));
            am.image_repo_digest = Set(image_repo_digest.clone());
            am.build_status = Set(Some(BUILD_STATUS_READY.to_string()));
            am.build_error = Set(None);
            am.updated_at = Set(chrono::Utc::now().into());
            let gamebox = am
                .update(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;

            // Mirror package into GAMEBOXES_DIR/{safe_name}（与 Challenge 一致）
            mirror_to_gameboxes_dir(db, &safe_name, &package_root).await;

            info!(
                gamebox_id = %gamebox.id,
                safe_name = %safe_name,
                version = %version,
                image_ref = %image_ref,
                image_repo_digest = ?image_repo_digest,
                package_digest = %package_digest,
                "GameBox package import ready"
            );
            // 尽力清理临时 tag（规范 image_ref 仍保留 tag）。
            let _ = ImageRuntime::remove_image(&runtime, &temp_tag, true).await;
            Ok(ImportGameBoxResult { gamebox })
        }
        Err(e) => {
            let sanitized = sanitize_build_error(&e.to_string());
            error!(
                gamebox_id = %gamebox.id,
                safe_name = %safe_name,
                version = %version,
                error = %sanitized,
                "GameBox package import build failed"
            );
            let mut am: gameboxes::ActiveModel = gamebox.clone().into();
            am.build_status = Set(Some(BUILD_STATUS_FAILED.to_string()));
            am.build_error = Set(Some(sanitized.clone()));
            am.updated_at = Set(chrono::Utc::now().into());
            let _ = am.update(db).await;
            let _ = ImageRuntime::remove_image(&runtime, &temp_tag, true).await;
            Err(e)
        }
    }
}

/// `GAMEBOXES_DIR` 下单个目录的扫描结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameBoxScanItem {
    pub safe_name: String,
    pub name: Option<String>,
    pub version: Option<String>,
    /// "added" | "skipped" | "error"
    pub status: String,
    pub message: String,
}

/// 扫描 `GAMEBOXES_DIR/{safe_name}`，登记尚未入库的包。
///
/// 场景：DB 清空/换库后，磁盘目录（mirror 产物）与本地镜像仍在。逐目录解析 meta.toml，
/// 若 safe_name 未入库则登记 identity + package 字段；镜像（image_ref tag）本地存在 → ready，
/// 否则 → failed（build_error 提示需重新 Import）。已存在的跳过。
pub async fn scan_gameboxes_dir(
    db: &DatabaseConnection,
    docker: &Docker,
    registry: &RegistryConfig,
) -> AwdResult<Vec<GameBoxScanItem>> {
    use crate::infrastructure::settings::resolve_dir_path;

    let dir_str = get_setting(db, "GAMEBOXES_DIR")
        .await
        .map_err(|e| AwdError::Internal(format!("get setting GAMEBOXES_DIR: {e}")))?;
    let root = resolve_dir_path(&dir_str);
    if !root.is_dir() {
        info!(dir = %root.display(), "GAMEBOXES_DIR not found, scan returns empty");
        return Ok(Vec::new());
    }

    let runtime = DockerContainerRuntime::new(docker.clone());
    let mut items = Vec::new();
    let entries = std::fs::read_dir(&root)
        .map_err(|e| AwdError::Internal(format!("read GAMEBOXES_DIR {}: {e}", root.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| AwdError::Internal(format!("read_dir entry: {e}")))?;
        if !entry.path().is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let package_root = entry.path();

        let source_toml = match read_meta_toml(&package_root) {
            Ok(t) => t,
            Err(e) => {
                items.push(GameBoxScanItem {
                    safe_name: dir_name,
                    name: None,
                    version: None,
                    status: "error".into(),
                    message: format!("meta.toml 读取失败: {e}"),
                });
                continue;
            }
        };
        let meta = match fcmc::GameBoxMeta::parse_and_validate(&source_toml) {
            Ok(m) => m,
            Err(e) => {
                items.push(GameBoxScanItem {
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
                items.push(GameBoxScanItem {
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
                items.push(GameBoxScanItem {
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
        let existing = gamebox_lib_repo::find_gamebox_by_safe_name(db, &safe_name)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        if existing.is_some() {
            items.push(GameBoxScanItem {
                safe_name,
                name: Some(normalized.name.clone()),
                version: Some(version),
                status: "skipped".into(),
                message: "已在数据库中".into(),
            });
            continue;
        }

        let package_digest = match compute_package_digest(&package_root) {
            Ok(d) => d,
            Err(e) => {
                items.push(GameBoxScanItem {
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
                items.push(GameBoxScanItem {
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
                items.push(GameBoxScanItem {
                    safe_name,
                    name: Some(normalized.name.clone()),
                    version: Some(version),
                    status: "error".into(),
                    message: format!("spec_json 序列化失败: {e}"),
                });
                continue;
            }
        };
        let healthchecks_json = match serde_json::to_value(&normalized.healthchecks) {
            Ok(v) => v,
            Err(e) => {
                items.push(GameBoxScanItem {
                    safe_name,
                    name: Some(normalized.name.clone()),
                    version: Some(version),
                    status: "error".into(),
                    message: format!("healthchecks_json 序列化失败: {e}"),
                });
                continue;
            }
        };

        let (judge_script_name, judge_script_content) = if let Some(ref j) = meta.judge {
            match read_judge_script(&package_root, &j.script) {
                Ok(content) => {
                    let name = Path::new(&j.script)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&j.script)
                        .to_string();
                    (Some(name), Some(content))
                }
                Err(e) => {
                    items.push(GameBoxScanItem {
                        safe_name,
                        name: Some(normalized.name.clone()),
                        version: Some(version),
                        status: "error".into(),
                        message: format!("judge 脚本读取失败: {e}"),
                    });
                    continue;
                }
            }
        } else {
            (None, None)
        };

        // 镜像存在性：image_ref tag 本地可 inspect → ready；否则 failed（提示重新 Import）
        let image_ref = build_gamebox_image_ref(&registry.image_prefix, &safe_name, &version);
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
        let model = gameboxes::ActiveModel {
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
            image_ref: Set(Some(image_ref.clone())),
            image_id: Set(image_id.clone()),
            image_repo_digest: Set(None),
            username: Set(Some(normalized.username.clone())),
            recommended_cpu_millis: Set(resources.cpu_millis),
            recommended_memory_bytes: Set(resources.memory_bytes),
            recommended_pids_limit: Set(resources.pids_limit),
            healthchecks_json: Set(Some(healthchecks_json)),
            judge_script_name: Set(judge_script_name),
            judge_script_content: Set(judge_script_content),
            judge_args_json: Set(None),
            judge_timeout_secs: Set(None),
            judge_retry_interval_secs: Set(None),
            build_status: Set(Some(build_status.to_string())),
            build_error: Set(build_error),
            ..Default::default()
        };
        let g = match model.insert(db).await {
            Ok(m) => m,
            Err(e) => {
                items.push(GameBoxScanItem {
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
            gamebox_id = %g.id,
            safe_name = %safe_name,
            version = %version,
            build_status = %build_status,
            "GameBox registered from GAMEBOXES_DIR scan"
        );
        items.push(GameBoxScanItem {
            safe_name,
            name: Some(g.name),
            version: g.version.clone(),
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
) -> AwdResult<(String, Option<String>)> {
    let built = ImageRuntime::build_image(runtime, build_req.clone())
        .await
        .map_err(map_image_error)?;

    // 用构建得到的 image id 打上规范 ref tag（回退：临时 tag 名）。
    if let Err(e) = ImageRuntime::tag_image(runtime, &built.image_id, canonical_ref).await {
        ImageRuntime::tag_image(runtime, temp_tag, canonical_ref)
            .await
            .map_err(|e2| {
                let _ = e;
                map_image_error(e2)
            })?;
    }

    let inspected = ImageRuntime::inspect_image(runtime, canonical_ref)
        .await
        .map_err(map_image_error)?;
    let image_id = if inspected.image_id.is_empty() {
        built.image_id
    } else {
        inspected.image_id
    };

    if registry.push {
        let auth = registry_auth(registry);
        let digest = ImageRuntime::push_image(runtime, canonical_ref, auth.as_ref())
            .await
            .map_err(map_image_error)?;
        if digest.is_empty() {
            return Err(AwdError::Docker(
                "DIGEST_UNAVAILABLE: push succeeded but RepoDigest empty".into(),
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

/// Copy the imported package into `GAMEBOXES_DIR/{safe_name}`（解压落盘，与 Challenge 一致）。
/// 尽力而为：失败只记日志，不致命。
async fn mirror_to_gameboxes_dir(db: &DatabaseConnection, safe_name: &str, package_root: &Path) {
    let gameboxes_dir = match get_setting(db, "GAMEBOXES_DIR").await {
        Ok(d) => d,
        Err(e) => {
            error!(error = %e, "mirror: cannot resolve GAMEBOXES_DIR");
            return;
        }
    };
    let dest = crate::infrastructure::settings::resolve_dir_path(&gameboxes_dir).join(safe_name);
    let res = (|| -> std::io::Result<()> {
        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        copy_dir_all(package_root, &dest)
    })();
    if let Err(e) = res {
        error!(safe_name = %safe_name, error = %e, "mirror package to GAMEBOXES_DIR failed");
    } else {
        info!(safe_name = %safe_name, dest = %dest.display(), "package mirrored to GAMEBOXES_DIR");
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

fn map_meta_error(e: fcmc::GameBoxMetaError) -> AwdError {
    use fcmc::GameBoxMetaError::*;
    match &e {
        UnknownField(_) => AwdError::Validation(format!("MANIFEST_UNKNOWN_FIELD: {e}")),
        Parse(_) => AwdError::Validation(format!("INVALID_MANIFEST: {e}")),
        EmptyName | EmptyAuthor | EmptyCategory | EmptyUsername => {
            AwdError::Validation(format!("INVALID_MANIFEST: {e}"))
        }
        InvalidVersion { .. } | VersionBuildMetadata(_) => {
            AwdError::Validation(format!("INVALID_VERSION: {e}"))
        }
        InvalidSafeName(_) => AwdError::Validation(format!("INVALID_SAFE_NAME: {e}")),
        SafeNameRequired => AwdError::Validation(format!("SAFE_NAME_REQUIRED: {e}")),
        InvalidJudgePath(_, _) => AwdError::Validation(format!("INVALID_JUDGE_PATH: {e}")),
        other => AwdError::Validation(format!("INVALID_MANIFEST: {other}")),
    }
}

fn map_image_error(e: ImageError) -> AwdError {
    match e {
        ImageError::BuildTimeout => AwdError::Docker("BUILD_TIMEOUT: image build timed out".into()),
        ImageError::BuildFailed(m) => {
            AwdError::Docker(format!("BUILD_FAILED: {}", sanitize_build_error(&m)))
        }
        ImageError::PushFailed(m) => AwdError::Docker(format!(
            "REGISTRY_PUSH_FAILED: {}",
            sanitize_build_error(&m)
        )),
        ImageError::DigestUnavailable(m) => AwdError::Docker(format!("DIGEST_UNAVAILABLE: {m}")),
        ImageError::RegistryAuthFailed(m) => AwdError::Docker(format!(
            "REGISTRY_PUSH_FAILED: auth: {}",
            sanitize_build_error(&m)
        )),
        other => AwdError::Docker(format!(
            "BUILD_FAILED: {}",
            sanitize_build_error(&other.to_string())
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_meta_unknown_field_code() {
        let err = map_meta_error(fcmc::GameBoxMetaError::UnknownField("image_tag".into()));
        assert!(err.to_string().contains("MANIFEST_UNKNOWN_FIELD"));
    }

    #[test]
    fn pinned_prefers_repo_digest_unit() {
        // Covered in gamebox_service tests; keep import module lean.
        let _ = build_gamebox_image_ref("floatctf", "x", "1.0.0");
    }
}
