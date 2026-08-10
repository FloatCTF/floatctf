//! Admin challenge import / check / docker image ensure.

pub mod import_service;
pub mod revision_repo;

use crate::api::prelude::*;
use crate::entity::{challenges, prelude::Challenges};
use crate::modules::challenge::catalog::{ChallengeRevisionDto, ChallengesDto};
use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use fcmc::{DockerContainerRuntime, ImageRuntime};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

#[derive(Debug, MultipartForm)]
struct UploadForm {
    /// 单题 package zip：meta.toml + src/** + attachment/**
    #[multipart(limit = "10240MB")]
    package_zip: TempFile,
}

/// POST /api/admin/challenges/import —— package zip 导入（同步 build + pin digest）
#[post("/import")]
pub async fn web_import_challenge(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    MultipartForm(form): MultipartForm<UploadForm>,
) -> UniResult<ImportChallengeResponse> {
    let user = user.into_inner();

    let result = import_service::import_challenge_package(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        &ctx.config.registry,
        form.package_zip.file.path(),
    )
    .await?;

    let response = ImportChallengeResponse {
        challenge: ChallengesDto::from_model(ctx.db.get_ref(), &result.challenge)
            .await
            .map_err(AppError::from)?,
        revision: result.revision.into(),
        already_exists: result.already_exists,
    };

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGES",
            "IMPORT",
            format!("{} 导入题目包: {}", user.username, response.challenge.name).as_str(),
            json!({
                "challenge_id": response.challenge.id,
                "version": response.revision.version,
                "build_status": response.revision.build_status,
                "already_exists": response.already_exists,
            }),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(response)).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportChallengeResponse {
    pub challenge: ChallengesDto,
    pub revision: ChallengeRevisionDto,
    /// True when an identical ready revision already existed (build skipped).
    pub already_exists: bool,
}

/// POST /api/admin/challenges/check
#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeCheckResult {
    pub id: Uuid,
    pub challenge_name: String,
    pub is_ok: bool,
    pub docker_image: bool,
    pub attachment: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeCheckRequest {
    pub challenge_id_list: Option<Vec<Uuid>>,
}

#[post("/check")]
pub async fn check_challenges(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    ccr: Json<ChallengeCheckRequest>,
) -> UniResult<Vec<ChallengeCheckResult>> {
    let ccr = ccr.into_inner();
    let challenge_dir = get_setting(&ctx.db, "CHALLENGES_DIR")
        .await
        .map_err(|e| AppError::BadRequest(format!("get setting error: {}", e)))?;

    let challenges = {
        if let Some(ids) = ccr.challenge_id_list {
            Challenges::find()
                .filter(challenges::Column::Id.is_in(ids))
                .all(ctx.db.get_ref())
                .await?
        } else {
            Challenges::find().all(ctx.db.get_ref()).await?
        }
    };

    let runtime = DockerContainerRuntime::new(ctx.docker.get_ref().clone());
    let mut results = Vec::new();
    for challenge in challenges {
        let latest = revision_repo::find_latest_ready(ctx.db.get_ref(), challenge.id)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let (docker_image_ok, attachment_ok) = match latest {
            Some(rev) => {
                // 镜像检查：latest ready revision 的 pin（RepoDigest/image_id）必须本地可 inspect
                let docker_ok = match revision_repo::effective_image_ref(&rev) {
                    Ok(pin) => ImageRuntime::inspect_image(&runtime, &pin).await.is_ok(),
                    Err(_) => false,
                };
                let attach_ok = match &rev.attachment_path {
                    Some(rel) => {
                        let p = std::path::Path::new(&challenge_dir)
                            .join(&challenge.safe_name)
                            .join(rel);
                        p.is_file()
                    }
                    None => true,
                };
                (docker_ok, attach_ok)
            }
            None => (false, true),
        };

        results.push(ChallengeCheckResult {
            id: challenge.id,
            challenge_name: challenge.name,
            is_ok: docker_image_ok && attachment_ok,
            docker_image: docker_image_ok,
            attachment: attachment_ok,
        });
    }

    UniResponse::ok(results.into()).into()
}

/// POST /api/admin/challenges/build
///
/// Ready revisions are immutable — this endpoint does NOT rebuild them. It
/// re-ensures the pinned image exists locally (pull by RepoDigest when missing).
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildChallengeRequest {
    pub challenge_id: Option<Uuid>,
    pub challenge_id_list: Option<Vec<Uuid>>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildChallengeResult {
    pub challenge_name: String,
    pub is_ok: bool,
    pub message: String,
}

#[post("/build")]
pub async fn build_challenge(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    bcr: Json<BuildChallengeRequest>,
) -> UniResult<Vec<BuildChallengeResult>> {
    let user = user.into_inner();
    let bcr = bcr.into_inner();
    let mut challenge_id_list = Vec::new();
    if let Some(c) = bcr.challenge_id {
        challenge_id_list.push(c);
    }
    if let Some(l) = bcr.challenge_id_list {
        challenge_id_list.extend(l);
    }

    let mut res = Vec::new();
    for challenge_id in challenge_id_list {
        let challenge = Challenges::find_by_id(challenge_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

        let latest = revision_repo::find_latest_ready(ctx.db.get_ref(), challenge.id)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let (is_ok, message) = match latest {
            Some(rev) if rev.container_port.is_some() => {
                match revision_repo::effective_image_ref(&rev) {
                    Ok(pin) => {
                        let runtime = DockerContainerRuntime::new(ctx.docker.get_ref().clone());
                        match ImageRuntime::ensure_image(&runtime, &pin, None).await {
                            Ok(_) => (true, "ok".to_string()),
                            Err(e) => (false, e.to_string()),
                        }
                    }
                    Err(e) => (false, e),
                }
            }
            Some(_) => (true, "no docker runtime (static/misc)".to_string()),
            None => (
                false,
                "no ready revision; import a package first".to_string(),
            ),
        };

        res.push(BuildChallengeResult {
            challenge_name: challenge.name.clone(),
            is_ok,
            message,
        });

        ctx.log
            .add_log(
                "INFO",
                "CHALLENGES",
                "BUILD",
                format!("{} 确保题目镜像: {}", user.username, challenge.name).as_str(),
                json!({"challenge_name": challenge.name, "success": is_ok}),
                None,
                user.id.into(),
                Some(&ctx.req),
            )
            .await;
    }

    UniResponse::ok(res.into()).into()
}

// re-export for tests / other modules
pub use crate::modules::challenge::build::import_service::ImportChallengeResult;
