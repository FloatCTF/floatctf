use std::str::FromStr;

use sea_orm::Condition;

use crate::api::dto::map_dto_vec;

use super::dto::AnnouncementsDto;
use crate::{
    api::{
        FilterMapping, apply_filters, dto::DeleteItemsRequest, prelude::*,
        sea_orm_utils::paginate_query,
    },
    entity::{announcements, super_admin},
};

/// GET /api/announcements (player)
#[get("")]
pub async fn get_player_announcements(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<AnnouncementsDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(announcements::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "title",
            column: Box::new(|v| Condition::all().add(announcements::Column::Title.contains(v))),
        },
        FilterMapping {
            key: "content",
            column: Box::new(|v| Condition::all().add(announcements::Column::Content.contains(v))),
        },
    ];

    let stmt = announcements::Entity::find();
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(announcements::Column::UpdatedAt);

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

/// GET /api/admin/announcements
#[get("")]
pub async fn get_announcements(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<AnnouncementsDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(announcements::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "title",
            column: Box::new(|v| Condition::all().add(announcements::Column::Title.contains(v))),
        },
        FilterMapping {
            key: "content",
            column: Box::new(|v| Condition::all().add(announcements::Column::Content.contains(v))),
        },
    ];

    let stmt = announcements::Entity::find();
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(announcements::Column::UpdatedAt);

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

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAnnouncementRequest {
    pub title: String,
    pub content: Option<String>,
}

/// POST /api/admin/announcements
#[post("")]
pub async fn create_announcement(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    car: Json<CreateAnnouncementRequest>,
) -> UniResult<AnnouncementsDto> {
    let car = car.into_inner();
    let user = user.into_inner();
    let publisher_id = user.id;
    let admin_username = user.username.clone();

    // Lookup superadmin username by publisher_id
    let publisher = super_admin::Entity::find_by_id(publisher_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "SuperAdmin {} not exist",
            publisher_id
        )))?
        .username;

    let announcement = announcements::ActiveModel {
        title: Set(car.title),
        content: Set(car.content),
        publisher_id: Set(publisher_id),
        publisher: Set(publisher),
        ..Default::default()
    };
    let announcement = announcement.insert(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "ANNOUNCEMENTS",
            "CREATE",
            format!("{} 创建公告: {}", admin_username, announcement.title).as_str(),
            json!({"title": announcement.title}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(announcement.into())).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchAnnouncementRequest {
    pub title: Option<String>,
    pub content: Option<Option<String>>,
}

/// PATCH /api/admin/announcements/{announcement_id}
#[patch("/{announcement_id}")]
pub async fn patch_announcement(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    announcement_id: Path<Uuid>,
    par: Json<PatchAnnouncementRequest>,
) -> UniResult<AnnouncementsDto> {
    let user = user.into_inner();
    let announcement_id = announcement_id.into_inner();
    let par = par.into_inner();
    let announcement = announcements::Entity::find_by_id(announcement_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            " {} not exist",
            announcement_id
        )))?;

    let mut m_announcement = announcement.into_active_model();

    par.title.map(|t| {
        m_announcement.title = Set(t);
    });
    par.content.map(|c| {
        m_announcement.content = Set(c);
    });
    let announcement = m_announcement.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "ANNOUNCEMENTS",
            "UPDATE",
            format!("{} 更新公告: {}", user.username, announcement.title).as_str(),
            json!({"announcement_id": announcement.id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(announcement.into())).into()
}

/// DELETE /api/admin/announcements
#[delete("")]
pub async fn delete_announcement(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();
    let mut deleted_count = 0;
    for announcement_id in dir.id_list {
        let announcement = announcements::Entity::find_by_id(announcement_id)
            .one(ctx.db.get_ref())
            .await?;
        if let Some(announcement) = announcement {
            let r = announcement.delete(ctx.db.get_ref()).await?;
            deleted_count += r.rows_affected;
        }
    }

    ctx.log
        .add_log(
            "INFO",
            "ANNOUNCEMENTS",
            "DELETE",
            format!("{} 删除 {} 条公告", user.username, deleted_count).as_str(),
            json!({"deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}
