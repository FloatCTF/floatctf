//! GameBox 题库与 EventGameBox 管理端处理器。

use std::str::FromStr;

use actix_multipart::form::{MultipartForm, tempfile::TempFile};
use actix_web::web;
use fcmc::{DockerContainerRuntime, ImageRuntime};
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
    infrastructure::settings::{get_setting, resolve_dir_path},
    modules::event::awd::{
        domain::instance_ip_for_offset,
        repo::{event_gamebox_repo, event_repo},
        service::gamebox_service,
    },
    modules::gamebox::{
        self, BUILD_STATUS_READY, GameBoxScanItem, import as gamebox_import,
        library as gamebox_lib_repo,
    },
};

use super::dto::*;

// ────────────────────────────────────────────────────────────────────────────
// GameBox 库（全局单版本 identity）
// ────────────────────────────────────────────────────────────────────────────

/// GET /api/admin/awd/gameboxes
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

#[derive(Debug, MultipartForm)]
struct ImportGameBoxForm {
    #[multipart(limit = "256MB")]
    package_zip: TempFile,
}

/// POST /api/admin/awd/gameboxes/import —— package zip 导入（单版本：同步 build）
#[post("/awd/gameboxes/import")]
pub async fn import_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    MultipartForm(form): MultipartForm<ImportGameBoxForm>,
) -> UniResult<ImportGameBoxResponse> {
    let zip_path = form.package_zip.file.path();
    let result = gamebox_import::import_gamebox_package(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        &ctx.config.registry,
        zip_path,
    )
    .await
    .map_err(AppError::from)?;

    UniResponse::ok(
        ImportGameBoxResponse {
            gamebox: GameBoxLibraryDto::from(&result.gamebox),
        }
        .into(),
    )
    .into()
}

/// POST /api/admin/awd/gameboxes/check —— 检查当前版本镜像本地可用 + package 目录已镜像
#[derive(Debug, Serialize, Deserialize)]
pub struct GameBoxCheckResult {
    pub id: Uuid,
    pub gamebox_name: String,
    pub is_ok: bool,
    pub docker_image: bool,
    pub package_dir: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameBoxCheckRequest {
    pub gamebox_id_list: Option<Vec<Uuid>>,
}

#[post("/awd/gameboxes/check")]
pub async fn check_gameboxes(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    req: web::Json<GameBoxCheckRequest>,
) -> UniResult<Vec<GameBoxCheckResult>> {
    let req = req.into_inner();
    let gamebox_dir = get_setting(&ctx.db, "GAMEBOXES_DIR")
        .await
        .map_err(|e| AppError::BadRequest(format!("get setting error: {}", e)))?;

    let gameboxes = {
        if let Some(ids) = req.gamebox_id_list {
            gameboxes::Entity::find()
                .filter(gameboxes::Column::Id.is_in(ids))
                .all(ctx.db.get_ref())
                .await?
        } else {
            gameboxes::Entity::find().all(ctx.db.get_ref()).await?
        }
    };

    let runtime = DockerContainerRuntime::new(ctx.docker.get_ref().clone());
    let mut results = Vec::new();
    for gb in gameboxes {
        let (docker_ok, dir_ok) = if gb.build_status.as_deref() == Some(BUILD_STATUS_READY) {
            // 镜像检查：当前版本 image pin（RepoDigest > image_id）必须本地可 inspect
            let docker_ok = match gamebox::effective_image_ref_from_gamebox(&gb) {
                Ok(pin) => ImageRuntime::inspect_image(&runtime, &pin).await.is_ok(),
                Err(_) => false,
            };
            // package 目录检查：GAMEBOXES_DIR/{safe_name} 已镜像到磁盘
            let dir_ok = resolve_dir_path(&gamebox_dir).join(&gb.safe_name).is_dir();
            (docker_ok, dir_ok)
        } else {
            (false, true)
        };

        results.push(GameBoxCheckResult {
            id: gb.id,
            gamebox_name: gb.name,
            is_ok: docker_ok && dir_ok,
            docker_image: docker_ok,
            package_dir: dir_ok,
        });
    }

    UniResponse::ok(results.into()).into()
}

/// POST /api/admin/awd/gameboxes/build —— 仅 re-ensure 当前版本 pin 镜像本地存在（pull by RepoDigest when missing）
#[derive(Debug, Serialize, Deserialize)]
pub struct GameBoxBuildRequest {
    pub gamebox_id: Option<Uuid>,
    pub gamebox_id_list: Option<Vec<Uuid>>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct GameBoxBuildResult {
    pub gamebox_name: String,
    pub is_ok: bool,
    pub message: String,
}

#[post("/awd/gameboxes/build")]
pub async fn build_gamebox(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    req: web::Json<GameBoxBuildRequest>,
) -> UniResult<Vec<GameBoxBuildResult>> {
    let user = user.into_inner();
    let req = req.into_inner();
    let mut id_list = Vec::new();
    if let Some(id) = req.gamebox_id {
        id_list.push(id);
    }
    if let Some(l) = req.gamebox_id_list {
        id_list.extend(l);
    }

    let mut res = Vec::new();
    for gb_id in id_list {
        let gb = gameboxes::Entity::find_by_id(gb_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!("gamebox {} not exist", gb_id)))?;

        let (is_ok, message) = if gb.build_status.as_deref() == Some(BUILD_STATUS_READY) {
            match gamebox::effective_image_ref_from_gamebox(&gb) {
                Ok(pin) => {
                    let runtime = DockerContainerRuntime::new(ctx.docker.get_ref().clone());
                    match ImageRuntime::ensure_image(&runtime, &pin, None).await {
                        Ok(_) => (true, "ok".to_string()),
                        Err(e) => (false, e.to_string()),
                    }
                }
                Err(e) => (false, e.to_string()),
            }
        } else {
            (
                false,
                "no ready package; import a package first".to_string(),
            )
        };

        res.push(GameBoxBuildResult {
            gamebox_name: gb.name.clone(),
            is_ok,
            message,
        });

        ctx.log
            .add_log(
                "INFO",
                "GAMEBOXES",
                "BUILD",
                format!("{} 确保 gamebox 镜像: {}", user.username, gb.name).as_str(),
                json!({ "gamebox_name": gb.name, "success": is_ok }),
                None,
                user.id.into(),
                Some(&ctx.req),
            )
            .await;
    }

    UniResponse::ok(res.into()).into()
}

/// POST /api/admin/awd/gameboxes/scan —— 扫描 GAMEBOXES_DIR 登记未入库 package
#[post("/awd/gameboxes/scan")]
pub async fn scan_gameboxes(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
) -> UniResult<Vec<GameBoxScanItem>> {
    let items = gamebox_import::scan_gameboxes_dir(
        ctx.db.get_ref(),
        ctx.docker.get_ref(),
        &ctx.config.registry,
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(items.into()).into()
}

/// PATCH /api/admin/awd/gameboxes/{gamebox_id} —— 身份 + 可编辑运行参数
#[patch("/awd/gameboxes/{gamebox_id}")]
pub async fn update_gamebox(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    path: web::Path<Uuid>,
    body: web::Json<UpdateGameBoxIdentityRequest>,
) -> UniResult<GameBoxLibraryDto> {
    let gamebox_id = path.into_inner();
    let req = body.into_inner();

    // JSON 文本 → Value（非法 JSON 直接 400）
    let healthchecks_json = match req.healthchecks_json {
        Some(Some(t)) => Some(Some(
            serde_json::from_str::<serde_json::Value>(&t).map_err(|e| {
                AppError::Validation(format!("healthchecks_json 不是合法 JSON: {}", e))
            })?,
        )),
        Some(None) => Some(None),
        None => None,
    };
    let judge_args_json = match req.judge_args_json {
        Some(Some(t)) => Some(Some(
            serde_json::from_str::<serde_json::Value>(&t).map_err(|e| {
                AppError::Validation(format!("judge_args_json 不是合法 JSON: {}", e))
            })?,
        )),
        Some(None) => Some(None),
        None => None,
    };

    let gb = gamebox::update_gamebox_identity_checked(
        ctx.db.get_ref(),
        gamebox_id,
        gamebox_lib_repo::GameBoxIdentityPatch {
            name: req.name,
            category: req.category,
            description: req.description,
            hidden: req.hidden,
            username: req.username,
            recommended_cpu_millis: req.recommended_cpu_millis,
            recommended_memory_bytes: req.recommended_memory_bytes,
            recommended_pids_limit: req.recommended_pids_limit,
            healthchecks_json,
            judge_script_name: req.judge_script_name,
            judge_script_content: req.judge_script_content,
            judge_args_json,
            judge_timeout_secs: req.judge_timeout_secs,
            judge_retry_interval_secs: req.judge_retry_interval_secs,
        },
    )
    .await
    .map_err(AppError::from)?;
    UniResponse::ok(GameBoxLibraryDto::from(&gb).into()).into()
}

/// POST /api/admin/awd/gameboxes/{gamebox_id}/hide —— 归档
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
        gamebox_version: d.gamebox.version.clone(),
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

/// POST /api/admin/events/{event_id}/awd/gameboxes —— 选择 GameBox（当前版本）
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

    // 单版本模型：事件引用 GameBox 当前版本（ready 校验）
    let gb = gamebox_lib_repo::find_gamebox_by_id(db, req.gamebox_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("GameBox not found".into()))?;
    if gb.build_status.as_deref() != Some(BUILD_STATUS_READY) {
        return Err(AppError::Validation(format!(
            "GameBox '{}' has no ready package; import a package first (status={:?})",
            gb.name, gb.build_status
        ))
        .into());
    }

    // UNIQUE(event_id, gamebox_id)
    if event_gamebox_repo::find_event_gamebox(db, event_id, gb.id)
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

    // Prefill resources from GameBox recommended_*.
    let eg = event_gamebox_repo::create_event_gamebox(
        db,
        event_id,
        gb.id,
        host_offset,
        true,
        req.hidden,
        gb.recommended_cpu_millis,
        gb.recommended_memory_bytes,
        gb.recommended_pids_limit,
        None,
        gb.judge_timeout_secs,
        gb.judge_retry_interval_secs,
        req.break_points,
        req.loss_points,
        req.fix_points,
        req.down_points,
        req.first_bonus,
    )
    .await
    .map_err(AppError::from)?;

    touch_event_configuration(db, event_id).await?;

    UniResponse::ok(
        EventGameBoxDto {
            id: eg.id,
            gamebox_id: eg.gamebox_id,
            gamebox_name: gb.name,
            gamebox_safe_name: gb.safe_name,
            gamebox_version: gb.version.clone(),
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
    UniResponse::ok(to_event_gamebox_dto(d).into()).into()
}

/// DELETE /api/admin/events/{event_id}/awd/gameboxes/{event_gamebox_id}
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

async fn touch_event_configuration(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<(), AppError> {
    event_repo::touch_configuration(db, event_id)
        .await
        .map_err(|e| AppError::Database(e.to_string()))
}

/// IP 确定性校验工具（供 Precheck 使用）
#[allow(dead_code)]
pub(crate) fn debug_instance_ip(subnet: &str, host_offset: i16) -> Option<String> {
    instance_ip_for_offset(subnet, host_offset)
}
