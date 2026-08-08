//! GameBox library + EventGameBox admin handlers（§46 术语统一）。
//!
//! - `/api/admin/awd/gameboxes`：全局 GameBox 库（identity + immutable Revision）
//! - `/api/admin/events/{event_id}/awd/gameboxes`：赛事选择（pin revision + 计分/资源配置）
//!
//! Handler 只做 parse/auth/service 调用（§64），复杂逻辑一律在 service/repo。

use actix_web::web;
use sea_orm::{DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};
use uuid::Uuid;

use crate::{
    api::{AppError, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard, prelude::*},
    modules::event::awd_team::{
        domain::instance_ip_for_offset,
        repo::{event_gamebox_repo, event_repo, gamebox_lib_repo},
        service::gamebox_service,
    },
};

use super::dto::*;

// ────────────────────────────────────────────────────────────────────────────
// GameBox 库（全局 identity + Revision）
// ────────────────────────────────────────────────────────────────────────────

/// GET /api/admin/awd/gameboxes
#[get("/awd/gameboxes")]
pub async fn list_gamebox_library(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
) -> UniResult<Vec<GameBoxLibraryDto>> {
    let items = gamebox_lib_repo::list_gameboxes_with_revisions(ctx.db.get_ref(), true)
        .await
        .map_err(AppError::from)?;
    let dto: Vec<GameBoxLibraryDto> = items
        .into_iter()
        .map(|g| GameBoxLibraryDto {
            id: g.gamebox.id,
            name: g.gamebox.name,
            safe_name: g.gamebox.safe_name,
            category: g.gamebox.category,
            description: g.gamebox.description,
            hidden: g.gamebox.hidden,
            latest_revision: g.latest_revision.as_ref().map(GameBoxRevisionDto::from),
        })
        .collect();
    UniResponse::ok(dto.into()).into()
}

/// POST /api/admin/awd/gameboxes —— 创建 GameBox（自动 Revision 1）
#[post("/awd/gameboxes")]
pub async fn create_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    body: web::Json<CreateGameBoxRequest>,
) -> UniResult<GameBoxLibraryDto> {
    let req = body.into_inner();
    let safe_name = match req.safe_name {
        Some(s) => {
            crate::modules::event::awd_team::domain::validate_safe_name(&s)
                .map_err(|e| AppError::Validation(e))?;
            s
        }
        None => gamebox_service::unique_safe_name(ctx.db.get_ref(), &req.name)
            .await
            .map_err(AppError::from)?,
    };
    let (gb, rev) = gamebox_service::create_gamebox_with_revision(
        ctx.db.get_ref(),
        req.name,
        safe_name,
        req.category,
        req.description,
        req.hidden,
        req.config.into_config(),
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(
        GameBoxLibraryDto {
            id: gb.id,
            name: gb.name,
            safe_name: gb.safe_name,
            category: gb.category,
            description: gb.description,
            hidden: gb.hidden,
            latest_revision: Some(GameBoxRevisionDto::from(&rev)),
        }
        .into(),
    )
    .into()
}

/// POST /api/admin/awd/gameboxes/{gamebox_id}/revisions —— 编辑（→ Revision N+1）
/// spec 未变化时返回 200 + latest_revision 不变（§36 不创建重复 revision）。
#[post("/awd/gameboxes/{gamebox_id}/revisions")]
pub async fn edit_gamebox_revision(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: web::Json<EditGameBoxRevisionRequest>,
) -> UniResult<GameBoxLibraryDto> {
    let gamebox_id = path.into_inner();
    let req = body.into_inner();
    let created = gamebox_service::edit_gamebox_revision(
        ctx.db.get_ref(),
        gamebox_id,
        req.config.into_config(),
    )
    .await
    .map_err(AppError::from)?;

    let gb = gamebox_lib_repo::find_gamebox_by_id(ctx.db.get_ref(), gamebox_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("GameBox not found".into()))?;
    let latest = match created {
        Some(r) => Some(r),
        None => gamebox_lib_repo::find_revisions_by_gamebox(ctx.db.get_ref(), gamebox_id)
            .await
            .map_err(AppError::from)?
            .into_iter()
            .next(),
    };
    UniResponse::ok(
        GameBoxLibraryDto {
            id: gb.id,
            name: gb.name,
            safe_name: gb.safe_name,
            category: gb.category,
            description: gb.description,
            hidden: gb.hidden,
            latest_revision: latest.as_ref().map(GameBoxRevisionDto::from),
        }
        .into(),
    )
    .into()
}

/// POST /api/admin/awd/gameboxes/{gamebox_id}/hide —— 归档（§55：被赛事引用禁止 hard delete）
#[post("/awd/gameboxes/{gamebox_id}/hide")]
pub async fn hide_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<()> {
    let gamebox_id = path.into_inner();
    gamebox_lib_repo::set_gamebox_hidden(ctx.db.get_ref(), gamebox_id, true)
        .await
        .map_err(AppError::from)?;
    UniResponse::ok_none().into()
}

// ────────────────────────────────────────────────────────────────────────────
// 赛事 GameBox 选择（EventGameBox）
// ────────────────────────────────────────────────────────────────────────────

/// GET /api/admin/events/{event_id}/awd/gameboxes
#[get("/events/{event_id}/awd/gameboxes")]
pub async fn list_event_gameboxes(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
) -> UniResult<Vec<EventGameBoxDto>> {
    let event_id = path.into_inner();
    let details = event_gamebox_repo::find_event_gameboxes_detail(ctx.db.get_ref(), event_id)
        .await
        .map_err(AppError::from)?;
    let dto: Vec<EventGameBoxDto> = details
        .into_iter()
        .map(|d| EventGameBoxDto {
            id: d.event_gamebox.id,
            gamebox_id: d.gamebox.id,
            gamebox_name: d.gamebox.name,
            gamebox_safe_name: d.gamebox.safe_name,
            revision_id: d.revision.id,
            revision_number: d.revision.revision_number,
            host_offset: d.event_gamebox.host_offset,
            enabled: d.event_gamebox.enabled,
            hidden: d.event_gamebox.hidden,
            cpu_millis: d.event_gamebox.cpu_millis,
            memory_bytes: d.event_gamebox.memory_bytes,
            pids_limit: d.event_gamebox.pids_limit,
            judge_timeout_secs: d.event_gamebox.judge_timeout_secs,
            judge_retry_interval_secs: d.event_gamebox.judge_retry_interval_secs,
            break_points: d.event_gamebox.break_points,
            loss_points: d.event_gamebox.loss_points,
            fix_points: d.event_gamebox.fix_points,
            down_points: d.event_gamebox.down_points,
            first_bonus: d.event_gamebox.first_bonus,
            created_at: d.event_gamebox.created_at,
        })
        .collect();
    UniResponse::ok(dto.into()).into()
}

/// POST /api/admin/events/{event_id}/awd/gameboxes —— 赛事选择 GameBox
/// （默认 pin latest revision；host_offset 缺省自动分配；完成后 touch_configuration §37）
#[post("/events/{event_id}/awd/gameboxes")]
pub async fn add_event_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: web::Json<AddEventGameBoxRequest>,
) -> UniResult<EventGameBoxDto> {
    let event_id = path.into_inner();
    let req = body.into_inner();
    let db: &DatabaseConnection = ctx.db.get_ref();

    // 校验 GameBox 存在
    let gb = gamebox_lib_repo::find_gamebox_by_id(db, req.gamebox_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("GameBox not found".into()))?;

    // §12：Event 内一个 GameBox 只能有一个选择
    if event_gamebox_repo::find_event_gamebox(db, event_id, req.gamebox_id)
        .await
        .map_err(AppError::from)?
        .is_some()
    {
        return Err(AppError::Conflict(format!(
            "GameBox already selected for this event: {}",
            gb.name
        ))
        .into());
    }

    // pin revision：显式指定或 latest（§35：保存后必须 pin 具体 UUID）
    let revision = match req.revision_id {
        Some(rid) => gamebox_lib_repo::find_revision_by_id(db, rid)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound("Revision not found".into()))?,
        None => gamebox_lib_repo::find_revisions_by_gamebox(db, req.gamebox_id)
            .await
            .map_err(AppError::from)?
            .into_iter()
            .next()
            .ok_or_else(|| {
                AppError::NotFound("GameBox has no revision; create one first".into())
            })?,
    };
    if revision.gamebox_id != req.gamebox_id {
        return Err(AppError::Conflict("revision 不属于该 GameBox（§11 一致性）".into()).into());
    }

    // host_offset：显式或自动分配（2..254 未占用）
    let host_offset = match req.host_offset {
        Some(o) => {
            if !(2..=254).contains(&o) {
                return Err(AppError::Validation("host_offset 必须在 2..254".into()).into());
            }
            o
        }
        None => {
            let existing = event_gamebox_repo::find_event_gameboxes_by_event(db, event_id)
                .await
                .map_err(AppError::from)?;
            let used: Vec<i16> = existing.iter().map(|e| e.host_offset).collect();
            let mut free: Option<i16> = None;
            for cand in 2..=254i16 {
                if !used.contains(&cand) {
                    free = Some(cand);
                    break;
                }
            }
            free.ok_or_else(|| AppError::Conflict("no free host_offset (2..254) for event".into()))?
        }
    };

    let eg = event_gamebox_repo::create_event_gamebox(
        db,
        event_id,
        req.gamebox_id,
        revision.id,
        host_offset,
        true,
        req.hidden,
        revision.default_cpu_millis,
        revision.default_memory_bytes,
        revision.default_pids_limit,
        None,
        revision.default_judge_timeout_secs,
        revision.default_judge_retry_interval_secs,
        req.break_points,
        req.loss_points,
        req.fix_points,
        req.down_points,
        req.first_bonus,
    )
    .await
    .map_err(AppError::from)?;

    // §37：EventGameBox add → configuration_generation += 1
    touch_event_configuration(db, event_id).await?;

    UniResponse::ok(
        EventGameBoxDto {
            id: eg.id,
            gamebox_id: eg.gamebox_id,
            gamebox_name: gb.name,
            gamebox_safe_name: gb.safe_name,
            revision_id: revision.id,
            revision_number: revision.revision_number,
            host_offset: eg.host_offset,
            enabled: eg.enabled,
            hidden: eg.hidden,
            cpu_millis: eg.cpu_millis,
            memory_bytes: eg.memory_bytes,
            pids_limit: eg.pids_limit,
            judge_timeout_secs: eg.judge_timeout_secs,
            judge_retry_interval_secs: eg.judge_retry_interval_secs,
            break_points: eg.break_points,
            loss_points: eg.loss_points,
            fix_points: eg.fix_points,
            down_points: eg.down_points,
            first_bonus: eg.first_bonus,
            created_at: eg.created_at,
        }
        .into(),
    )
    .into()
}

/// PATCH /api/admin/events/{event_id}/awd/gameboxes/{event_gamebox_id}
/// （计分/资源/判题覆盖/pin revision 变更；已部署的 host_offset 禁改 §38；§37 touch_configuration）
#[patch("/events/{event_id}/awd/gameboxes/{event_gamebox_id}")]
pub async fn update_event_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<UpdateEventGameBoxRequest>,
) -> UniResult<EventGameBoxDto> {
    let (_event_id, event_gamebox_id) = path.into_inner();
    let req = body.into_inner();
    let db: &DatabaseConnection = ctx.db.get_ref();

    let eg = event_gamebox_repo::find_event_gamebox_by_id(db, event_gamebox_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("EventGameBox not found".into()))?;

    // revision 变更必须属于同一 GameBox（§11）
    if let Some(rid) = req.revision_id {
        let rev = gamebox_lib_repo::find_revision_by_id(db, rid)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound("Revision not found".into()))?;
        if rev.gamebox_id != eg.gamebox_id {
            return Err(
                AppError::Conflict("revision 不属于该 GameBox（§11 一致性）".into()).into(),
            );
        }
    }

    event_gamebox_repo::update_event_gamebox(
        db,
        event_gamebox_id,
        event_gamebox_repo::EventGameBoxPatch {
            gamebox_revision_id: req.revision_id,
            enabled: req.enabled,
            hidden: req.hidden,
            cpu_millis: req.cpu_millis,
            memory_bytes: req.memory_bytes,
            pids_limit: req.pids_limit,
            healthcheck_override_json: None,
            judge_timeout_secs: req.judge_timeout_secs,
            judge_retry_interval_secs: req.judge_retry_interval_secs,
            break_points: req.break_points,
            loss_points: req.loss_points,
            fix_points: req.fix_points,
            down_points: req.down_points,
            first_bonus: req.first_bonus,
        },
    )
    .await
    .map_err(AppError::from)?;

    touch_event_configuration(db, eg.event_id).await?;

    let d = event_gamebox_repo::find_event_gameboxes_detail(db, eg.event_id)
        .await
        .map_err(AppError::from)?
        .into_iter()
        .find(|d| d.event_gamebox.id == event_gamebox_id)
        .ok_or_else(|| AppError::NotFound("EventGameBox not found".into()))?;
    UniResponse::ok(
        EventGameBoxDto {
            id: d.event_gamebox.id,
            gamebox_id: d.gamebox.id,
            gamebox_name: d.gamebox.name,
            gamebox_safe_name: d.gamebox.safe_name,
            revision_id: d.revision.id,
            revision_number: d.revision.revision_number,
            host_offset: d.event_gamebox.host_offset,
            enabled: d.event_gamebox.enabled,
            hidden: d.event_gamebox.hidden,
            cpu_millis: d.event_gamebox.cpu_millis,
            memory_bytes: d.event_gamebox.memory_bytes,
            pids_limit: d.event_gamebox.pids_limit,
            judge_timeout_secs: d.event_gamebox.judge_timeout_secs,
            judge_retry_interval_secs: d.event_gamebox.judge_retry_interval_secs,
            break_points: d.event_gamebox.break_points,
            loss_points: d.event_gamebox.loss_points,
            fix_points: d.event_gamebox.fix_points,
            down_points: d.event_gamebox.down_points,
            first_bonus: d.event_gamebox.first_bonus,
            created_at: d.event_gamebox.created_at,
        }
        .into(),
    )
    .into()
}

/// DELETE /api/admin/events/{event_id}/awd/gameboxes/{event_gamebox_id}
/// （移除赛事选择；被 Instance 引用时 DB RESTRICT 拒绝；§37 touch_configuration）
#[delete("/events/{event_id}/awd/gameboxes/{event_gamebox_id}")]
pub async fn delete_event_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
) -> UniResult<()> {
    let (event_id, event_gamebox_id) = path.into_inner();
    let db: &DatabaseConnection = ctx.db.get_ref();

    let eg = event_gamebox_repo::find_event_gamebox_by_id(db, event_gamebox_id)
        .await
        .map_err(AppError::from)?;
    let eg = match eg {
        Some(eg) => eg,
        None => return UniResponse::ok_none().into(),
    };
    if eg.event_id != event_id {
        return Err(AppError::NotFound("EventGameBox not found".into()).into());
    }

    // §38：已部署（有 instance）的 EventGameBox 禁止直接移除 —— 视为 destructive
    let instance_count = crate::entity::awd_gamebox_instances::Entity::find()
        .filter(crate::entity::awd_gamebox_instances::Column::EventGameboxId.eq(event_gamebox_id))
        .count(db)
        .await
        .map_err(AppError::from)?;
    if instance_count > 0 {
        return Err(AppError::Conflict(
            "EventGameBox 已有实例，禁止移除（先处理实例或视为 destructive 重配置）".into(),
        )
        .into());
    }

    event_gamebox_repo::delete_event_gamebox(db, event_gamebox_id)
        .await
        .map_err(AppError::from)?;
    touch_event_configuration(db, event_id).await?;
    UniResponse::ok_none().into()
}

// ────────────────────────────────────────────────────────────────────────────
// 辅助
// ────────────────────────────────────────────────────────────────────────────

/// §37：EventGameBox 增删改 → configuration_generation += 1（Verified 失效）
async fn touch_event_configuration(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<(), AppError> {
    event_repo::touch_configuration(db, event_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))
}

/// IP 确定性校验工具（供 Precheck 使用；handler 层不直接算 IP，§64）
#[allow(dead_code)]
pub(crate) fn debug_instance_ip(subnet: &str, host_offset: i16) -> Option<String> {
    instance_ip_for_offset(subnet, host_offset)
}
