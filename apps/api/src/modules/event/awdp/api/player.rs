//! AWDP 选手侧路由（挂 /api/events/{event_id}/awdp/*）。
//!
//! URL 保持 event-oriented（plan §59），handler 内经
//! `run_repo::find_active_competition_for_event` 解析到 active competition run
//! （动作类端点）；展示类端点（overview/rounds/evaluations）用
//! `find_display_run_for_event`——run 结束（Ended）或过渡态（PreparingFix）时
//! 也要能如实展示，避免把已结束赛事误报为 pending。

use actix_web::web::{self, Json};
use uuid::Uuid;

use actix_multipart::form::{MultipartForm, tempfile::TempFile};

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::UserJwtGuard, prelude::*},
    entity::{
        awdp_runs, events,
        sea_orm_active_enums::{AwdpPhase, EventFamily, ParticipantMode},
    },
    modules::event::awdp::{
        api::dto::*,
        repo::{evaluation_repo, event_gamebox_repo, event_repo, round_repo, run_repo, score_repo},
        service::{
            break_service, evaluation, patch_service,
            runtime::{self, Subject},
            trend,
        },
    },
};

/// 解析 subject（Individual → user；Team → 事件内战队）。
/// §58 授权修复：Individual 模式必须已加入赛事（event_users 注册行），未加入 → 403。
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
        // Individual：必须已注册（event_users 行；join_event 或管理员预注册）。
        crate::modules::event::awdp::service::authorization::require_event_participant(
            ctx.db.get_ref(),
            event_id,
            user_id,
        )
        .await
        .map_err(AppError::from)?;
        Ok(Subject::user(user_id))
    }
}

/// 解析事件的 active competition run（未开始返回 None）。
async fn active_run_for_event(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
) -> Result<Option<awdp_runs::Model>, AppError> {
    Ok(run_repo::find_active_competition_for_event(db, event_id).await?)
}

/// 解析事件的展示用 run（active 优先，否则最新一条——覆盖 Ended / PreparingFix）。
async fn display_run_for_event(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
) -> Result<Option<awdp_runs::Model>, AppError> {
    Ok(run_repo::find_display_run_for_event(db, event_id).await?)
}

async fn flag_prefix_async(ctx: &ReqCtx) -> String {
    crate::infrastructure::settings::get_setting(ctx.db.get_ref(), "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into())
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
    if event.family != EventFamily::Awdp {
        return Err(AppError::Validation("not an AWDP event".into()));
    }

    let awdp = event_repo::ensure_by_event_id(
        db,
        event_id,
        &crate::modules::event::awdp::domain::AwdpConfig::default(),
    )
    .await?;
    // 展示用 run：active 优先，否则回退最新一条（Ended 时仍显示 Finished + 全量
    // timestamps/score；PreparingFix 过渡态如实展示，不再误报 pending）。
    let run = display_run_for_event(db, event_id).await?;
    let gameboxes = event_gamebox_repo::list_for_event(db, event_id).await?;
    let my_score = match &run {
        Some(run) => score_repo::my_total(db, run.id, subject.user_id, subject.team_id).await?,
        None => 0,
    };

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

        // 实例状态（run 未启动时无实例）。
        let (broken, instance_view) = match &run {
            Some(run) => {
                let broken = crate::modules::event::awdp::repo::break_repo::already_broken(
                    db,
                    run.id,
                    eg.gamebox_id,
                    subject.user_id,
                    subject.team_id,
                )
                .await?;
                let view = match runtime::get_my_instance_view(db, run.id, eg.gamebox_id, subject)
                    .await?
                {
                    Some(v) => Some(InstanceViewDto::from(&v)),
                    None => None,
                };
                (broken, view)
            }
            None => (false, None),
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
            source_code_dir: if run
                .as_ref()
                .map(|r| r.phase == AwdpPhase::Fix)
                .unwrap_or(false)
            {
                gamebox.awdp_source_code_dir.clone()
            } else {
                None
            },
        });
    }

    let config = crate::modules::event::awdp::domain::AwdpConfig {
        break_duration_secs: awdp.break_duration_secs,
        fix_duration_secs: awdp.fix_duration_secs,
        fix_round_interval_secs: awdp.fix_round_interval_secs,
        break_score: awdp.break_score,
        fix_round_score: awdp.fix_round_score,
    };
    UniResponse::ok(
        AwdpOverviewDto {
            event_id,
            phase: run
                .as_ref()
                .map(|r| r.phase.clone())
                .unwrap_or(AwdpPhase::Pending),
            break_duration_secs: awdp.break_duration_secs,
            fix_duration_secs: awdp.fix_duration_secs,
            fix_round_interval_secs: awdp.fix_round_interval_secs,
            total_rounds: config.total_rounds(),
            break_score: awdp.break_score,
            fix_round_score: awdp.fix_round_score,
            started_at: run.as_ref().and_then(|r| r.started_at),
            break_ends_at: run.as_ref().and_then(|r| r.break_ends_at),
            fix_started_at: run.as_ref().and_then(|r| r.fix_started_at),
            fix_ends_at: run.as_ref().and_then(|r| r.fix_ends_at),
            finished_at: run.as_ref().and_then(|r| r.finished_at),
            current_round: run.as_ref().map(|r| r.current_round).unwrap_or(0),
            next_action_at: run.as_ref().and_then(|r| r.next_action_at),
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
    let run = active_run_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    let eg = event_gamebox_repo::require_by_id(ctx.db.get_ref(), eg_id).await?;
    let view = runtime::start_instance(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        &ctx.config.awdp,
        run.id,
        eg.gamebox_id,
        subject,
        &flag_prefix_async(&ctx).await,
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
    let run = active_run_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    let eg = event_gamebox_repo::require_by_id(ctx.db.get_ref(), eg_id).await?;
    let view =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, eg.gamebox_id, subject).await?;
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
    let run = active_run_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    let eg = event_gamebox_repo::require_by_id(ctx.db.get_ref(), eg_id).await?;
    let view =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, eg.gamebox_id, subject).await?;
    UniResponse::ok(view.map(|v| InstanceViewDto::from(&v)).into()).into()
}

/// POST /api/events/{event_id}/awdp/gameboxes/{eg_id}/break —— Break flag 提交。
#[post("{event_id}/awdp/gameboxes/{eg_id}/break")]
pub async fn submit_break_flag(
    user: UserJwtGuard,
    ctx: ReqCtx,
    state: web::Data<crate::bootstrap::AppState>,
    path: web::Path<(Uuid, Uuid)>,
    body: Json<BreakSubmitRequest>,
) -> UniResult<BreakSubmitResponse> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let run = active_run_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    let eg = event_gamebox_repo::require_by_id(ctx.db.get_ref(), eg_id).await?;
    let result = break_service::submit_flag(
        ctx.db.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        run.id,
        eg.gamebox_id,
        &body.flag,
        subject,
    )
    .await
    .map_err(AppError::from)?;
    if result.scored {
        crate::modules::event::awdp::realtime::score_changed(
            &state,
            event_id,
            "break",
            run.break_score,
        );
    }
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
    let run = active_run_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    let eg = event_gamebox_repo::require_by_id(ctx.db.get_ref(), eg_id).await?;
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, eg.gamebox_id, subject).await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    let view = runtime::reset_instance(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        view.instance_id,
        subject,
        &flag_prefix_async(&ctx).await,
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
    state: web::Data<crate::bootstrap::AppState>,
    path: web::Path<(Uuid, Uuid)>,
    MultipartForm(form): MultipartForm<PatchUploadForm>,
) -> UniResult<PatchSubmitResponse> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let run = active_run_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    let eg = event_gamebox_repo::require_by_id(ctx.db.get_ref(), eg_id).await?;
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, eg.gamebox_id, subject).await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    let bytes = std::fs::read(form.patch_file.file.path())
        .map_err(|e| AppError::BadRequest(format!("read patch_file: {e}")))?;
    let payload = patch_service::extract_patch_payload(&bytes).map_err(AppError::Validation)?;
    let result = patch_service::apply_patch(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        run.id,
        view.instance_id,
        &payload,
        subject,
    )
    .await
    .map_err(AppError::from)?;
    let (status, error_message) = match result {
        patch_service::PatchResult::Applied => ("applied", None),
        patch_service::PatchResult::Failed(reason) => ("failed", Some(reason)),
    };
    crate::modules::event::awdp::realtime::patch_applied(
        &state,
        event_id,
        view.instance_id,
        status,
    );
    UniResponse::ok(
        PatchSubmitResponse {
            status: status.into(),
            error_message,
        }
        .into(),
    )
    .into()
}

/// POST .../test-check —— 手动 Test Check 同步执行（不排队）：HTTP 请求内直接
/// healthcheck + judge，写终态并返回结果；不计分。
#[post("{event_id}/awdp/gameboxes/{eg_id}/test-check")]
pub async fn manual_test_check(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<ManualCheckDto> {
    let (event_id, eg_id) = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let run = active_run_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    let eg = event_gamebox_repo::require_by_id(ctx.db.get_ref(), eg_id).await?;
    let Some(view) =
        runtime::get_my_instance_view(ctx.db.get_ref(), run.id, eg.gamebox_id, subject).await?
    else {
        return Err(AppError::NotFound("instance not started".into()));
    };
    let evaluation =
        evaluation::manual_check_enqueue(ctx.db.get_ref(), run.id, view.instance_id, subject)
            .await
            .map_err(AppError::from)?;
    // 同步执行：不排队等 worker，直接跑完 healthcheck + judge 再返回终态。
    let result =
        evaluation::manual_check_run_now(ctx.db.get_ref(), ctx.docker.get_ref(), &evaluation)
            .await
            .map_err(AppError::from)?;
    UniResponse::ok(
        ManualCheckDto {
            evaluation_id: evaluation.id,
            status: "completed".to_string(),
            healthcheck_ok: Some(result.healthcheck_ok),
            healthcheck_detail: Some(result.healthcheck_detail),
            judge_ok: Some(result.judge_ok),
            judge_detail: Some(result.judge_detail),
            // Fix 阶段 Test Check 含 exploit 诊断（与练习模式一致；不计分）。
            exploit_ok: result.exploit_ok,
            exploit_detail: result.exploit_detail,
        }
        .into(),
    )
    .into()
}

/// GET .../source —— source.tar.gz 私有下载（Fix only；presigned /private/ 代理路径）。
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

    let run = active_run_for_event(db, event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    if run.phase != AwdpPhase::Fix {
        return Err(AppError::InvalidState(
            "source.tar.gz 仅在 Fix 阶段可下载".into(),
        ));
    }
    let (eg, gamebox) = event_gamebox_repo::effective_gamebox_spec(db, eg_id).await?;
    let _ = eg;
    let key = gamebox.awdp_source_artifact_key.ok_or_else(|| {
        AppError::NotFound("该 GameBox 没有 source.tar.gz 产物（无 [awdp] capability）".into())
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

/// GET {event_id}/awdp/scores —— 赛事官方积分榜（按 participant_mode 聚合 user/team）。
#[get("{event_id}/awdp/scores")]
pub async fn get_scoreboard(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<crate::modules::event::awdp::service::scoreboard::AwdpScoreRow>> {
    let event_id = path.into_inner();
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound("event not found".into()))?;
    if event.family != EventFamily::Awdp {
        return Err(AppError::Validation("not an AWDP event".into()));
    }
    let rows =
        crate::modules::event::awdp::service::scoreboard::get_scoreboard(ctx.db.get_ref(), &event)
            .await
            .map_err(AppError::from)?;
    UniResponse::ok(rows.into()).into()
}

/// GET {event_id}/awdp/trend —— AWDP 官方积分趋势（Break/Fix 累计得分曲线）。
#[get("{event_id}/awdp/trend")]
pub async fn get_trend(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<trend::TrendItem>> {
    let event_id = path.into_inner();
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound("event not found".into()))?;
    if event.family != EventFamily::Awdp {
        return Err(AppError::Validation("not an AWDP event".into()));
    }
    let items = trend::get_trend(ctx.db.get_ref(), &event)
        .await
        .map_err(AppError::from)?;
    UniResponse::ok(items.into()).into()
}

/// GET {event_id}/awdp/scoreboard —— 选手端积分榜明细矩阵：
/// 汇总行 + Break 每题攻破状态 + Fix 每题每回合官方结果 + 每题 fix 得分。
#[get("{event_id}/awdp/scoreboard")]
pub async fn get_scoreboard_detail(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<crate::modules::event::awdp::service::scoreboard::AwdpScoreboardDetail> {
    let event_id = path.into_inner();
    let user = user.into_inner();
    let event = events::Entity::find_by_id(event_id)
        .filter(events::Column::Hidden.eq(false))
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound("event not found".into()))?;
    if event.family != EventFamily::Awdp {
        return Err(AppError::Validation("not an AWDP event".into()));
    }
    // is_me 高亮：Team 模式查当前用户所属队伍（无队伍 → None，不强制要求已加入，
    // 与现有 /scores 的"登录即可看榜"语义一致）。
    let me_team_id = if event.participant_mode == ParticipantMode::Team {
        crate::modules::event::common::infrastructure::event_repository::find_user_team_membership(
            ctx.db.get_ref(),
            event_id,
            user.id,
        )
        .await
        .ok()
        .flatten()
        .map(|m| m.team_id)
    } else {
        None
    };
    let detail = crate::modules::event::awdp::service::scoreboard::get_scoreboard_detail(
        ctx.db.get_ref(),
        &event,
        Some(user.id),
        me_team_id,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(detail.into()).into()
}

/// GET {event_id}/awdp/stream —— AWDP 实时事件流（competition 按 event_id 过滤）。
#[get("{event_id}/awdp/stream")]
pub async fn event_stream(
    _user: UserJwtGuard,
    hub: web::Data<crate::infrastructure::realtime::BroadcastEventPublisher>,
    path: web::Path<Uuid>,
) -> actix_web::HttpResponse {
    use futures_util::stream::unfold;

    let event_id = path.into_inner();
    let rx = hub.subscribe();

    let body = unfold(
        (rx, false, event_id),
        |(mut rx, primed, event_id)| async move {
            if !primed {
                return Some((
                    Ok::<_, actix_web::Error>(web::Bytes::from(": connected\n\n")),
                    (rx, true, event_id),
                ));
            }
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        if ev.event_id != event_id {
                            continue;
                        }
                        match serde_json::to_string(&ev) {
                            Ok(json) => {
                                return Some((
                                    Ok(web::Bytes::from(format!("data: {json}\n\n"))),
                                    (rx, true, event_id),
                                ));
                            }
                            Err(_) => continue,
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let payload = serde_json::json!({
                            "type": "stream.lagged",
                            "event_id": event_id,
                        });
                        return Some((
                            Ok(web::Bytes::from(format!("data: {payload}\n\n"))),
                            (rx, true, event_id),
                        ));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );
    actix_web::HttpResponse::Ok()
        .insert_header((actix_web::http::header::CONTENT_TYPE, "text/event-stream"))
        .insert_header((actix_web::http::header::CACHE_CONTROL, "no-cache"))
        .insert_header((actix_web::http::header::CONNECTION, "keep-alive"))
        .streaming(body)
}

/// GET {event_id}/awdp/rounds —— Fix 回合时间线（选手展示）。
#[get("{event_id}/awdp/rounds")]
pub async fn get_rounds(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<AwdpRoundDto>> {
    let event_id = path.into_inner();
    let _user = user.into_inner();
    let run = display_run_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    let rounds = round_repo::list_for_run(ctx.db.get_ref(), run.id).await?;
    UniResponse::ok(
        rounds
            .into_iter()
            .map(|r| AwdpRoundDto {
                id: r.id,
                sequence: r.sequence,
                starts_at: r.starts_at,
                cutoff_at: r.cutoff_at,
                status: r.status,
            })
            .collect::<Vec<_>>()
            .into(),
    )
    .into()
}

/// GET {event_id}/awdp/evaluations —— 我的官方评估历史。
#[get("{event_id}/awdp/evaluations")]
pub async fn get_my_evaluations(
    user: UserJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<AwdpEvaluationDto>> {
    let event_id = path.into_inner();
    let user = user.into_inner();
    let subject = resolve_subject(&ctx, event_id, user.id).await?;
    let run = display_run_for_event(ctx.db.get_ref(), event_id)
        .await?
        .ok_or_else(|| AppError::InvalidState("AWDP 事件尚未开始".into()))?;
    let all = evaluation_repo::list_for_run(ctx.db.get_ref(), run.id).await?;
    let mut out = Vec::new();
    for ev in all {
        // 只暴露本人实例的评估。
        let owned = crate::modules::event::awdp::repo::instance_repo::find_by_instance_id(
            ctx.db.get_ref(),
            ev.instance_id,
        )
        .await
        .map(|(inst, ext)| {
            let ok = match subject {
                Subject {
                    user_id: Some(u),
                    team_id: None,
                } => inst.owner_user_id == Some(u),
                Subject {
                    user_id: None,
                    team_id: Some(t),
                } => inst.owner_team_id == Some(t),
                _ => false,
            };
            (ok, ext.gamebox_id)
        })
        .unwrap_or((false, Uuid::nil()));
        if !owned.0 {
            continue;
        }
        let round_seq = match ev.fix_round_id {
            Some(rid) => round_repo::find_by_id(ctx.db.get_ref(), rid)
                .await
                .map(|r| r.sequence)
                .ok(),
            None => None,
        };
        out.push(AwdpEvaluationDto {
            id: ev.id,
            instance_id: ev.instance_id,
            gamebox_id: owned.1,
            fix_round_id: ev.fix_round_id,
            round_sequence: round_seq,
            kind: ev.kind,
            status: ev.status,
            healthcheck_result: ev.healthcheck_result,
            judge_result: ev.judge_result,
            exploit_result: ev.exploit_result,
            finished_at: ev.finished_at,
        });
    }
    UniResponse::ok(out.into()).into()
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
        .service(download_source)
        .service(get_rounds)
        .service(get_my_evaluations)
        .service(get_scoreboard)
        .service(get_trend)
        .service(get_scoreboard_detail)
        .service(event_stream);
}
