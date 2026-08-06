//! Super-admin login and admin CRUD for administrators.

pub mod dto;
pub use dto::SuperAdminDto;


use crate::api::dto::DeleteItemsRequest;
use crate::api::dto::map_dto_vec;

use crate::api::extractor::auth::SuperAdminJwtGuard;
use crate::api::prelude::*;
use crate::core::security::jwt::{Role, gen_jwt_token};
use crate::entity::{prelude::SuperAdmin, super_admin};
use argon2::{
    Argon2, PasswordHash, PasswordVerifier,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};

// ── Login (registered under /api, path /admin/session) ──────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct SuperAdminLoginRequest {
    username: String,
    password: String,
}

/// POST /api/admin/session
#[post("/admin/session")]
pub async fn super_admin_login(
    ctx: ReqCtx,
    slr: Json<SuperAdminLoginRequest>,
) -> UniResult<String> {
    let slr = slr.into_inner();

    match SuperAdmin::find()
        .filter(super_admin::Column::Username.eq(slr.username))
        .one(ctx.db.get_ref())
        .await?
    {
        Some(super_admin) => {
            let verified = {
                let parsed_hash = PasswordHash::new(&super_admin.password).map_err(|e| {
                    AppError::Internal(format!("Failed to new the PasswordHash: {e}"))
                })?;
                Argon2::default()
                    .verify_password(slr.password.as_bytes(), &parsed_hash)
                    .is_ok()
            };

            if verified {
                ctx.log
                    .add_log(
                        "INFO",
                        "AUTH",
                        "LOGIN",
                        format!("管理员 {} 登陆成功", super_admin.username).as_str(),
                        json!([]),
                        None,
                        super_admin.id.into(),
                        Some(&ctx.req),
                    )
                    .await;
                let jwt = gen_jwt_token(super_admin.id, Role::SuperAdmin, None)
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;

                UniResponse::ok(jwt.into()).into()
            } else {
                ctx.log
                    .add_log(
                        "ERROR",
                        "AUTH",
                        "LOGIN",
                        format!("管理员 {} 登陆失败", super_admin.username).as_str(),
                        json!([]),
                        None,
                        super_admin.id.into(),
                        Some(&ctx.req),
                    )
                    .await;
                AppError::Unauthorized.into()
            }
        }
        None => AppError::Unauthorized.into(),
    }
}

// ── Super-admin CRUD (under /api/admin/super_admin) ─────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSuperAdminRequest {
    username: String,
    password: String,
    email: String,
}

/// POST /api/admin/super_admin
#[post("")]
pub async fn create_super_admin(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    csr: Json<CreateSuperAdminRequest>,
) -> UniResult<SuperAdminDto> {
    let user = user.into_inner();
    let csr = csr.into_inner();

    let hashed_password = {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2
            .hash_password(csr.password.as_bytes(), &salt)
            .map_err(|e| AppError::BadRequest(format!("{}", e.to_string())))?
            .to_string();

        password_hash
    };

    let new_super_admin = super_admin::ActiveModel {
        username: Set(csr.username),
        password: Set(hashed_password),
        email: Set(csr.email),
        ..Default::default()
    };

    let super_admin = new_super_admin.insert(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "SUPER_ADMIN",
            "CREATE",
            format!("{} 创建管理员: {}", user.username, super_admin.username).as_str(),
            json!({"username": super_admin.username}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(super_admin.into())).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchSuperAdminRequest {
    username: Option<String>,
    password: Option<String>,
    email: Option<String>,
}

/// POST /api/admin/super_admin/{super_user_id}
#[post("/{super_user_id}")]
pub async fn patch_super_admin(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    psr: Json<PatchSuperAdminRequest>,
    super_user_id: Path<Uuid>,
) -> UniResult<SuperAdminDto> {
    let user = user.into_inner();
    let psr = psr.into_inner();
    let super_user_id = super_user_id.into_inner();

    let super_admin = super_admin::Entity::find_by_id(super_user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("{} not exist", super_user_id)))?;

    let mut m_super_admin = super_admin.into_active_model();

    psr.username.map(|u| {
        m_super_admin.username = Set(u);
    });

    if let Some(p) = psr.password {
        let hashed_password = {
            let salt = SaltString::generate(&mut OsRng);
            let argon2 = Argon2::default();

            let password_hash = argon2
                .hash_password(p.as_bytes(), &salt)
                .map_err(|e| AppError::BadRequest(format!("{}", e.to_string())))?
                .to_string();

            password_hash
        };
        m_super_admin.password = Set(hashed_password);
    }

    psr.email.map(|e| {
        m_super_admin.email = Set(e);
    });

    let super_admin = m_super_admin.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "SUPER_ADMIN",
            "UPDATE",
            format!("{} 更新管理员: {}", user.username, super_admin.username).as_str(),
            json!({"username": super_admin.username}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(super_admin.into())).into()
}

/// GET /api/admin/super_admin
#[get("")]
pub async fn get_super_admins(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<SuperAdminDto>> {
    let mut query_params = query_params.0;

    let stmt = super_admin::Entity::find().order_by_desc(super_admin::Column::UpdatedAt);

    if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
        let paginator = stmt.paginate(ctx.db.get_ref(), limit);
        let items = paginator.fetch_page(page.saturating_sub(1)).await?;
        query_params.total = Some(paginator.num_items().await? as usize);

        UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
    } else {
        let items = stmt.all(ctx.db.get_ref()).await?;
        query_params.total = Some(items.len());

        UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
    }
}

/// GET /api/admin/super_admin/{super_user_id}
#[get("/{super_user_id}")]
pub async fn get_super_admin(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    super_user_id: Path<Uuid>,
) -> UniResult<SuperAdminDto> {
    let super_user_id = super_user_id.into_inner();
    let model = super_admin::Entity::find_by_id(super_user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("{} not exist", super_user_id)))?;

    UniResponse::ok(Some(model.into())).into()
}

/// DELETE /api/admin/super_admin
#[delete("")]
pub async fn delete_super_admin(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();

    let r = super_admin::Entity::delete_many()
        .filter(super_admin::Column::Id.is_in(dir.id_list))
        .exec(ctx.db.get_ref())
        .await?;

    ctx.log
        .add_log(
            "INFO",
            "SUPER_ADMIN",
            "DELETE",
            format!("{} 删除 {} 个管理员", user.username, r.rows_affected).as_str(),
            json!({"deleted_count": r.rows_affected}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(r.rows_affected.into()).into()
}
