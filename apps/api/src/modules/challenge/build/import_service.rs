//! Challenge package import pipeline.
//!
//! Flow (mirrors GameBox import):
//! 1. Safe extract zip → discover package root → require meta.toml + src/Dockerfile
//! 2. Parse/validate meta.toml via `fcmc::ChallengeMeta`
//! 3. Compute package_digest (meta.toml + src/** + attachment/**) + spec_digest
//! 4. Attachment: read + hash metadata (file copied to CHALLENGES_DIR after success)
//! 5. Transaction1: upsert identity by safe_name; insert/retry revision as `building`
//! 6. Outside txn: docker build (context = package/src ONLY); optional registry push
//! 7. Transaction2: mark ready (image pins) or failed (sanitized error)
//! 8. On success: mirror package (src/ + attachment/) into CHALLENGES_DIR/{safe_name}

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
use tracing::{error, info};
use uuid::Uuid;

use crate::api::AppError;
use crate::core::config::RegistryConfig;
use crate::entity::{challenge_revisions, challenges};
use crate::infrastructure::package::{
    self, compute_package_digest, compute_spec_digest, discover_package_root, extract_package_zip,
    read_meta_toml, read_package_file, require_package_layout, sanitize_build_error, sha256_hex,
};
use crate::infrastructure::settings::get_setting;

use super::revision_repo;

/// Max attachment size (bounded; matches package single-file limit headroom).
const MAX_ATTACHMENT_BYTES: u64 = 64 * 1024 * 1024;

/// Result of import (returned to admin API).
#[derive(Debug, Clone)]
pub struct ImportChallengeResult {
    pub challenge: challenges::Model,
    pub revision: challenge_revisions::Model,
    /// True when an identical ready revision already existed (build skipped).
    pub already_exists: bool,
}

/// Import a Challenge package zip (multipart tempfile path).
///
/// Uses platform `RegistryConfig` for image prefix / push mode / credentials.
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

    // ── 2. Transaction1: identity + revision building ──────────────────────
    enum Tx1 {
        AlreadyReady {
            challenge: challenges::Model,
            revision: challenge_revisions::Model,
        },
        Build {
            challenge: challenges::Model,
            revision_id: Uuid,
        },
    }

    let tx1 = {
        let txn = db
            .begin()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let challenge = match challenges::Entity::find()
            .filter(crate::entity::challenges::Column::SafeName.eq(&safe_name))
            .one(&txn)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?
        {
            Some(existing) => {
                // Do NOT silently rewrite name/category/description on import.
                existing
            }
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

        let existing_rev =
            revision_repo::find_by_challenge_and_version(&txn, challenge.id, &version)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;

        let outcome = match existing_rev {
            Some(rev) if rev.build_status == revision_repo::BUILD_STATUS_READY => {
                if rev.package_digest == package_digest {
                    Tx1::AlreadyReady {
                        challenge: challenge.clone(),
                        revision: rev,
                    }
                } else {
                    return Err(AppError::Conflict(format!(
                        "CHALLENGE_VERSION_CONFLICT: version {version} already ready with different package_digest"
                    )));
                }
            }
            Some(rev) if rev.build_status == revision_repo::BUILD_STATUS_BUILDING => {
                return Err(AppError::Conflict(
                    "build in progress for this version".into(),
                ));
            }
            Some(rev) if rev.build_status == revision_repo::BUILD_STATUS_FAILED => {
                if rev.package_digest != package_digest {
                    return Err(AppError::Conflict(format!(
                        "CHALLENGE_VERSION_CONFLICT: failed version {version} has different package_digest; bump version"
                    )));
                }
                // Retry same package_digest.
                let updated = revision_repo::reset_to_building(
                    &txn,
                    rev.id,
                    source_toml.clone(),
                    spec_json.clone(),
                    spec_digest.clone(),
                    package_digest.clone(),
                    flag_type.clone(),
                    static_flag_value.clone(),
                    normalized.container_port.map(|p| p as i32),
                    resources.cpu_millis,
                    resources.memory_bytes,
                    resources.pids_limit,
                    attachment_path.clone(),
                    attachment_name.clone(),
                    attachment_size,
                    attachment_sha.clone(),
                    Some(image_ref.clone()),
                )
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;
                Tx1::Build {
                    challenge: challenge.clone(),
                    revision_id: updated.id,
                }
            }
            Some(_) => {
                return Err(AppError::Conflict(format!(
                    "unknown build_status for version {version}"
                )));
            }
            None => {
                let rev_no = revision_repo::next_revision_number(&txn, challenge.id)
                    .await
                    .map_err(|e| AppError::Database(e.to_string()))?;
                let inserted = revision_repo::insert_building(
                    &txn,
                    revision_repo::NewRevision {
                        challenge_id: challenge.id,
                        version: version.clone(),
                        revision_number: rev_no,
                        source_toml: source_toml.clone(),
                        spec_json: spec_json.clone(),
                        spec_digest: spec_digest.clone(),
                        package_digest: package_digest.clone(),
                        flag_type: flag_type.clone(),
                        static_flag_value: static_flag_value.clone(),
                        container_port: normalized.container_port.map(|p| p as i32),
                        recommended_cpu_millis: resources.cpu_millis,
                        recommended_memory_bytes: resources.memory_bytes,
                        recommended_pids_limit: resources.pids_limit,
                        attachment_path: attachment_path.clone(),
                        attachment_name: attachment_name.clone(),
                        attachment_size,
                        attachment_sha256: attachment_sha.clone(),
                        image_ref: Some(image_ref.clone()),
                    },
                )
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;
                Tx1::Build {
                    challenge: challenge.clone(),
                    revision_id: inserted.id,
                }
            }
        };

        txn.commit()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        outcome
    };

    let (challenge, revision_id) = match tx1 {
        Tx1::AlreadyReady {
            challenge,
            revision,
        } => {
            return Ok(ImportChallengeResult {
                challenge,
                revision,
                already_exists: true,
            });
        }
        Tx1::Build {
            challenge,
            revision_id,
        } => (challenge, revision_id),
    };

    // ── 3. Build outside txn (synchronous v1) ──────────────────────────────
    let context_dir = package_root.join("src");
    let short_id = &revision_id.to_string().replace('-', "")[..8];
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
    };

    let build_outcome = run_build_and_pin(&runtime, registry, &build_req, &image_ref, &temp_tag)
        .await
        .map_err(map_image_error);

    // ── 4. Transaction2: ready or failed ───────────────────────────────────
    match build_outcome {
        Ok((image_id, image_repo_digest)) => {
            let revision = revision_repo::mark_ready(
                db,
                revision_id,
                image_ref.clone(),
                image_id,
                image_repo_digest,
            )
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

            // Mirror package (src/ + attachment/) into CHALLENGES_DIR for static serving.
            mirror_to_challenges_dir(db, &safe_name, &package_root).await;

            info!(
                challenge_id = %challenge.id,
                revision_id = %revision.id,
                version = %version,
                "Challenge package import ready"
            );
            // Best-effort cleanup of temp tag (canonical image_ref still tagged).
            let _ = ImageRuntime::remove_image(&runtime, &temp_tag, true).await;
            Ok(ImportChallengeResult {
                challenge,
                revision,
                already_exists: false,
            })
        }
        Err(e) => {
            let sanitized = sanitize_build_error(&e.to_string());
            error!(
                revision_id = %revision_id,
                error = %sanitized,
                "Challenge package import build failed"
            );
            let _ = revision_repo::mark_failed(db, revision_id, sanitized.clone()).await;
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
) -> Result<(String, Option<String>), ImageError> {
    let built = ImageRuntime::build_image(runtime, build_req.clone()).await?;

    // Tag canonical ref from the built image id (fallback: temp tag name).
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

/// Copy the imported package (`src/`, `attachment/`, and any files at root) into
/// `CHALLENGES_DIR/{safe_name}` so existing static-file serving (attachment links)
/// keeps working. Best-effort: failure is logged, not fatal.
async fn mirror_to_challenges_dir(db: &DatabaseConnection, safe_name: &str, package_root: &Path) {
    let challenges_dir = match get_setting(db, "CHALLENGES_DIR").await {
        Ok(d) => d,
        Err(e) => {
            error!(error = %e, "mirror: cannot resolve CHALLENGES_DIR");
            return;
        }
    };
    let dest = Path::new(&challenges_dir).join(safe_name);
    let res = (|| -> std::io::Result<()> {
        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        copy_dir_all(package_root, &dest)
    })();
    if let Err(e) = res {
        error!(safe_name = %safe_name, error = %e, "mirror package to CHALLENGES_DIR failed");
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
