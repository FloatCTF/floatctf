use std::str::FromStr;

use sea_orm::Condition;

use crate::api::dto::map_dto_vec;

use crate::modules::event::common::api::EventLogsDto;
use crate::{
    api::{FilterMapping, apply_filters, prelude::*, sea_orm_utils::paginate_query},
    entity::{
        event_logs,
        sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
    },
};

/// GET /api/admin/events/{event_id}/logs
#[get("")]
pub async fn get_event_logs(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<EventLogsDto>> {
    let event_id = event_id.into_inner();
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_logs::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "user_id",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_logs::Column::UserId.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "team_id",
            column: Box::new(|v| {
                Condition::all()
                    .add(event_logs::Column::TeamId.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "family",
            column: Box::new(|v| {
                Condition::all().add(
                    event_logs::Column::Family
                        .eq(serde_json::from_str(v).unwrap_or(EventFamily::Jeopardy)),
                )
            }),
        },
        FilterMapping {
            key: "purpose",
            column: Box::new(|v| {
                Condition::all().add(
                    event_logs::Column::Purpose
                        .eq(serde_json::from_str(v).unwrap_or(EventPurpose::Competition)),
                )
            }),
        },
        FilterMapping {
            key: "participant_mode",
            column: Box::new(|v| {
                Condition::all().add(
                    event_logs::Column::ParticipantMode
                        .eq(serde_json::from_str(v).unwrap_or(ParticipantMode::Individual)),
                )
            }),
        },
        FilterMapping {
            key: "level",
            column: Box::new(|v| Condition::all().add(event_logs::Column::Level.eq(v.to_string()))),
        },
        FilterMapping {
            key: "action",
            column: Box::new(|v| Condition::all().add(event_logs::Column::Action.contains(v))),
        },
        FilterMapping {
            key: "details",
            column: Box::new(|v| Condition::all().add(event_logs::Column::Details.contains(v))),
        },
    ];

    let stmt = event_logs::Entity::find().filter(event_logs::Column::EventId.eq(event_id));
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(event_logs::Column::CreatedAt);

    let (items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            paginate_query(stmt, ctx.db.get_ref(), limit, page).await?
        } else {
            let items = stmt.all(ctx.db.get_ref()).await?;
            (items.clone(), items.len())
        };

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}
