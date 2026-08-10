//! GameBox library + EventGameBox admin handlers（§46 术语统一）。
//!
//! - `/api/admin/awd/gameboxes`：全局 GameBox 库（identity + immutable Revision）
//! - `/api/admin/events/{event_id}/awd/gameboxes`：赛事选择（pin revision + 计分/资源配置）
//!
//! Handler 只做 parse/auth/service 调用（§64），复杂逻辑一律在 service/repo。

use std::str::FromStr;

use actix_web::web;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    sea_query::Query,
};
use uuid::Uuid;

use crate::{
    api::{
        AppError, FilterMapping, UniResponse, UniResult, extractor::auth::SuperAdminJwtGuard,
        prelude::*, sea_orm_utils::query_query,
    },
    entity::{awd_event_gameboxes, gameboxes},
    modules::event::awd_team::{
        domain::instance_ip_for_offset,
        repo::{event_gamebox_repo, event_repo, gamebox_lib_repo},
        service::gamebox_service,
    },
};

use super::dto::*;

// ────────────────────────────────────────────────────────────────────────────
// GameBox 库（全局身份 + 单版本配置）
// ────────────────────────────────────────────────────────────────────────────

/// GET /api/admin/awd/gameboxes
/// 支持 Challenges 同款搜索/分页：`?page=&limit=&filter=name:xxx&category:yyy`。
#[get("/awd/gameboxes")]
pub async fn list_gamebox_library(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: web::Query<QueryParams>,
) -> UniResult<Vec<GameBoxLibraryDto>> {
    let mut query_params = query_params.0;
    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(gameboxes::Column::Id.eq(Uuid::from_str(v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "name",
            column: Box::new(|v| Condition::all().add(gameboxes::Column::Name.contains(v))),
        },
        FilterMapping {
            key: "safe_name",
            column: Box::new(|v| Condition::all().add(gameboxes::Column::SafeName.contains(v))),
        },
        FilterMapping {
            key: "category",
            column: Box::new(|v| Condition::all().add(gameboxes::Column::Category.contains(v))),
        },
        FilterMapping {
            key: "hidden",
            column: Box::new(|v| {
                Condition::all()
                    .add(gameboxes::Column::Hidden.eq(v.parse::<bool>().unwrap_or(true)))
            }),
        },
    ];
    let (items, total_items) = query_query::<gameboxes::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| stmt.order_by_asc(gameboxes::Column::Name))),
    )
    .await?;

    let dto: Vec<GameBoxLibraryDto> = items.iter().map(GameBoxLibraryDto::from).collect();
    query_params.total = Some(total_items);
    UniResponse::ok_meta(Some(dto), query_params.into()).into()
}

/// POST /api/admin/awd/gameboxes —— 创建 GameBox（身份 + 单版本配置一次写入）
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
    let gb = gamebox_service::create_gamebox(
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
    UniResponse::ok(GameBoxLibraryDto::from(&gb).into()).into()
}

/// PATCH /api/admin/awd/gameboxes/{gamebox_id} —— 编辑（原地覆盖单版本配置，同 Challenges）
#[patch("/awd/gameboxes/{gamebox_id}")]
pub async fn update_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: web::Json<UpdateGameBoxRequest>,
) -> UniResult<GameBoxLibraryDto> {
    let gamebox_id = path.into_inner();
    let req = body.into_inner();
    let gb =
        gamebox_service::update_gamebox(ctx.db.get_ref(), gamebox_id, req.config.into_config())
            .await
            .map_err(AppError::from)?;
    UniResponse::ok(GameBoxLibraryDto::from(&gb).into()).into()
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
/// 支持搜索（gamebox_name/gamebox_safe_name）+ 分页，同 event_challenges。
#[get("{event_id}/awd/gameboxes")]
pub async fn list_event_gameboxes(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    query_params: web::Query<QueryParams>,
) -> UniResult<Vec<EventGameBoxDto>> {
    let event_id = path.into_inner();
    let mut query_params = query_params.0;
    let mappings = [
        FilterMapping {
            key: "gamebox_name",
            column: Box::new(|v| {
                Condition::all().add(
                    awd_event_gameboxes::Column::GameboxId.in_subquery(
                        Query::select()
                            .column(gameboxes::Column::Id)
                            .from(gameboxes::Entity)
                            .and_where(gameboxes::Column::Name.contains(v))
                            .to_owned(),
                    ),
                )
            }),
        },
        FilterMapping {
            key: "gamebox_safe_name",
            column: Box::new(|v| {
                Condition::all().add(
                    awd_event_gameboxes::Column::GameboxId.in_subquery(
                        Query::select()
                            .column(gameboxes::Column::Id)
                            .from(gameboxes::Entity)
                            .and_where(gameboxes::Column::SafeName.contains(v))
                            .to_owned(),
                    ),
                )
            }),
        },
    ];
    let (items, total_items) = query_query::<awd_event_gameboxes::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(move |stmt| {
            stmt.filter(awd_event_gameboxes::Column::EventId.eq(event_id))
                .order_by_asc(awd_event_gameboxes::Column::HostOffset)
        })),
    )
    .await?;

    let mut dto = Vec::with_capacity(items.len());
    for eg in items {
        let gamebox =
            match event_gamebox_repo::find_gamebox_identity(ctx.db.get_ref(), eg.gamebox_id)
                .await
                .map_err(AppError::from)?
            {
                Some(g) => g,
                None => continue,
            };
        dto.push(to_event_gamebox_dto(
            event_gamebox_repo::EventGameBoxDetail {
                event_gamebox: eg,
                gamebox,
            },
        ));
    }
    query_params.total = Some(total_items);
    UniResponse::ok_meta(Some(dto), query_params.into()).into()
}

fn to_event_gamebox_dto(d: event_gamebox_repo::EventGameBoxDetail) -> EventGameBoxDto {
    EventGameBoxDto {
        id: d.event_gamebox.id,
        gamebox_id: d.gamebox.id,
        gamebox_name: d.gamebox.name,
        gamebox_safe_name: d.gamebox.safe_name,
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
}

/// POST /api/admin/events/{event_id}/awd/gameboxes —— 赛事选择 GameBox
/// （单版本：复制 GameBox 当前配置作为赛事默认值，可再覆盖；host_offset 缺省自动分配；
///   完成后 touch_configuration §37）
#[post("{event_id}/awd/gameboxes")]
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
    if gb.image_ref.is_none() {
        return Err(AppError::Validation(
            "GameBox 尚未配置（image_ref 为空），请先在 GameBox 库编辑".into(),
        )
        .into());
    }

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

    // 复制 GameBox 当前配置作为赛事默认值（单版本语义，同 Challenges 选择时固化副本）
    let eg = event_gamebox_repo::create_event_gamebox(
        db,
        event_id,
        req.gamebox_id,
        host_offset,
        true,
        req.hidden,
        gb.default_cpu_millis.unwrap_or(1000),
        gb.default_memory_bytes.unwrap_or(512 * 1024 * 1024),
        gb.default_pids_limit.unwrap_or(100),
        None,
        gb.default_judge_timeout_secs,
        gb.default_judge_retry_interval_secs,
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
/// （计分/资源/判题覆盖；已部署的 host_offset 禁改 §38；§37 touch_configuration）
#[patch("{event_id}/awd/gameboxes/{event_gamebox_id}")]
pub async fn update_event_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<UpdateEventGameBoxRequest>,
) -> UniResult<EventGameBoxDto> {
    let (event_id, event_gamebox_id) = path.into_inner();
    let req = body.into_inner();
    let db: &DatabaseConnection = ctx.db.get_ref();

    event_gamebox_repo::update_event_gamebox(
        db,
        event_gamebox_id,
        event_gamebox_repo::EventGameBoxPatch {
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

    touch_event_configuration(db, event_id).await?;

    let d = event_gamebox_repo::find_event_gameboxes_detail(db, event_id)
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
#[delete("{event_id}/awd/gameboxes/{event_gamebox_id}")]
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
