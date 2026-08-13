//! AWDP 练习 Judge 管理端 API（挂 /api/admin/events/{event_id}/awdp/practice-judge/*）。
//!
//! 配置页（VirtualEvent → AWDP Practice）：
//! - GET/PATCH 配置（enabled / judge_server_url / interval_secs / flag_path）
//! - POST deploy / stop（部署/停止练习子网内的 JudgeServer 容器）
//! - GET results（最近检查结果）

use actix_web::web::{self, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard, prelude::*},
    entity::{events, sea_orm_active_enums::EventFamily},
    modules::event::awdp::{
        repo::{event_repo, practice_judge_repo},
        service::practice_judge,
    },
};

/// 练习 Judge 配置 DTO（Pull worker 状态，plan §49）。
#[derive(Debug, Clone, Serialize)]
pub struct PracticeJudgeConfigDto {
    pub event_id: Uuid,
    pub enabled: bool,
    pub judge_server_url: String,
    pub interval_secs: i32,
    pub flag_path: String,
    pub container_status: String,
    pub container_id: Option<String>,
    pub last_sweep_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// 自动推导的 JudgeServer 基址（配置留空时展示）。
    pub resolved_judge_server_url: String,
    /// 练习子网名（展示）。
    pub network_name: String,
    /// worker 存活探测（容器 running + data /healthz 可达 → healthy）。
    pub worker_health: String,
    /// data plane 端点（玩家 contract：awdp-judge:8080，仅 GameBox 内部可达）。
    pub data_endpoint: String,
    /// pending 评估数。
    pub pending_evaluations: i64,
    /// running（含 lease）评估数。
    pub running_evaluations: i64,
    /// 最近心跳（running 评估的最大 heartbeat_at）。
    pub last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
}

/// 配置更新请求。
#[derive(Debug, Deserialize)]
pub struct PracticeJudgePatchRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub judge_server_url: Option<String>,
    #[serde(default)]
    pub interval_secs: Option<i32>,
    #[serde(default)]
    pub flag_path: Option<String>,
}

/// 检查结果 DTO。
#[derive(Debug, Clone, Serialize)]
pub struct PracticeJudgeResultDto {
    pub id: Uuid,
    pub run_id: Uuid,
    pub instance_id: Uuid,
    pub gamebox_id: Uuid,
    pub gamebox_name: String,
    pub owner_user_id: Option<Uuid>,
    pub owner_team_id: Option<Uuid>,
    pub check_kind: String,
    pub status: String,
    pub detail: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

async fn ensure_awdp_practice_event(ctx: &ReqCtx, event_id: Uuid) -> Result<(), AppError> {
    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound("event not found".into()))?;
    if event.family != EventFamily::Awdp {
        return Err(AppError::Validation("not an AWDP event".into()));
    }
    // AWDPlusPractice 系统练习赛事（system_key=awdp-practice）才允许配置练习 Judge。
    if event.system_key.as_deref() != Some(crate::core::system_ids::EVENT_PRACTICE_AWDP_SYSTEM_KEY)
    {
        return Err(AppError::Validation(
            "练习 Judge 仅可在 AWDPlusPractice 虚拟赛事配置".into(),
        ));
    }
    event_repo::ensure_by_event_id(ctx.db.get_ref(), event_id, &Default::default()).await?;
    Ok(())
}

async fn build_config_dto(
    db: &sea_orm::DatabaseConnection,
    settings: &crate::entity::awdp_practice_judge_settings::Model,
    config: &crate::core::config::AwdpStaticConfig,
    container_status: &str,
) -> PracticeJudgeConfigDto {
    let (pending, running, last_heartbeat) =
        crate::modules::event::awdp::service::judge_worker::queue_stats(db).await;
    PracticeJudgeConfigDto {
        event_id: settings.event_id,
        enabled: settings.enabled,
        judge_server_url: settings.judge_server_url.clone(),
        interval_secs: settings.interval_secs,
        flag_path: settings.flag_path.clone(),
        container_status: container_status.to_string(),
        container_id: settings.container_id.clone(),
        last_sweep_at: settings
            .last_sweep_at
            .map(|t| t.with_timezone(&chrono::Utc)),
        updated_at: settings.updated_at.with_timezone(&chrono::Utc),
        resolved_judge_server_url: practice_judge::resolve_judge_server_url(settings, config),
        network_name: crate::modules::event::awdp::domain::judge::PRACTICE_NETWORK_NAME.to_string(),
        worker_health: "unknown".to_string(),
        data_endpoint: format!(
            "http://{}:{}",
            config.practice_judge_data_host,
            crate::modules::event::awdp::domain::judge::PRACTICE_JUDGE_PORT
        ),
        pending_evaluations: pending,
        running_evaluations: running,
        last_heartbeat,
    }
}

/// 组装 DTO + worker 存活探测（容器 running 时打 data /healthz）。
async fn build_dto_with_health(
    ctx: &ReqCtx,
    settings: &crate::entity::awdp_practice_judge_settings::Model,
    config: &crate::core::config::AwdpStaticConfig,
    status: &str,
) -> Result<PracticeJudgeConfigDto, AppError> {
    let mut dto = build_config_dto(ctx.db.get_ref(), settings, config, status).await;
    if status == "running" {
        dto.worker_health =
            practice_judge::judge_worker_health(ctx.docker.get_ref(), config).await?;
    } else {
        dto.worker_health = status.to_string();
    }
    Ok(dto)
}

/// GET /api/admin/events/{event_id}/awdp/practice-judge —— 配置 + 容器状态。
#[get("{event_id}/awdp/practice-judge")]
pub async fn get_practice_judge(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<PracticeJudgeConfigDto> {
    let event_id = path.into_inner();
    ensure_awdp_practice_event(&ctx, event_id).await?;
    let settings = practice_judge_repo::ensure_settings(ctx.db.get_ref(), event_id).await?;
    let status =
        practice_judge::container_status(ctx.db.get_ref(), ctx.docker.get_ref(), &settings).await;
    let dto = build_dto_with_health(&ctx, &settings, &ctx.config.awdp, &status).await?;
    UniResponse::ok(dto.into()).into()
}

/// PATCH /api/admin/events/{event_id}/awdp/practice-judge —— 更新配置。
#[patch("{event_id}/awdp/practice-judge")]
pub async fn patch_practice_judge(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: Json<PracticeJudgePatchRequest>,
) -> UniResult<PracticeJudgeConfigDto> {
    let event_id = path.into_inner();
    ensure_awdp_practice_event(&ctx, event_id).await?;
    let body = body.into_inner();

    // 校验区间与路径。
    if let Some(secs) = body.interval_secs
        && !(10..=86400).contains(&secs)
    {
        return Err(AppError::Validation(
            "interval_secs 必须在 10~86400 之间".into(),
        ));
    }
    if let Some(path) = body.flag_path.as_deref()
        && (!path.starts_with('/') || path.len() > 200)
    {
        return Err(AppError::Validation(
            "flag_path 必须以 / 开头且不超过 200 字符".into(),
        ));
    }
    if let Some(url) = body.judge_server_url.as_deref()
        && !url.trim().is_empty()
        && !url.starts_with("http://")
        && !url.starts_with("https://")
    {
        return Err(AppError::Validation(
            "judge_server_url 必须是 http(s):// 地址或留空".into(),
        ));
    }

    let settings = practice_judge_repo::update_settings(
        ctx.db.get_ref(),
        event_id,
        &practice_judge_repo::PracticeJudgeSettingsPatch {
            enabled: body.enabled,
            judge_server_url: body.judge_server_url,
            interval_secs: body.interval_secs,
            flag_path: body.flag_path,
        },
    )
    .await?;
    crate::modules::event::common::application::event_log_service::insert_event_log(
        ctx.db.get_ref(),
        event_id,
        None,
        None,
        "info",
        "awdp.practice_judge.config",
        serde_json::json!({
            "enabled": settings.enabled,
            "interval_secs": settings.interval_secs,
            "flag_path": settings.flag_path,
        }),
    )
    .await;
    let status =
        practice_judge::container_status(ctx.db.get_ref(), ctx.docker.get_ref(), &settings).await;
    let dto = build_dto_with_health(&ctx, &settings, &ctx.config.awdp, &status).await?;
    UniResponse::ok(dto.into()).into()
}

/// POST /api/admin/events/{event_id}/awdp/practice-judge/deploy —— 部署 JudgeServer 容器。
#[post("{event_id}/awdp/practice-judge/deploy")]
pub async fn deploy_practice_judge(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<PracticeJudgeConfigDto> {
    let event_id = path.into_inner();
    ensure_awdp_practice_event(&ctx, event_id).await?;
    // 练习事件必须已存在（幂等 ensure AWDPlusPractice）。
    practice_judge::ensure_practice_event(ctx.db.get_ref()).await?;
    practice_judge::deploy_judge(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        &ctx.config.awdp,
        ctx.config.auth.jwt_secret.expose().as_bytes(),
        event_id,
    )
    .await
    .map_err(AppError::from)?;
    crate::modules::event::common::application::event_log_service::insert_event_log(
        ctx.db.get_ref(),
        event_id,
        None,
        None,
        "info",
        "awdp.practice_judge.deploy",
        serde_json::json!({ "ip": ctx.config.awdp.practice_judge_ip }),
    )
    .await;
    let settings = practice_judge_repo::ensure_settings(ctx.db.get_ref(), event_id).await?;
    let status =
        practice_judge::container_status(ctx.db.get_ref(), ctx.docker.get_ref(), &settings).await;
    let dto = build_dto_with_health(&ctx, &settings, &ctx.config.awdp, &status).await?;
    UniResponse::ok(dto.into()).into()
}

/// POST /api/admin/events/{event_id}/awdp/practice-judge/stop —— 停止 JudgeServer 容器。
#[post("{event_id}/awdp/practice-judge/stop")]
pub async fn stop_practice_judge(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<PracticeJudgeConfigDto> {
    let event_id = path.into_inner();
    ensure_awdp_practice_event(&ctx, event_id).await?;
    practice_judge::stop_judge(ctx.db.get_ref(), ctx.docker.get_ref(), event_id)
        .await
        .map_err(AppError::from)?;
    crate::modules::event::common::application::event_log_service::insert_event_log(
        ctx.db.get_ref(),
        event_id,
        None,
        None,
        "info",
        "awdp.practice_judge.stop",
        serde_json::json!({}),
    )
    .await;
    let settings = practice_judge_repo::ensure_settings(ctx.db.get_ref(), event_id).await?;
    let status =
        practice_judge::container_status(ctx.db.get_ref(), ctx.docker.get_ref(), &settings).await;
    let dto = build_dto_with_health(&ctx, &settings, &ctx.config.awdp, &status).await?;
    UniResponse::ok(dto.into()).into()
}

/// GET /api/admin/events/{event_id}/awdp/practice-judge/results —— 最近检查结果。
#[get("{event_id}/awdp/practice-judge/results")]
pub async fn list_practice_judge_results(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> UniResult<Vec<PracticeJudgeResultDto>> {
    let event_id = path.into_inner();
    ensure_awdp_practice_event(&ctx, event_id).await?;
    let limit: u64 = query
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
        .clamp(1, 200);
    let rows = practice_judge_repo::list_results(ctx.db.get_ref(), event_id, limit).await?;
    // 批量拉 gamebox 名称（展示）。
    let gamebox_ids: Vec<Uuid> = rows.iter().map(|r| r.gamebox_id).collect();
    let gamebox_names = if gamebox_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        crate::entity::gameboxes::Entity::find()
            .filter(crate::entity::gameboxes::Column::Id.is_in(gamebox_ids))
            .all(ctx.db.get_ref())
            .await
            .map_err(|e| AppError::Database(e.to_string()))?
            .into_iter()
            .map(|g| (g.id, g.name))
            .collect()
    };
    let dtos: Vec<PracticeJudgeResultDto> = rows
        .into_iter()
        .map(|r| {
            let gamebox_name = gamebox_names
                .get(&r.gamebox_id)
                .cloned()
                .unwrap_or_else(|| r.gamebox_id.to_string());
            PracticeJudgeResultDto {
                id: r.id,
                run_id: r.run_id,
                instance_id: r.instance_id,
                gamebox_id: r.gamebox_id,
                gamebox_name,
                owner_user_id: r.owner_user_id,
                owner_team_id: r.owner_team_id,
                check_kind: r.check_kind,
                status: r.status,
                detail: r.detail,
                created_at: r.created_at.with_timezone(&chrono::Utc),
            }
        })
        .collect();
    UniResponse::ok(dtos.into()).into()
}

/// 注册到 /api/admin/events scope（与 awdp admin_events_routes 同组）。
pub fn practice_judge_admin_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_practice_judge)
        .service(patch_practice_judge)
        .service(deploy_practice_judge)
        .service(stop_practice_judge)
        .service(list_practice_judge_results);
}
