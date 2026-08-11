//! 武器库 HTTP 处理器。

use actix_multipart::form::MultipartForm;
use actix_web::web::ServiceConfig;
use sea_orm::Condition;

use super::application;
use super::dto::{CreateWeaponRequest, PatchWeaponRequest, WeaponForm};
use crate::api::dto::map_dto_vec;

use crate::modules::weapon::WeaponsDto;
use crate::{
    api::{FilterMapping, dto::DeleteItemsRequest, prelude::*, sea_orm_utils::query_query},
    entity::weapons,
};

/// 注册player weapon routes under an existing `/weapons` scope。
///
/// 最终路径：`/api/weapons`
pub fn configure_player_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_weapons_player);
}

/// 注册admin weapon routes under an existing `/weapons` scope。
///
/// 最终路径：`/api/admin/weapons/**`
pub fn configure_admin_routes(cfg: &mut ServiceConfig) {
    cfg.service(create_weapon)
        .service(delete_weapon)
        .service(patch_weapon)
        .service(get_weapons_admin)
        .service(upload_weapon);
}

/// GET /api/weapons
#[get("")]
pub async fn get_weapons_player(
    _user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<WeaponsDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "name",
            column: Box::new(|v| Condition::all().add(weapons::Column::Name.contains(v))),
        },
        FilterMapping {
            key: "category",
            column: Box::new(|v| Condition::all().add(weapons::Column::Category.contains(v))),
        },
        FilterMapping {
            key: "description",
            column: Box::new(|v| Condition::all().add(weapons::Column::Description.contains(v))),
        },
        FilterMapping {
            key: "has_file",
            column: Box::new(|v| {
                Condition::all()
                    .add(weapons::Column::HasFile.eq(v.parse::<bool>().unwrap_or(false)))
            }),
        },
    ];

    let (items, total_items) = query_query::<weapons::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| {
            stmt.order_by_desc(weapons::Column::UpdatedAt)
        })),
    )
    .await?;

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/admin/weapons
#[get("")]
pub async fn get_weapons_admin(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
) -> UniResult<Vec<WeaponsDto>> {
    let weapons = application::list_all_admin(ctx.db.get_ref()).await?;
    UniResponse::ok(Some(map_dto_vec(weapons))).into()
}

/// POST /api/admin/weapons
#[post("")]
pub async fn create_weapon(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    cwr: Json<CreateWeaponRequest>,
) -> UniResult<WeaponsDto> {
    let user = user.into_inner();
    let cwr = cwr.into_inner();

    let weapon = application::create(ctx.db.get_ref(), cwr).await?;

    ctx.log
        .add_log(
            "INFO",
            "WEAPONS",
            "CREATE",
            format!("{} 创建武器: {}", user.username, weapon.name).as_str(),
            json!({"name": weapon.name, "category": weapon.category}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(weapon.into())).into()
}

/// PATCH /api/admin/weapons/{weapon_id}
#[patch("/{weapon_id}")]
pub async fn patch_weapon(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    weapon_id: Path<Uuid>,
    pwr: Json<PatchWeaponRequest>,
) -> UniResult<WeaponsDto> {
    let user = user.into_inner();
    let weapon_id = weapon_id.into_inner();
    let pwr = pwr.into_inner();

    let weapon = application::patch(ctx.db.get_ref(), weapon_id, pwr).await?;

    ctx.log
        .add_log(
            "INFO",
            "WEAPONS",
            "UPDATE",
            format!("{} 更新武器: {}", user.username, weapon.name).as_str(),
            json!({"weapon_id": weapon.id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(weapon.into())).into()
}

/// DELETE /api/admin/weapons
#[delete("")]
pub async fn delete_weapon(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();

    let deleted_count: u64 = application::delete_many(ctx.db.get_ref(), dir.id_list).await?;

    ctx.log
        .add_log(
            "INFO",
            "WEAPONS",
            "DELETE",
            format!("{} 删除 {} 个武器", user.username, deleted_count).as_str(),
            json!({"deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(deleted_count)).into()
}

/// POST /api/admin/weapons/{weapon_id}/upload
#[post("/{weapon_id}/upload")]
pub async fn upload_weapon(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    weapon_id: Path<Uuid>,
    MultipartForm(form): MultipartForm<WeaponForm>,
) -> UniResult<()> {
    let user = user.into_inner();
    let weapon_id = weapon_id.into_inner();
    let weapon_file = form.weapon;
    let file_name = weapon_file
        .file_name
        .unwrap_or_else(|| weapon_id.to_string());

    application::upload_file(
        ctx.db.get_ref(),
        ctx.rustfs.get_ref(),
        weapon_id,
        weapon_file.file.path(),
        file_name.clone(),
    )
    .await?;

    ctx.log
        .add_log(
            "INFO",
            "WEAPONS",
            "UPLOAD",
            format!("{} 上传武器文件: {}", user.username, file_name).as_str(),
            json!({"weapon_id": weapon_id, "file_name": file_name}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
