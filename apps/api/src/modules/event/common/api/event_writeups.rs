use std::str::FromStr;

use sea_orm::Condition;

use crate::api::dto::map_dto_vec;

use crate::modules::event::common::api::EventWriteupDto;
use crate::{
    api::{FilterMapping, apply_filters, prelude::*, sea_orm_utils::paginate_query},
    entity::event_writeup,
};

/// GET /api/admin/events/{event_id}/writeups
#[get("")]
pub async fn get_all_event_writeups(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<EventWriteupDto>> {
    let event_id = event_id.into_inner();
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "user_id",
            column: Box::new(|v| {
                Condition::all().add(
                    event_writeup::Column::UserId.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
        FilterMapping {
            key: "team_id",
            column: Box::new(|v| {
                Condition::all().add(
                    event_writeup::Column::TeamId.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
        FilterMapping {
            key: "file_url",
            column: Box::new(|v| Condition::all().add(event_writeup::Column::FileUrl.contains(v))),
        },
    ];

    let stmt = event_writeup::Entity::find().filter(event_writeup::Column::EventId.eq(event_id));
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(event_writeup::Column::CreatedAt);

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
