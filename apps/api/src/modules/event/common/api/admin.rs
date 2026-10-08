//! 管理端赛事 HTTP 处理器——薄适配 `admin_service`。

use actix_web::web;

use crate::api::dto::map_dto_vec;

use crate::modules::event::common::api::EventsDto;
use crate::{
    api::{dto::DeleteItemsRequest, prelude::*, sea_orm_utils::query_query},
    entity::events,
    modules::event::common::application::admin_service::{self as svc},
};

// Re-export DTOs so external imports of admin event types keep working.
pub use crate::modules::event::common::application::admin_service::{
    CreateEventRequest, DataEventChallenge, DataEventChallengeSolve, DataPresent,
    PatchEventRequest, ReportTeam, ReportUser,
};

/// POST /api/admin/events
#[post("")]
pub async fn create_event(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    cer: Json<CreateEventRequest>,
) -> UniResult<EventsDto> {
    let user = user.into_inner();
    let cer = cer.into_inner();
    info!("POST /api/admin/events\nCreate Event Request:{:?}", cer);

    let event = svc::create_event(ctx.db.get_ref(), cer).await?;

    ctx.log
        .add_log(
            "INFO",
            "EVENTS",
            "CREATE",
            format!("{} 创建比赛: {}", user.username, event.title).as_str(),
            json!({"title": event.title, "family": event.family, "purpose": event.purpose, "participant_mode": event.participant_mode}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(event.into())).into()
}

/// PATCH /api/admin/events/{event_id}
#[patch("/{event_id}")]
pub async fn patch_event(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    state: actix_web::web::Data<crate::bootstrap::AppState>,
    per: Json<PatchEventRequest>,
    event_id: Path<Uuid>,
) -> UniResult<EventsDto> {
    let user = user.into_inner();
    let per = per.into_inner();
    let event_id = event_id.into_inner();

    let event = svc::patch_event(ctx.db.get_ref(), event_id, per).await?;

    // AWDP 事件时间/规则修改 → 推 SSE（选手端 eventInfo/进度条即时刷新，
    // 不依赖 15s poll 才看到新 start/end 时间）。
    if event.family == crate::entity::sea_orm_active_enums::EventFamily::Awdp {
        crate::modules::event::awdp::realtime::publish(
            &state,
            event.id,
            "awdp.event_updated",
            serde_json::json!({}),
        );
    }

    ctx.log
        .add_log(
            "INFO",
            "EVENTS",
            "UPDATE",
            format!("{} 更新比赛: {}", user.username, event.title).as_str(),
            json!({"event_id": event.id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(event.into())).into()
}

/// GET /api/admin/events
#[get("")]
pub async fn get_events(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<EventsDto>> {
    let mut query_params = query_params.0;
    let mappings = svc::admin_event_filter_mappings();
    let (items, total_items) = query_query::<events::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| {
            stmt.order_by_desc(events::Column::UpdatedAt)
        })),
    )
    .await?;

    query_params.total = Some(total_items);
    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/admin/events/{event_id}
#[get("/{event_id}")]
pub async fn get_event(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
) -> UniResult<EventsDto> {
    let event_id = event_id.into_inner();
    let event = svc::get_event(ctx.db.get_ref(), event_id).await?;
    UniResponse::ok(Some(event.into())).into()
}

/// DELETE /api/admin/events
///
/// 赛事运行时必须显式拆除：DB 行删除后这些宿主资源无法再被解析，因此**先快照、删除行、
/// 再按快照拆除**。
///
/// 两套运行时都要拆，且**互不相同**：
/// - AWD：容器 / Docker 网络 / WireGuard 接口 / nftables 规则（`awd::archive_service`）；
/// - AWDP：每赛事一个 JudgeServer 容器 + 专属 Docker 网络 + nftables ACL 表 + GameBox 实例容器
///   （`awdp::practice_judge`）。
///
/// P-01：此前这里只有 AWD 分支，删除已部署的 AWDP 赛事会让 judge 容器与专属网络**永久泄漏**
/// （`awdp_event_networks.event_id` 对 `events` 是 ON DELETE CASCADE，行一删名字就再也推导不出来）。
#[delete("")]
pub async fn delete_event(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    awd: web::Data<crate::bootstrap::state::AwdDependencies>,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();

    let mut snapshots = Vec::new();
    for event_id in &dir.id_list {
        match crate::modules::event::awd::service::archive_service::snapshot_event_runtime(
            ctx.db.get_ref(),
            awd.containers.as_ref(),
            *event_id,
        )
        .await
        {
            Ok(Some(snapshot)) => snapshots.push(snapshot),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                "[Delete] snapshot AWD runtime for {} failed: {}",
                event_id,
                error
            ),
        }
    }

    // AWDP 运行时快照（只读）：必须在删除行之前完成，否则 `event_instances` 里的
    // GameBox 实例容器名会随 CASCADE 一起消失，再也无法解析。
    let mut awdp_snapshots = Vec::new();
    for event_id in &dir.id_list {
        match crate::modules::event::awdp::service::practice_judge::snapshot_event_runtime(
            ctx.db.get_ref(),
            *event_id,
        )
        .await
        {
            Ok(Some(snapshot)) => awdp_snapshots.push(snapshot),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                "[Delete] snapshot AWDP runtime for {} failed: {}",
                event_id,
                error
            ),
        }
    }

    let deleted_count = svc::delete_events(ctx.db.get_ref(), dir.id_list).await?;

    for snapshot in &snapshots {
        // 删除失败（例如受保护赛事）时不得拆运行时，否则赛事记录仍在而容器已消失。
        match events::Entity::find_by_id(snapshot.event_id)
            .one(ctx.db.get_ref())
            .await
        {
            Ok(Some(_)) => {
                tracing::warn!(
                    "[Delete] event {} still exists after delete; skipping runtime teardown",
                    snapshot.event_id
                );
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    "[Delete] cannot confirm deletion of {}: {}; skipping runtime teardown",
                    snapshot.event_id,
                    error
                );
                continue;
            }
        }

        if let Err(error) =
            crate::modules::event::awd::service::archive_service::teardown_event_runtime(
                ctx.db.get_ref(),
                awd.containers.as_ref(),
                awd.network.as_ref(),
                awd.firewall.as_ref(),
                snapshot,
            )
            .await
        {
            tracing::warn!(
                "[Delete] teardown AWD runtime for {} failed: {}",
                snapshot.event_id,
                error
            );
        }
    }

    // AWDP 运行时拆除。与 AWD 同一保护语义：**删除失败（例如受保护赛事）时不得拆运行时**，
    // 否则赛事记录仍在而容器已消失。拆除本身幂等且容忍资源已不存在。
    for snapshot in &awdp_snapshots {
        match events::Entity::find_by_id(snapshot.event_id)
            .one(ctx.db.get_ref())
            .await
        {
            Ok(Some(_)) => {
                tracing::warn!(
                    "[Delete] event {} still exists after delete; skipping AWDP runtime teardown",
                    snapshot.event_id
                );
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    "[Delete] cannot confirm deletion of {}: {}; skipping AWDP runtime teardown",
                    snapshot.event_id,
                    error
                );
                continue;
            }
        }

        if let Err(error) =
            crate::modules::event::awdp::service::practice_judge::teardown_event_runtime(
                ctx.docker.get_ref(),
                snapshot,
            )
            .await
        {
            tracing::warn!(
                "[Delete] teardown AWDP runtime for {} failed: {}",
                snapshot.event_id,
                error
            );
        }
    }

    ctx.log
        .add_log(
            "INFO",
            "EVENTS",
            "DELETE",
            format!("{} 删除 {} 场比赛", user.username, deleted_count).as_str(),
            json!({"deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}

/// GET /api/admin/events/{event_id}/data
#[get("/{event_id}/data")]
pub async fn get_data(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
) -> UniResult<DataPresent> {
    let data_present = svc::get_data_present(ctx.db.clone(), *event_id).await?;
    UniResponse::ok(data_present.into()).into()
}

/// GET /api/admin/events/{event_id}/report
#[get("/{event_id}/report")]
pub async fn get_report(
    admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
) -> UniResult<String> {
    let admin = admin.into_inner();
    let event_id = event_id.into_inner();

    let (event, s3_key) = svc::export_writeup_report(&ctx.db, &ctx.rustfs, event_id).await?;

    let message = format!(
        "{} export event {} all wirteup!",
        admin.username, event.title
    );
    info!(message);
    ctx.log
        .add_log(
            "INFO",
            "FILES",
            "EXPORT",
            &message,
            json!([]),
            None,
            admin.id.into(),
            Some(&ctx.req),
        )
        .await;
    UniResponse::ok(s3_key.into()).into()
}
