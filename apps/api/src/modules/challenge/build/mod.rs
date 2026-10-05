//! 题目镜像/包构建相关能力。

pub mod import_service;

use crate::api::prelude::*;
use crate::entity::{challenges, prelude::Challenges};
use crate::modules::challenge::build::import_service::ChallengeScanItem;
use crate::modules::challenge::catalog::ChallengesDto;
use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use fcmc::{DockerContainerRuntime, ImageRuntime, content_image_ref};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

#[derive(Debug, MultipartForm)]
struct UploadForm {
    /// 单题 package zip：meta.toml + src/** + attachment/**
    #[multipart(limit = "10240MB")]
    package_zip: TempFile,
}

/// POST /api/admin/challenges/import —— package zip 导入（单版本：同步 build + pin digest）
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
        challenge: ChallengesDto::from(&result.challenge),
    };

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGES",
            "IMPORT",
            format!("{} 导入题目包: {}", user.username, response.challenge.name).as_str(),
            json!({
                "challenge_id": response.challenge.id,
                "version": response.challenge.version,
                "build_status": response.challenge.build_status,
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
}

/// POST /api/admin/challenges/scan —— 扫描 CHALLENGES_DIR 登记未入库 package
#[post("/scan")]
pub async fn scan_challenges(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
) -> UniResult<Vec<ChallengeScanItem>> {
    let items = import_service::scan_challenges_dir(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        &ctx.config.registry,
    )
    .await?;
    UniResponse::ok(items.into()).into()
}

/// POST /api/admin/challenges/check
#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeCheckResult {
    pub id: Uuid,
    pub challenge_name: String,
    pub is_ok: bool,
    /// 镜像可用。static / attachment-only 题目没有镜像，恒为 true（没有东西需要构建）。
    pub docker_image: bool,
    pub attachment: bool,
    /// static / attachment-only 内容（无 `src/Dockerfile`）：前端据此不显示 Build 按钮。
    pub static_content: bool,
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
        // static / attachment-only（container_port 为空）没有镜像，也没有 Dockerfile：
        // 它不需要构建，只有附件需要存在。绝不能被判成“镜像缺失 → Build”。
        let static_content = challenge.container_port.is_none();
        let attach_ok = match &challenge.attachment_path {
            Some(rel) => {
                let p = crate::infrastructure::settings::resolve_dir_path(&challenge_dir)
                    .join(&challenge.safe_name)
                    .join(rel);
                p.is_file()
            }
            None => true,
        };
        let docker_ok = if static_content {
            true
        } else {
            // 期望镜像（规范 tag）：只要镜像已在本地就绪，Check 就应把 build_status
            // 修回 ready——否则管理员只能「删除题目 + 重新 Scan」，实测中正是如此。
            let expected_ref = content_image_ref(
                fcmc::ArtifactKind::Challenge,
                &ctx.config.registry.image_prefix,
                &challenge.safe_name,
                challenge.version.as_deref().unwrap_or_default(),
            );
            let pinned = effective_image_ref(
                challenge.image_repo_digest.as_deref(),
                challenge.image_id.as_deref(),
            )
            .ok();
            let mut inspected: Option<String> = None;
            for candidate in [Some(expected_ref.clone()), pinned].into_iter().flatten() {
                if let Ok(info) = ImageRuntime::inspect_image(&runtime, &candidate).await {
                    inspected = Some(info.image_id);
                    break;
                }
            }
            match inspected {
                Some(image_id) => {
                    if challenge.build_status.as_deref() != Some(import_service::BUILD_STATUS_READY)
                    {
                        let mut am: challenges::ActiveModel = challenge.clone().into();
                        am.build_status = Set(Some(import_service::BUILD_STATUS_READY.to_string()));
                        am.build_error = Set(None);
                        am.image_ref = Set(Some(expected_ref));
                        if !image_id.is_empty() {
                            am.image_id = Set(Some(image_id));
                        }
                        am.updated_at = Set(chrono::Utc::now().into());
                        if let Err(e) = am.update(ctx.db.get_ref()).await {
                            tracing::warn!(
                                challenge = %challenge.safe_name,
                                error = %e,
                                "Check 修复题目构建状态失败"
                            );
                        }
                    }
                    true
                }
                None => false,
            }
        };

        results.push(ChallengeCheckResult {
            id: challenge.id,
            challenge_name: challenge.name,
            is_ok: docker_ok && attach_ok,
            docker_image: docker_ok,
            attachment: attach_ok,
            static_content,
        });
    }

    UniResponse::ok(results.into()).into()
}

/// POST /api/admin/challenges/build
///
/// 单版本模型：仅 re-ensure 当前版本的 pin 镜像本地存在（pull by RepoDigest when missing），
/// 不重新构建。
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

        let (is_ok, message) =
            if challenge.build_status.as_deref() == Some(import_service::BUILD_STATUS_READY) {
                if challenge.container_port.is_some() {
                    match effective_image_ref(
                        challenge.image_repo_digest.as_deref(),
                        challenge.image_id.as_deref(),
                    ) {
                        Ok(pin) => {
                            let runtime = DockerContainerRuntime::new(ctx.docker.get_ref().clone());
                            match ImageRuntime::ensure_image(&runtime, &pin, None).await {
                                Ok(_) => (true, "ok".to_string()),
                                Err(e) => (false, e.to_string()),
                            }
                        }
                        Err(e) => (false, e),
                    }
                } else {
                    (true, "no docker runtime (static/misc)".to_string())
                }
            } else {
                (
                    false,
                    "no ready package; import a package first".to_string(),
                )
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

/// 镜像钉扎：`image_repo_digest`（repo@sha256:…）优先于 `image_id`（仅本地 sha256:…）。
pub fn effective_image_ref(
    repo_digest: Option<&str>,
    image_id: Option<&str>,
) -> Result<String, String> {
    if let Some(d) = repo_digest.filter(|d| !d.is_empty()) {
        return Ok(d.to_string());
    }
    if let Some(id) = image_id.filter(|id| !id.is_empty()) {
        return Ok(id.to_string());
    }
    Err("no image pin (image_repo_digest/image_id)".to_string())
}

// re-export for tests / other modules
pub use crate::modules::challenge::build::import_service::ImportChallengeResult;
