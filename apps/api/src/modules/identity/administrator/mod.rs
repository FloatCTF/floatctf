//! 管理员账户管理。

pub mod dto;
pub use dto::SuperAdminDto;

use crate::api::dto::DeleteItemsRequest;
use crate::api::dto::map_dto_vec;

use crate::api::extractor::auth::{SuperAdminJwtGuard, has_all_access};
use crate::api::prelude::*;
use crate::core::security::jwt::gen_session_token;
use crate::core::system_ids::PLATFORM_ROLE_SUPER_ADMIN;
use crate::entity::sea_orm_active_enums::AccountStatus;
use crate::entity::{prelude::Users, super_admin, user_platform_roles, users};
use argon2::{
    Argon2, PasswordHash, PasswordVerifier,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use sea_orm::{ConnectionTrait, TransactionTrait};

// ── Login (registered under /api, path /admin/session) ──────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct SuperAdminLoginRequest {
    username: String,
    password: String,
}

/// POST `/api/admin/session`（**过渡期别名**，Wave 10 起由 `/api/session` 取代）。
///
/// 统一账户后管理员不再是独立账号类型，因此认证走**同一张 `users` 表**
/// （口令哈希由 M3 逐字节迁移，二者一致）。
///
/// 与 `/api/session` 的唯一差别：这里是**管理端**入口，因此额外要求账号确有平台管理能力。
/// v1 的语义是「`super_admin` 表里有这个人」，v2 精确对应「持有全权（`grants_all`）角色」。
/// 若不保留这个要求，`/api/admin/session` 就退化成 `/api/session` 的复制品，
/// 且会让「管理入口」丧失它本来的语义。
#[post("/admin/session")]
pub async fn super_admin_login(
    ctx: ReqCtx,
    slr: Json<SuperAdminLoginRequest>,
) -> UniResult<String> {
    let slr = slr.into_inner();

    let user = Users::find()
        .filter(users::Column::Username.eq(slr.username))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::Unauthorized)?;

    let verified = {
        let parsed_hash = PasswordHash::new(&user.password)
            .map_err(|e| AppError::Internal(format!("Failed to new the PasswordHash: {e}")))?;
        Argon2::default()
            .verify_password(slr.password.as_bytes(), &parsed_hash)
            .is_ok()
    };

    if !verified {
        ctx.log
            .add_log(
                "ERROR",
                "AUTH",
                "LOGIN",
                format!("管理员 {} 登陆失败", user.username).as_str(),
                json!([]),
                user.id.into(),
                Some(&ctx.req),
            )
            .await;
        return AppError::Unauthorized.into();
    }

    // 凭据正确但账号被禁用 → 403（而不是 401）
    if user.status != AccountStatus::Active {
        ctx.log
            .add_log(
                "ERROR",
                "AUTH",
                "LOGIN_DISABLED",
                format!("管理员 {} 账号已禁用，拒绝登录", user.username).as_str(),
                json!([]),
                user.id.into(),
                Some(&ctx.req),
            )
            .await;
        return AppError::Forbidden("账号已被禁用".to_string()).into();
    }

    // 已认证但不是管理员 → 403（令牌有效，只是这个账号没有管理能力）
    if !has_all_access(ctx.db.get_ref(), user.id).await? {
        ctx.log
            .add_log(
                "ERROR",
                "AUTH",
                "LOGIN_FORBIDDEN",
                format!("{} 无平台管理权限，拒绝管理端登录", user.username).as_str(),
                json!([]),
                user.id.into(),
                Some(&ctx.req),
            )
            .await;
        return AppError::Forbidden("需要平台管理权限".to_string()).into();
    }

    ctx.log
        .add_log(
            "INFO",
            "AUTH",
            "LOGIN",
            format!("管理员 {} 登陆成功", user.username).as_str(),
            json!([]),
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    let jwt = gen_session_token(user.id, user.token_version)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    UniResponse::ok(jwt.into()).into()
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

    // 统一账户过渡期：身份归 `users`，`super_admin` 表在 Wave 10 前只是管理端 CRUD 的对象。
    // 因此新建管理员必须**同时**写入两张表并授予内置 super-admin 角色，
    // 否则这个管理员能出现在列表里却**无法登录**（Wave 4 引入的真实回归）。
    // 三步在同一事务内完成，避免出现「users 有、super_admin 没有」的半成品账号。
    let txn = ctx.db.get_ref().begin().await?;

    let id = Uuid::new_v4();
    let nickname = nickname_for(&txn, &csr.username, id).await?;

    users::Entity::insert(users::ActiveModel {
        id: Set(id),
        username: Set(csr.username.clone()),
        nickname: Set(nickname),
        password: Set(hashed_password.clone()),
        email: Set(csr.email.clone()),
        avatar: Set(None),
        status: Set(AccountStatus::Active),
        token_version: Set(0),
        ..Default::default()
    })
    .exec(&txn)
    .await?;

    let new_super_admin = super_admin::ActiveModel {
        id: Set(id),
        username: Set(csr.username),
        password: Set(hashed_password),
        email: Set(csr.email),
        ..Default::default()
    };

    let super_admin = new_super_admin.insert(&txn).await?;

    user_platform_roles::Entity::insert(user_platform_roles::ActiveModel {
        user_id: Set(id),
        role_id: Set(PLATFORM_ROLE_SUPER_ADMIN),
        granted_by: Set(Some(user.id)),
        ..Default::default()
    })
    .exec(&txn)
    .await?;

    txn.commit().await?;

    ctx.log
        .add_log(
            "INFO",
            "SUPER_ADMIN",
            "CREATE",
            format!("{} 创建管理员: {}", user.username, super_admin.username).as_str(),
            json!({"username": super_admin.username}),
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
        .ok_or_else(|| AppError::NotFound(format!("{} 不存在", super_user_id)))?;

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

    let password_changed = m_super_admin.password.is_set();
    let super_admin = m_super_admin.update(ctx.db.get_ref()).await?;

    // 镜像到统一身份表：不镜像会让改名/改密的管理员**用新凭据登录失败**。
    let txn = ctx.db.get_ref().begin().await?;
    let mirror = Users::find_by_id(super_user_id)
        .one(&txn)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "统一账户 {} 不存在（super_admin 与 users 不一致，请人工核对）",
                super_user_id
            ))
        })?;
    let next_version = if password_changed {
        // 改密即吊销：所有已签发会话令牌立即失效
        mirror.token_version.saturating_add(1)
    } else {
        mirror.token_version
    };
    let mut m_mirror = mirror.into_active_model();
    m_mirror.username = Set(super_admin.username.clone());
    m_mirror.email = Set(super_admin.email.clone());
    m_mirror.password = Set(super_admin.password.clone());
    m_mirror.token_version = Set(next_version);
    m_mirror.update(&txn).await?;
    txn.commit().await?;

    ctx.log
        .add_log(
            "INFO",
            "SUPER_ADMIN",
            "UPDATE",
            format!("{} 更新管理员: {}", user.username, super_admin.username).as_str(),
            json!({"username": super_admin.username}),
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
        .ok_or_else(|| AppError::NotFound(format!("{} 不存在", super_user_id)))?;

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

    let txn = ctx.db.get_ref().begin().await?;

    let r = super_admin::Entity::delete_many()
        .filter(super_admin::Column::Id.is_in(dir.id_list.clone()))
        .exec(&txn)
        .await?;

    // 必须同时删除统一身份行：否则被「删除」的管理员仍持有有效凭据并**仍能登录**。
    // 删 users 会级联清掉 user_platform_roles（ON DELETE CASCADE），
    // 并把各审计列的 actor 置空（ON DELETE SET NULL）。
    users::Entity::delete_many()
        .filter(users::Column::Id.is_in(dir.id_list))
        .exec(&txn)
        .await?;

    txn.commit().await?;

    ctx.log
        .add_log(
            "INFO",
            "SUPER_ADMIN",
            "DELETE",
            format!("{} 删除 {} 个管理员", user.username, r.rows_affected).as_str(),
            json!({"deleted_count": r.rows_affected}),
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(r.rows_affected.into()).into()
}

/// `users.nickname` 唯一，而管理员没有昵称 —— 用用户名合成。
///
/// 若与既有昵称冲突（玩家可能恰好叫这个名字），追加基于 ID 的确定性短后缀。
/// **不**自动改动既有玩家的昵称：改别人的展示名不是建号流程该做的事。
async fn nickname_for<C: ConnectionTrait>(
    db: &C,
    username: &str,
    id: Uuid,
) -> Result<String, AppError> {
    let base = username.trim().to_string();
    let taken = Users::find()
        .filter(users::Column::Nickname.eq(base.clone()))
        .one(db)
        .await?
        .is_some();
    if !taken {
        return Ok(base);
    }
    let suffix = &id.to_string()[..8];
    Ok(format!("{base}#{suffix}"))
}
