//! 平台登录 ↔ OIDC SSO 会话的衔接（`/api/users/sso`，旧版接口，保持兼容）。
//!
//! - 平台登录 `POST /api/users/session` 成功后调用 [`establish_after_password_login`] 下发 SSO Cookie；
//! - `POST /api/users/sso`：本功能上线前已持有平台 JWT 的 SPA 会话，用 Bearer JWT 换取 SSO Cookie（幂等）；
//! - `DELETE /api/users/sso`：SPA 登出时撤销服务端 SSO 会话（不依赖 localStorage 清理）。
//!
//! `[oidc].multi_account = true` 时这些接口改为操作浏览器账号容器（见 `oidc::accounts`）；
//! 新版主站使用 `/api/users/web-session`（`api::web_session`）。

use actix_web::{HttpRequest, HttpResponse, cookie::Cookie, delete, http::header, post};
use chrono::{DateTime, FixedOffset, TimeZone, Utc};

use super::protocol::{removal_cookie, sso_cookie, sso_cookie_name, sso_cookie_value};
use crate::api::{AppError, UniResponse, extractor::auth::UserJwtGuard};
use crate::core::security::jwt::validate_jwt;
use crate::entity::users;
use crate::infrastructure::WebDb;
use crate::modules::identity::oidc::{OidcProvider, WebOidc, accounts, store};

/// 平台 JWT 默认有效期（`gen_jwt_token` 缺省 8h），用于推算旧 JWT（无 `iat`）的签发时间。
const PLATFORM_JWT_DEFAULT_TTL_SECS: i64 = 8 * 3600;

/// 平台口令登录成功后建立 SSO 会话，返回要下发的 Cookie。
///
/// 多账号模式下只把账号加入本浏览器容器，不改变主站活跃账号（旧接口只发 JWT，主站状态由 JWT 决定）。
pub async fn establish_after_password_login(
    db: &sea_orm::DatabaseConnection,
    oidc: &OidcProvider,
    req: &HttpRequest,
    user: &users::Model,
) -> Result<Vec<Cookie<'static>>, AppError> {
    if oidc.config.multi_account {
        let mut browser = accounts::load(db, oidc, req).await?;
        accounts::add_account(
            db,
            oidc,
            &mut browser,
            user,
            Utc::now().fixed_offset(),
            oidc.config.session_ttl_secs,
            store::SessionOrigin::PasswordLogin,
        )
        .await?;
        return Ok(browser.cookies);
    }
    // 同一浏览器的旧会话（可能是另一个账号）先撤销，避免会话串号。
    if let Some(old) = sso_cookie_value(req, oidc) {
        store::revoke_session(db, &old, "replaced").await?;
    }
    let (value, _) = store::create_session(
        db,
        user.id,
        Utc::now().fixed_offset(),
        oidc.config.session_ttl_secs,
        store::SessionOrigin::PasswordLogin,
    )
    .await?;
    Ok(vec![sso_cookie(oidc, value)])
}

/// 由 Bearer 平台 JWT 推出桥接会话的认证时间与有效期：
/// `auth_time` 取 JWT 签发时间（桥接不是一次新的认证），会话不比 JWT 活得更久。
pub(crate) fn bridge_window(
    req: &HttpRequest,
    oidc: &OidcProvider,
) -> Result<(DateTime<FixedOffset>, i64), AppError> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or(AppError::Unauthorized)?;
    let claims = validate_jwt(token.to_string()).map_err(|_| AppError::Unauthorized)?;
    let now = Utc::now().timestamp();
    let exp = claims.exp as i64;
    let issued_at = claims
        .iat
        .map(|iat| iat as i64)
        .unwrap_or(exp - PLATFORM_JWT_DEFAULT_TTL_SECS)
        .min(now);
    let auth_time = Utc
        .timestamp_opt(issued_at, 0)
        .single()
        .ok_or(AppError::Unauthorized)?
        .fixed_offset();
    let ttl = oidc.config.session_ttl_secs.min(exp - now);
    if ttl <= 0 {
        return Err(AppError::Unauthorized);
    }
    Ok((auth_time, ttl))
}

#[derive(serde::Serialize)]
pub struct SsoBridgeResult {
    /// 本次是否新建了会话（false = 已有同用户的有效会话，幂等返回）。
    pub established: bool,
}

/// POST /api/users/sso —— 用平台 JWT 建立 SSO 会话（幂等）。
#[post("/sso")]
pub async fn bridge(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
) -> Result<HttpResponse, AppError> {
    let Some(oidc) = oidc else {
        return Err(AppError::NotFound("OIDC 未启用".into()));
    };
    let user = user.into_inner();
    let unchanged = || {
        HttpResponse::Ok().json(UniResponse::ok(Some(SsoBridgeResult {
            established: false,
        })))
    };

    if oidc.config.multi_account {
        let mut browser = accounts::load(&db, &oidc, &req).await?;
        if browser.accounts.iter().any(|(_, u)| u.id == user.id) {
            let mut resp = unchanged();
            for cookie in &browser.cookies {
                let _ = resp.add_cookie(cookie);
            }
            return Ok(resp);
        }
        let (auth_time, ttl) = bridge_window(&req, &oidc)?;
        accounts::add_account(
            &db,
            &oidc,
            &mut browser,
            &user,
            auth_time,
            ttl,
            store::SessionOrigin::JwtBridge,
        )
        .await?;
        let mut resp =
            HttpResponse::Ok().json(UniResponse::ok(Some(SsoBridgeResult { established: true })));
        for cookie in &browser.cookies {
            let _ = resp.add_cookie(cookie);
        }
        return Ok(resp);
    }

    if let Some(current) = sso_cookie_value(&req, &oidc)
        && let Some((_, session_user)) = store::find_valid_session(&db, &current).await?
        && session_user.id == user.id
    {
        return Ok(unchanged());
    }
    let (auth_time, ttl) = bridge_window(&req, &oidc)?;
    if let Some(old) = sso_cookie_value(&req, &oidc) {
        store::revoke_session(&db, &old, "replaced").await?;
    }
    let (value, _) = store::create_session(
        &db,
        user.id,
        auth_time,
        ttl,
        store::SessionOrigin::JwtBridge,
    )
    .await?;
    let mut cookie = sso_cookie(&oidc, value);
    cookie.set_max_age(actix_web::cookie::time::Duration::seconds(ttl));
    Ok(HttpResponse::Ok()
        .cookie(cookie)
        .json(UniResponse::ok(Some(SsoBridgeResult { established: true }))))
}

/// DELETE /api/users/sso —— 撤销当前浏览器的 SSO 会话并清除 Cookie。
///
/// 多账号模式下旧接口没有「哪个账号」的概念：撤销本浏览器全部账号（与旧语义「退出本浏览器 SSO」一致）。
#[delete("/sso")]
pub async fn sign_out(
    req: HttpRequest,
    oidc: Option<WebOidc>,
    db: WebDb,
) -> Result<HttpResponse, AppError> {
    let Some(oidc) = oidc else {
        // 未启用 OIDC 时没有 SSO 会话可清理；保持幂等成功，前端无需区分。
        return Ok(HttpResponse::Ok().json(UniResponse::<()>::ok_none()));
    };
    if !accounts::first_party_request(&req, &oidc) {
        return Err(AppError::Forbidden("跨站请求被拒绝".into()));
    }
    if oidc.config.multi_account {
        let mut browser = accounts::load(&db, &oidc, &req).await?;
        accounts::sign_out_browser(
            &db,
            &oidc,
            &mut browser,
            accounts::RevokeReason::LegacyLogout,
        )
        .await?;
        let mut resp = HttpResponse::Ok().json(UniResponse::<()>::ok_none());
        for cookie in &browser.cookies {
            let _ = resp.add_cookie(cookie);
        }
        return Ok(resp);
    }
    if let Some(value) = sso_cookie_value(&req, &oidc) {
        store::revoke_session(&db, &value, "legacy_logout").await?;
    }
    Ok(HttpResponse::Ok()
        .cookie(removal_cookie(
            sso_cookie_name(oidc.secure_cookies()),
            oidc.secure_cookies(),
        ))
        .json(UniResponse::<()>::ok_none()))
}
