//! AWDP 选手侧路由（挂 /api/events/{event_id}/awdp/*）。

use actix_web::web::{self, Json};
use chrono::Utc;
use uuid::Uuid;

use actix_multipart::form::{MultipartForm, tempfile::TempFile};

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::UserJwtGuard, prelude::*},
    entity::{
        events,
        sea_orm_active_enums::{AwdpPhase, ParticipantMode},
    },
    modules::event::awdp::{
        api::dto::*,
        domain::config::AwdpConfig,
        repo::{break_repo, event_gamebox_repo, event_repo, score_repo},
        service::{
            break_service, evaluation, patch_service,
            runtime::{self, Subject},
        },
    },
};

/// 解析 subject（Individual → user；Team → 事件内战队）。
async fn resolve_subject(ctx: &ReqCtx, event_id: Uuid, user_id: Uuid) -> Result<Subject, AppError> {
    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound("event not found".into()))?;
    if event.participant_mode == ParticipantMode::Team {
        let membership =
            crate::modules::event::common::infrastructure::event_repository::find_user_team_membership(
                ctx.db.get_ref(), event_id, user_id,
            )
            .await?
            .ok_or_else(|| AppError::Forbidden("you are not in any team for this event".into()))?;
        Ok(Subject::team(membership.team_id))
    } else {
        Ok(Subject::user(user_id))
    }
}

/// GET /api/events/{event_id}/awdp —— 概览（phase/timing/score/gameboxes）。
#[get("{event_id}/awdp")]
pub async fn get_overview(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<AwdpOverviewDto> {
    let event_id = path.into_inner();
    let user = user.into_inner();
    let db = ctx.db.get_ref();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;

    let event = events::Entity::find_by_id(event_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("event not found".into()))?;
    if event.family != crate::entity::sea_orm_active_enums::EventFamily::Awdp {
        return Err(AppError::Validation("not an AWDP event".into()));
    }

    let awdp = event_repo::ensure_by_event_id(db, event_id, &AwdpConfig::default()).await?;
    let gameboxes = event_gamebox_repo::list_for_event(db, event_id).await?;
    let my_score = score_repo::my_total(db, event_id, subject.user_id, subject.team_id).await?;

    let mut gb_dtos = Vec::new();
    for eg in gameboxes {
        if eg.hidden {
            continue;
        }
        let gamebox = event_gamebox_repo::find_gamebox_identity(db, eg.gamebox_id).await?;
        let exposed = crate::modules::gamebox::healthcheck::parse_healthchecks(
            &eg.healthcheck_override_json.clone().unwrap_or_else(|| {
                gamebox
                    .healthchecks_json
                    .clone()
                    .unwrap_or_else(|| serde_json::json!([]))
            }),
        )
        .map_err(AppError::from)?
        .into_iter()
        .map(|c| match c {
            crate::modules::gamebox::healthcheck::AppHealthcheck::Http { port, .. } => {
                ("http".to_string(), port)
            }
            crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { port } => {
                ("tcp".to_string(), port)
            }
        })
        .collect::<Vec<_>>();

        let broken =
            break_repo::already_broken(db, event_id, eg.id, subject.user_id, subject.team_id)
                .await?;

        // 实例状态。
        let instance_view =
            match runtime::get_my_instance_view(db, event_id, eg.id, subject).await? {
                Some(v) => Some(InstanceViewDto::from(&v)),
                None => None,
            };

        gb_dtos.push(AwdpGameBoxDto {
            id: eg.id,
            gamebox_id: eg.gamebox_id,
            name: gamebox.name.clone(),
            category: gamebox.category.clone(),
            enabled: eg.enabled,
            hidden: eg.hidden,
            exposed,
            broken,
            instance: instance_view,
            source_code_dir: if awdp.phase == AwdpPhase::Fix {
                gamebox.awdp_source_code_dir.clone()
            } else {
                None
            },
        });
    }

    let config = AwdpConfig {
        break_duration_secs: awdp.break_duration_secs,
        fix_duration_secs: awdp.fix_duration_secs,
        fix_round_interval_secs: awdp.fix_round_interval_secs,
        break_score: awdp.break_score,
        fix_round_score: awdp.fix_round_score,
    };
    UniResponse::ok(
        AwdpOverviewDto {
            event_id,
            phase: awdp.phase.clone(),
            break_duration_secs: awdp.break_duration_secs,
            fix_duration_secs: awdp.fix_duration_secs,
            fix_round_interval_secs: awdp.fix_round_interval_secs,
            total_rounds: config.total_rounds(),
            break_score: awdp.break_score,
            fix_round_score: awdp.fix_round_score,
            started_at: awdp.started_at,
            break_ends_at: awdp.break_ends_at,
            fix_started_at: awdp.fix_started_at,
            fix_ends_at: awdp.fix_ends_at,
            finished_at: awdp.finished_at,
            current_round: awdp.current_round,
            next_action_at: awdp.next_action_at,
            my_score,
            gameboxes: gb_dtos,
        }
        .into(),
    )
    .into()
}

/// POST /api/events/{event_id}/awdp/gameboxes/{eg_id}/instance —— 启动/复用。
#[post("{event_id}/awdp/gameboxes/{eg_id}/instance")]
pub async fn start_my_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<InstanceViewDto> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let flag_prefix = crate::infrastructure::settings::get_setting(ctx.db.get_ref(), "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());
    let view = runtime::start_instance(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        event_id,
        eg_id,
        subject,
        &flag_prefix,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(InstanceViewDto::from(&view).into()).into()
}

/// POST /api/events/{event_id}/awdp/gameboxes/{eg_id}/instance/stop —— 停止。
#[post("{event_id}/awdp/gameboxes/{eg_id}/instance/stop")]
pub async fn stop_my_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let view = runtime::get_my_instance_view(ctx.db.get_ref(), event_id, eg_id, subject).await?;
    let Some(view) = view else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    runtime::stop_instance(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        view.instance_id,
        subject,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

/// GET /api/events/{event_id}/awdp/gameboxes/{eg_id}/instance —— 实例详情。
#[get("{event_id}/awdp/gameboxes/{eg_id}/instance")]
pub async fn get_my_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<Option<InstanceViewDto>> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let view = runtime::get_my_instance_view(ctx.db.get_ref(), event_id, eg_id, subject).await?;
    UniResponse::ok(view.map(|v| InstanceViewDto::from(&v)).into()).into()
}

/// POST /api/events/{event_id}/awdp/gameboxes/{eg_id}/break —— Break flag 提交。
#[post("{event_id}/awdp/gameboxes/{eg_id}/break")]
pub async fn submit_break_flag(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
    body: Json<BreakSubmitRequest>,
) -> UniResult<BreakSubmitResponse> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let result = break_service::submit_flag(
        ctx.db.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        event_id,
        eg_id,
        &body.flag,
        subject,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(
        BreakSubmitResponse {
            accepted: result.accepted,
            scored: result.scored,
            already_broken: result.already_broken,
        }
        .into(),
    )
    .into()
}

/// POST .../instance/reset —— 玩家 Reset（pristine 重建，保留端点）。
#[post("{event_id}/awdp/gameboxes/{eg_id}/instance/reset")]
pub async fn reset_my_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<InstanceViewDto> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let flag_prefix = crate::infrastructure::settings::get_setting(ctx.db.get_ref(), "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), event_id, eg_id, subject).await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    let view = runtime::reset_instance(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        view.instance_id,
        subject,
        &flag_prefix,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(InstanceViewDto::from(&view).into()).into()
}

#[derive(Debug, MultipartForm)]
struct PatchUploadForm {
    #[multipart(rename = "patch_file")]
    patch_file: TempFile,
}

/// POST .../patch —— 上传并应用 patch.sh（Fix only；multipart）。
#[post("{event_id}/awdp/gameboxes/{eg_id}/patch")]
pub async fn upload_patch(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
    MultipartForm(form): MultipartForm<PatchUploadForm>,
) -> UniResult<PatchSubmitResponse> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), event_id, eg_id, subject).await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    let bytes = std::fs::read(form.patch_file.file.path())
        .map_err(|e| AppError::BadRequest(format!("read patch_file: {e}")))?;
    let script = String::from_utf8(bytes)
        .map_err(|_| AppError::Validation("patch.sh 必须是 UTF-8 文本脚本".into()))?;
    if script.len() > patch_service::MAX_PATCH_BYTES {
        return Err(AppError::Validation(format!(
            "patch.sh 超过 {} KiB 上限",
            patch_service::MAX_PATCH_BYTES / 1024
        )));
    }
    let result = patch_service::apply_patch(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        event_id,
        view.instance_id,
        &script,
        subject,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(
        PatchSubmitResponse {
            status: match result {
                patch_service::PatchResult::Applied => "applied".into(),
                patch_service::PatchResult::Failed => "failed".into(),
            },
        }
        .into(),
    )
    .into()
}

/// POST .../test-check —— 手动 Test Check（healthcheck + judge，不计分）。
#[post("{event_id}/awdp/gameboxes/{eg_id}/test-check")]
pub async fn manual_test_check(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<ManualCheckDto> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), event_id, eg_id, subject).await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    let result = evaluation::manual_check(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        event_id,
        view.instance_id,
        subject,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(
        ManualCheckDto {
            healthcheck_ok: result.healthcheck_ok,
            healthcheck_detail: result.healthcheck_detail,
            judge_ok: result.judge_ok,
            judge_detail: result.judge_detail,
        }
        .into(),
    )
    .into()
}

/// GET .../source —— source.zip 私有下载（Fix only；presigned /private/ 代理路径）。
#[get("{event_id}/awdp/gameboxes/{eg_id}/source")]
pub async fn download_source(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<String> {
    let (event_id, eg_id) = path.into_inner();
    let _user = user.into_inner();
    let _subject = resolve_subject(&ctx, event_id, _user.id).await?;
    let db = ctx.db.get_ref();

    let awdp = event_repo::require_by_event_id(db, event_id).await?;
    if awdp.phase != AwdpPhase::Fix {
        return Err(AppError::InvalidState(
            "source.zip 仅在 Fix 阶段可下载".into(),
        ));
    }
    let (eg, gamebox) = event_gamebox_repo::effective_gamebox_spec(db, eg_id).await?;
    let _ = eg;
    let key = gamebox.awdp_source_artifact_key.ok_or_else(|| {
        AppError::NotFound("该 GameBox 没有 source.zip 产物（无 [awdp] capability）".into())
    })?;
    let url = crate::modules::platform::files::download::presign_private_download_url(
        ctx.rustfs.clone(),
        &key,
        300,
    )
    .await
    .map_err(|e| AppError::BadRequest(e.to_string()))?;
    UniResponse::ok(url.into()).into()
}

/// 路由注册（挂进 /api/events scope，与 common 同组，见 bootstrap/routes.rs 注释）。
pub fn player_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_overview)
        .service(start_my_instance)
        .service(stop_my_instance)
        .service(reset_my_instance)
        .service(get_my_instance)
        .service(submit_break_flag)
        .service(upload_patch)
        .service(manual_test_check)
        .service(download_source);
}
