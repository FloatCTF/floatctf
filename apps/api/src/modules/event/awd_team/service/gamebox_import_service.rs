//! GameBox package import pipeline.
//!
//! Flow:
//! 1. Safe extract zip → discover package root → require meta.toml + src/Dockerfile
//! 2. Parse/validate meta.toml via fcmc::GameBoxMeta
//! 3. Compute package_digest + spec_digest
//! 4. Transaction1: upsert identity by safe_name; insert/retry revision as `building`
//! 5. Outside txn: docker build (context = package/src only); optional registry push
//! 6. Transaction2: mark ready (image pins) or failed (sanitized error)
//!
//! v1: synchronous build is OK (no durable job system for docker builds).

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use bollard::Docker;
use fcmc::{
    DockerContainerRuntime, ImageBuildRequest, ImageError, ImageRuntime, RegistryAuth,
    build_gamebox_image_ref,
};
use sea_orm::{DatabaseConnection, TransactionTrait};
use tracing::{error, info};
use uuid::Uuid;

use crate::core::config::RegistryConfig;
use crate::entity::{gamebox_revisions, gameboxes};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    repo::{gamebox_lib_repo, gamebox_revision_repo},
    service::gamebox_package::{
        compute_package_digest, compute_spec_digest, discover_package_root, extract_package_zip,
        read_judge_script, read_meta_toml, require_package_layout, sanitize_build_error,
    },
};

/// Result of import (returned to admin API).
#[derive(Debug, Clone)]
pub struct ImportGameBoxResult {
    pub gamebox: gameboxes::Model,
    pub revision: gamebox_revisions::Model,
    /// True when an identical ready revision already existed (build skipped).
    pub already_exists: bool,
}

/// Import a GameBox package zip (multipart tempfile path).
///
/// Uses platform `RegistryConfig` for image prefix / push mode / credentials.
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

    // ── 2. Transaction1: identity + revision building ──────────────────────
    // Decision outcome carried out of the txn.
    enum Tx1 {
        AlreadyReady {
            gamebox: gameboxes::Model,
            revision: gamebox_revisions::Model,
        },
        Build {
            gamebox: gameboxes::Model,
            revision_id: Uuid,
        },
    }

    let tx1 = {
        let txn = db
            .begin()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;

        let gamebox = match gamebox_lib_repo::find_gamebox_by_safe_name(&txn, &safe_name)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?
        {
            Some(existing) => {
                // Do NOT silently rewrite name/category/description on import.
                existing
            }
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

        let existing_rev =
            gamebox_revision_repo::find_by_gamebox_and_version(&txn, gamebox.id, &version)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;

        let outcome = match existing_rev {
            Some(rev) if rev.build_status == gamebox_revision_repo::BUILD_STATUS_READY => {
                if rev.package_digest == package_digest {
                    Tx1::AlreadyReady {
                        gamebox: gamebox.clone(),
                        revision: rev,
                    }
                } else {
                    return Err(AwdError::Conflict(format!(
                        "VERSION_CONFLICT: version {version} already ready with different package_digest"
                    )));
                }
            }
            Some(rev) if rev.build_status == gamebox_revision_repo::BUILD_STATUS_BUILDING => {
                return Err(AwdError::Conflict(
                    "build in progress for this version".into(),
                ));
            }
            Some(rev) if rev.build_status == gamebox_revision_repo::BUILD_STATUS_FAILED => {
                if rev.package_digest != package_digest {
                    return Err(AwdError::Conflict(format!(
                        "VERSION_CONFLICT: failed version {version} has different package_digest; bump version"
                    )));
                }
                // Retry same package_digest.
                let updated = gamebox_revision_repo::reset_to_building(
                    &txn,
                    rev.id,
                    source_toml.clone(),
                    spec_json.clone(),
                    spec_digest.clone(),
                    package_digest.clone(),
                    Some(image_ref.clone()),
                    normalized.username.clone(),
                    resources.cpu_millis,
                    resources.memory_bytes,
                    resources.pids_limit,
                    healthchecks_json.clone(),
                    judge_script_name.clone(),
                    judge_script_content.clone(),
                )
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
                Tx1::Build {
                    gamebox: gamebox.clone(),
                    revision_id: updated.id,
                }
            }
            Some(_) => {
                return Err(AwdError::Conflict(format!(
                    "unknown build_status for version {version}"
                )));
            }
            None => {
                let rev_no = gamebox_revision_repo::next_revision_number(&txn, gamebox.id)
                    .await
                    .map_err(|e| AwdError::Database(e.to_string()))?;
                let inserted = gamebox_revision_repo::insert_building(
                    &txn,
                    gamebox_revision_repo::NewRevision {
                        gamebox_id: gamebox.id,
                        version: version.clone(),
                        revision_number: rev_no,
                        source_toml: source_toml.clone(),
                        spec_json: spec_json.clone(),
                        spec_digest: spec_digest.clone(),
                        package_digest: package_digest.clone(),
                        image_ref: Some(image_ref.clone()),
                        username: normalized.username.clone(),
                        recommended_cpu_millis: resources.cpu_millis,
                        recommended_memory_bytes: resources.memory_bytes,
                        recommended_pids_limit: resources.pids_limit,
                        healthchecks_json: healthchecks_json.clone(),
                        judge_script_name: judge_script_name.clone(),
                        judge_script_content: judge_script_content.clone(),
                        judge_args_json: None,
                        judge_timeout_secs: None,
                        judge_retry_interval_secs: None,
                    },
                )
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
                Tx1::Build {
                    gamebox: gamebox.clone(),
                    revision_id: inserted.id,
                }
            }
        };

        txn.commit()
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        outcome
    };

    let (gamebox, revision_id) = match tx1 {
        Tx1::AlreadyReady { gamebox, revision } => {
            return Ok(ImportGameBoxResult {
                gamebox,
                revision,
                already_exists: true,
            });
        }
        Tx1::Build {
            gamebox,
            revision_id,
        } => (gamebox, revision_id),
    };

    // ── 3. Build outside txn (synchronous v1) ──────────────────────────────
    let context_dir = package_root.join("src");
    let short_id = &revision_id.to_string().replace('-', "")[..8];
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
    };

    let build_outcome =
        run_build_and_pin(&runtime, registry, &build_req, &image_ref, &temp_tag).await;

    // ── 4. Transaction2: ready or failed ───────────────────────────────────
    match build_outcome {
        Ok((image_id, image_repo_digest)) => {
            let revision = gamebox_revision_repo::mark_ready(
                db,
                revision_id,
                image_ref.clone(),
                image_id,
                image_repo_digest,
            )
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
            info!(
                gamebox_id = %gamebox.id,
                revision_id = %revision.id,
                version = %version,
                "GameBox package import ready"
            );
            // Best-effort cleanup of temp tag (canonical image_ref still tagged).
            let _ = ImageRuntime::remove_image(&runtime, &temp_tag, true).await;
            Ok(ImportGameBoxResult {
                gamebox,
                revision,
                already_exists: false,
            })
        }
        Err(e) => {
            let sanitized = sanitize_build_error(&e.to_string());
            error!(
                revision_id = %revision_id,
                error = %sanitized,
                "GameBox package import build failed"
            );
            let _ = gamebox_revision_repo::mark_failed(db, revision_id, sanitized.clone()).await;
            let _ = ImageRuntime::remove_image(&runtime, &temp_tag, true).await;
            Err(e)
        }
    }
}

async fn run_build_and_pin(
    runtime: &DockerContainerRuntime,
    registry: &RegistryConfig,
    build_req: &ImageBuildRequest,
    canonical_ref: &str,
    temp_tag: &str,
) -> AwdResult<(String, Option<String>)> {
    // Prefer ImageRuntime trait methods (DockerContainerRuntime also has a legacy
    // inherent build_image(&str, &Path) used by challenge build).
    let built = ImageRuntime::build_image(runtime, build_req.clone())
        .await
        .map_err(map_image_error)?;

    // Tag canonical ref from the built image id (fallback: temp tag name).
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
