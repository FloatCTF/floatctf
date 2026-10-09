//! 鉴权相关请求提取器与守卫。
//!
//! # 401 与 403 的契约（v2 固定，SDK 与前端据此分流）
//!
//! | 状态 | 含义 | 客户端应做 |
//! |---|---|---|
//! | **401** | 未认证 / 凭据无效 / 凭据已吊销（`token_version` 不匹配）/ 令牌用途不符 | 清除本地令牌 → 跳登录 |
//! | **403** | 已认证但**不允许**（账号被禁用 / 缺少所需权限 / 对象级授权失败） | 保留令牌 → 提示无权限 |
//!
//! 这个分流很重要：403 不是「重新登录就能解决」，静默跳登录页会让用户以为会话丢了。
//! Wave 7 的 SDK 会把它映射成 `FloatCTFError.forbidden` + `onForbidden` 回调。
//!
//! # 为什么守卫必须查库
//!
//! JWT 只含身份（见 `core::security::jwt`），因此：
//! - 「账号是否被禁用」只能查 `users.status`；
//! - 「凭据是否已吊销」只能比对 `users.token_version`。
//!
//! 两者都是**每请求**校验，所以禁用/改密/吊销**立刻生效**——这是 v2 相对 v1 的关键提升。

use crate::api::AppError;
use crate::core::permission::PermissionSet;
use crate::core::security::jwt::{TokenPurpose, validate_jwt};
use crate::core::system_ids::PLATFORM_ROLE_SUPER_ADMIN;
use crate::entity::sea_orm_active_enums::AccountStatus;
use crate::entity::{platform_roles, user_platform_roles, users};
use crate::infrastructure::WebDb;
use crate::modules::identity::authorization::permissions_of_roles;
use actix_web::FromRequest;
use actix_web::ResponseError;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

/// 认证失败的内部原因（仅用于日志与测试断言，不外泄细节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFailure {
    /// 缺少 / 格式错误的 Authorization 头。
    MissingCredentials,
    /// 令牌签名无效或已过期。
    InvalidToken,
    /// 令牌用途不是会话（例如把重置令牌当会话令牌用）。
    WrongPurpose,
    /// 账号不存在（含已被硬删除）。
    UnknownAccount,
    /// `token_version` 不匹配：凭据已被吊销（改密 / 禁用 / 显式吊销）。
    RevokedCredential,
    /// 账号被禁用。
    AccountDisabled,
}

impl AuthFailure {
    /// 该失败应返回 401 还是 403。
    ///
    /// 只有「已确认身份、但账号本身被禁用」才是 403；其余都是 401
    /// （调用方应重新认证，而不是以为自己只是没权限）。
    pub fn status_hint(self) -> u16 {
        match self {
            AuthFailure::AccountDisabled => 403,
            _ => 401,
        }
    }
}

/// 把认证失败转成平台 JSON 信封响应。
fn auth_error(failure: AuthFailure) -> actix_web::Error {
    // 统一走平台 JSON 信封（Actix 的 ErrorUnauthorized 只会返回纯文本）。
    let app_error = match failure {
        AuthFailure::AccountDisabled => AppError::Forbidden("账号已被禁用".to_string()),
        _ => AppError::Unauthorized,
    };
    actix_web::error::InternalError::from_response("unauthorized", app_error.error_response())
        .into()
}

/// 从请求头解析 Bearer 令牌。
fn bearer_token(req: &actix_web::HttpRequest) -> Option<String> {
    let raw = req.headers().get("Authorization")?.to_str().ok()?;
    let token = raw.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

/// 解析并校验账号身份（会话令牌专用）。
///
/// 校验顺序（顺序本身是安全语义，不要重排）：
/// 1. Bearer 令牌存在且可解析 → 否则 401；
/// 2. 签名/有效期 → 否则 401；
/// 3. **用途必须是 `Session`** → 否则 401（重置令牌不得访问业务端点）；
/// 4. 账号存在 → 否则 401；
/// 5. `token_version` 匹配 → 否则 401（凭据已吊销）；
/// 6. `status == active` → 否则 **403**（账号被禁用）。
pub async fn resolve_session_account<C: ConnectionTrait>(
    db: &C,
    req: &actix_web::HttpRequest,
) -> Result<users::Model, AuthFailure> {
    let token = bearer_token(req).ok_or(AuthFailure::MissingCredentials)?;
    let claims = validate_jwt(token).map_err(|_| AuthFailure::InvalidToken)?;

    // 用途检查必须在读库之前：重置令牌不应该有机会触发任何账号加载。
    if claims.purpose != TokenPurpose::Session {
        return Err(AuthFailure::WrongPurpose);
    }

    let user = users::Entity::find_by_id(claims.sub)
        .one(db)
        .await
        .map_err(|_| AuthFailure::UnknownAccount)?
        .ok_or(AuthFailure::UnknownAccount)?;

    // 版本检查在状态检查之前：吊销比禁用更「底层」。
    if user.token_version != claims.token_version {
        return Err(AuthFailure::RevokedCredential);
    }

    if user.status != AccountStatus::Active {
        return Err(AuthFailure::AccountDisabled);
    }

    Ok(user)
}

/// **统一账户守卫**：任何需要「已登录账号」的端点都应该用它。
///
/// 每次请求都会强制校验 `users.status` 与 `users.token_version`。
pub struct AccountGuard(users::Model);

impl AccountGuard {
    pub fn into_inner(self) -> users::Model {
        self.0
    }

    pub fn id(&self) -> uuid::Uuid {
        self.0.id
    }

    pub fn username(&self) -> &str {
        &self.0.username
    }

    pub fn model(&self) -> &users::Model {
        &self.0
    }
}

impl FromRequest for AccountGuard {
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let db = req.app_data::<WebDb>().cloned();
        let req = req.clone();

        Box::pin(async move {
            let Some(db) = db else {
                return Err(auth_error(AuthFailure::UnknownAccount));
            };
            match resolve_session_account(db.get_ref(), &req).await {
                Ok(user) => Ok(AccountGuard(user)),
                Err(failure) => Err(auth_error(failure)),
            }
        })
    }
}

/// 选手身份守卫（历史名称，语义已并入 [`AccountGuard`]）。
///
/// 保留是因为 ~100 个参赛端 handler 已经在用它返回 `users::Model`；
/// 它现在**委托给同一套校验**，因此禁用/吊销对参赛端同样立刻生效。
pub struct UserJwtGuard(users::Model);

impl UserJwtGuard {
    pub fn into_inner(self) -> users::Model {
        self.0
    }

    pub fn id(&self) -> uuid::Uuid {
        self.0.id
    }

    pub fn model(&self) -> &users::Model {
        &self.0
    }
}

impl FromRequest for UserJwtGuard {
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let db = req.app_data::<WebDb>().cloned();
        let req = req.clone();

        Box::pin(async move {
            let Some(db) = db else {
                return Err(auth_error(AuthFailure::UnknownAccount));
            };
            match resolve_session_account(db.get_ref(), &req).await {
                Ok(user) => Ok(UserJwtGuard(user)),
                Err(failure) => Err(auth_error(failure)),
            }
        })
    }
}

/// 账号是否持有「全权」能力（`grants_all` 角色，即内置 `super-admin`）。
///
/// 这是 **Wave 4 的过渡判据**：v1 里 admin 路由的唯一门槛就是「是 super_admin」，
/// 本函数**精确保留该语义**（而不是放宽）。Wave 5 会用逐路由的
/// `RequirePermission<P>` 取代它——届时每个端点声明自己真正需要的权限，
/// 不再需要「全权」这种粗粒度概念。
pub async fn has_all_access<C: ConnectionTrait>(
    db: &C,
    user_id: uuid::Uuid,
) -> Result<bool, sea_orm::DbErr> {
    let role_ids: Vec<uuid::Uuid> = user_platform_roles::Entity::find()
        .filter(user_platform_roles::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.role_id)
        .collect();

    if role_ids.is_empty() {
        return Ok(false);
    }

    // 固定主键的内置角色优先（不必读权限行）
    if role_ids.contains(&PLATFORM_ROLE_SUPER_ADMIN) {
        return Ok(true);
    }

    let grants_all = platform_roles::Entity::find()
        .filter(platform_roles::Column::Id.is_in(role_ids))
        .filter(platform_roles::Column::GrantsAll.eq(true))
        .one(db)
        .await?;

    Ok(grants_all.is_some())
}

/// 解析账号的**有效权限集合**（多角色并集；`grants_all` 短路为全集）。
pub async fn resolve_permissions<C: ConnectionTrait>(
    db: &C,
    user_id: uuid::Uuid,
) -> Result<PermissionSet, sea_orm::DbErr> {
    let role_ids = crate::modules::identity::authorization::role_ids_of_user(db, user_id).await?;
    let (set, unknown) = permissions_of_roles(db, &role_ids).await?;
    if !unknown.is_empty() {
        // 未知权限字符串不阻止请求（否则一次改名就会锁死平台），但必须可见。
        tracing::warn!(
            user_id = %user_id,
            unknown = ?unknown,
            "账号的有效权限里含无法解析的权限字符串（已忽略；请核对 platform_role_permissions）"
        );
    }
    Ok(set)
}

/// 超管守卫（**过渡期**：Wave 5 由 `RequirePermission<P>` 取代）。
///
/// 与 v1 的差别只有一处，但很关键：v1 从**令牌**里读 `role == SuperAdmin`，
/// v2 从**数据库**里读角色授予。因此撤销超管角色**立刻生效**。
///
/// 返回 `users::Model`（不再是 `super_admin::Model`）——统一账户后管理员就是普通账号。
pub struct SuperAdminJwtGuard(users::Model);

impl SuperAdminJwtGuard {
    pub fn into_inner(self) -> users::Model {
        self.0
    }

    pub fn id(&self) -> uuid::Uuid {
        self.0.id
    }

    pub fn username(&self) -> &str {
        &self.0.username
    }

    pub fn model(&self) -> &users::Model {
        &self.0
    }
}

impl FromRequest for SuperAdminJwtGuard {
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let db = req.app_data::<WebDb>().cloned();
        let req = req.clone();

        Box::pin(async move {
            let Some(db) = db else {
                return Err(auth_error(AuthFailure::UnknownAccount));
            };
            let user = match resolve_session_account(db.get_ref(), &req).await {
                Ok(user) => user,
                Err(failure) => return Err(auth_error(failure)),
            };

            // 身份已确认；现在才判断授权 —— 失败是 403 而不是 401
            // （令牌是有效的，只是这个账号没有管理能力）。
            match has_all_access(db.get_ref(), user.id).await {
                Ok(true) => Ok(SuperAdminJwtGuard(user)),
                Ok(false) => Err(actix_web::error::InternalError::from_response(
                    "forbidden",
                    AppError::Forbidden("需要平台管理权限".to_string()).error_response(),
                )
                .into()),
                Err(e) => {
                    tracing::error!(error = %e, user_id = %user.id, "校验平台管理权限失败");
                    Err(actix_web::error::InternalError::from_response(
                        "internal",
                        AppError::Internal("权限校验失败".to_string()).error_response(),
                    )
                    .into())
                }
            }
        })
    }
}
