//! 用户资料（选手）与管理端用户 CRUD。

pub mod dto;
pub use dto::UsersDto;

use std::str::FromStr;

use crate::api::dto::map_dto_vec;

use crate::api::extractor::auth::{SuperAdminJwtGuard, UserJwtGuard};
use crate::api::prelude::*;
use crate::api::{FilterMapping, dto::DeleteItemsRequest, sea_orm_utils::query_query};
use crate::entity::users;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use sea_orm::Condition;

// ── Player profile ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchMeRequest {
    pub nickname: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
}

/// GET /api/users/me
#[get("/me")]
pub async fn get_me(user: UserJwtGuard) -> UniResult<UsersDto> {
    let mut user = user.into_inner();
    user.password = "".to_string();
    UniResponse::ok(Some(user.into())).into()
}

/// PATCH /api/users/me
#[patch("/me")]
pub async fn patch_me(user: UserJwtGuard, ctx: ReqCtx, pmr: Json<PatchMeRequest>) -> UniResult<()> {
    let pmr = pmr.into_inner();
    let user = user.into_inner();

    let user_id = user.id;
    let username = user.username.clone();
    let mut m_user = user.into_active_model();
    pmr.nickname.map(|n| {
        m_user.nickname = Set(n);
    });
    pmr.email.map(|e| {
        m_user.email = Set(e);
    });

    if let Some(p) = pmr.password {
        let hashed_password = {
            let salt = SaltString::generate(&mut OsRng);
            let argon2 = Argon2::default();

            let password_hash = argon2
                .hash_password(p.as_bytes(), &salt)
                .map_err(|e| AppError::BadRequest(format!("{}", e.to_string())))?
                .to_string();

            password_hash
        };

        m_user.password = Set(hashed_password);
    }

    m_user.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "USER",
            "UPDATE_PROFILE",
            format!("{} 更新个人信息", username).as_str(),
            json!({}),
            user_id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

// ── Admin user CRUD ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    username: String,
    password: String,
    nickname: String,
    email: String,
}

/// POST /api/admin/users
#[post("")]
pub async fn admin_create_user(
    admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    cur: Json<CreateUserRequest>,
) -> UniResult<UsersDto> {
    let admin = admin.into_inner();
    let cur = cur.into_inner();

    let hashed_password = {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2
            .hash_password(cur.password.as_bytes(), &salt)
            .map_err(|e| AppError::BadRequest(format!("{}", e.to_string())))?
            .to_string();

        password_hash
    };

    let new_user = users::ActiveModel {
        username: Set(cur.username),
        password: Set(hashed_password),
        email: Set(cur.email),
        nickname: Set(cur.nickname),
        ..Default::default()
    };

    let user = new_user.insert(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "USERS",
            "CREATE",
            format!("{} 创建用户: {}", admin.username, user.username).as_str(),
            json!({"username": user.username}),
            None,
            admin.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(user.into())).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PathUserRequest {
    username: Option<String>,
    nickname: Option<String>,
    password: Option<String>,
    email: Option<String>,
}

/// PATCH /api/admin/users/{user_id}
#[patch("/{user_id}")]
pub async fn admin_patch_user(
    admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    pur: Json<PathUserRequest>,
    user_id: Path<Uuid>,
) -> UniResult<UsersDto> {
    let admin = admin.into_inner();
    let pur = pur.into_inner();
    let user_id = user_id.into_inner();
    let user = users::Entity::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", user_id)))?;

    let mut m_user = user.into_active_model();

    pur.username.map(|u| {
        m_user.username = Set(u);
    });

    if let Some(p) = pur.password {
        let hashed_password = {
            let salt = SaltString::generate(&mut OsRng);
            let argon2 = Argon2::default();

            let password_hash = argon2
                .hash_password(p.as_bytes(), &salt)
                .map_err(|e| AppError::BadRequest(format!("{}", e.to_string())))?
                .to_string();

            password_hash
        };

        m_user.password = Set(hashed_password);
    }

    pur.email.map(|e| {
        m_user.email = Set(e);
    });

    pur.nickname.map(|n| {
        m_user.nickname = Set(n);
    });
    m_user.updated_at = Set(Utc::now().into());

    let user = m_user.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "USERS",
            "UPDATE",
            format!("{} 更新用户: {}", admin.username, user.username).as_str(),
            json!({"user_id": user.id}),
            None,
            admin.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(user.into())).into()
}

/// GET /api/admin/users
#[get("")]
pub async fn admin_get_users(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<UsersDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(users::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "username",
            column: Box::new(|v| Condition::all().add(users::Column::Username.contains(v))),
        },
        FilterMapping {
            key: "nickname",
            column: Box::new(|v| Condition::all().add(users::Column::Nickname.contains(v))),
        },
        FilterMapping {
            key: "email",
            column: Box::new(|v| Condition::all().add(users::Column::Email.contains(v))),
        },
    ];

    let (items, total_items) = query_query::<users::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| {
            stmt.order_by_desc(users::Column::UpdatedAt)
        })),
    )
    .await?;

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/admin/users/{id}
#[get("/{id}")]
pub async fn admin_get_user(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    user_id: Path<Uuid>,
) -> UniResult<UsersDto> {
    let user_id = user_id.into_inner();
    let model = users::Entity::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", user_id)))?;

    UniResponse::ok(Some(model.into())).into()
}

/// DELETE /api/admin/users
#[delete("")]
pub async fn admin_delete_user(
    admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let admin = admin.into_inner();
    let dir = dir.into_inner();
    let deleted_count = users::Entity::delete_many()
        .filter(users::Column::Id.is_in(dir.id_list))
        .exec(ctx.db.get_ref())
        .await?
        .rows_affected;

    ctx.log
        .add_log(
            "INFO",
            "USERS",
            "DELETE",
            format!("{} 删除 {} 个用户", admin.username, deleted_count).as_str(),
            json!({"deleted_count": deleted_count}),
            None,
            admin.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}
