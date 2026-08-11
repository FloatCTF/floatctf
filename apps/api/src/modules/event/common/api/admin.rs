//! Admin event HTTP handlers — thin adapters over `modules::event::common::application::admin_service`.

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
    per: Json<PatchEventRequest>,
    event_id: Path<Uuid>,
) -> UniResult<EventsDto> {
    let user = user.into_inner();
    let per = per.into_inner();
    let event_id = event_id.into_inner();

    let event = svc::patch_event(ctx.db.get_ref(), event_id, per).await?;

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
#[delete("")]
pub async fn delete_event(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();
    let deleted_count = svc::delete_events(ctx.db.get_ref(), dir.id_list).await?;

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
