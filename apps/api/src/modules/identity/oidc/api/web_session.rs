//! FloatCTF 主站的第一方 Cookie 会话（`/api/users/web-session`，`[oidc].unified_web_session = true`）。
//!
//! 主站与 OIDC Provider 共用同一个服务端可撤销的身份会话源（`oidc_browser_sessions`，挂在浏览器账号容器上）：
//! - 主站输入密码 → 建立身份会话并设为主站活跃账号（之后在 RP 选择该账号免密）；
//! - 在 OIDC 登录页输入密码 → 主站自动恢复（仅当本浏览器恰好一个可恢复账号时）；
//! - 主站退出 → 撤销对应身份会话（不通知第三方应用）。
//!
//! 浏览器从不拿到平台 JWT：认证凭据只有 HttpOnly 容器 Cookie。所有状态变更请求都要求第一方来源 +
//! `X-CSRF-Token`（由容器秘密派生，经本接口 GET 返回给同源脚本）。详见 `docs/identity/ADR-unified-identity.md`。

use actix_web::{HttpRequest, HttpResponse, get, post, web::Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::protocol::{audit, login_rate_ok};
use crate::api::{AppError, UniResponse, extractor::auth::UserJwtGuard};
use crate::infrastructure::{LogService, WebDb, WebLog};
use crate::modules::identity::oidc::{
    OidcProvider, WebOidc,
    accounts::{self, Browser, CSRF_HEADER, RevokeReason},
    store::SessionOrigin,
};

/// 主站当前用户（只含展示字段）。
#[derive(Debug, Serialize)]
pub struct WebUserDto {
    pub id: Uuid,
    pub username: String,
    pub nickname: String,
    pub avatar: Option<String>,
}

/// 本浏览器中的一个已登录账号。`id` 是会话 id（切换/退出时提交），不是凭据。
#[derive(Debug, Serialize)]
pub struct WebAccountDto {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub nickname: String,
    pub avatar: Option<String>,
    /// 是否为主站当前活跃账号。
    pub active: bool,
}

/// `GET /api/users/web-session` 的响应。
#[derive(Debug, Serialize)]
pub struct WebSessionStatus {
    /// `cookie` = 统一 Cookie 会话；`legacy` = 部署未开启，前端继续使用平台 JWT。
    pub mode: &'static str,
    pub authenticated: bool,
    pub user: Option<WebUserDto>,
    /// 同源脚本随每个请求放进 `X-CSRF-Token`；浏览器还没有账号容器时为空。
    pub csrf_token: Option<String>,
    /// 本浏览器中真实有效的已登录账号（服务端判定，不是历史记录）。
    pub accounts: Vec<WebAccountDto>,
    /// 主站未登录且恰好有一个可自动恢复的账号（`POST /restore` 会成功）。
    pub auto_restore: bool,
    /// 一个浏览器最多同时登录的账号数。
    pub max_accounts: usize,
    /// 部署是否支持「退出所有关联应用」（标准 Back-Channel Logout）。
    pub global_logout: bool,
    /// 只出现在「退出所有关联应用」的响应里：每个应用的真实通知结果。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logout_report: Option<super::logout::GlobalLogoutReport>,
}

impl WebSessionStatus {
    fn legacy() -> Self {
        Self {
            mode: "legacy",
            authenticated: false,
            user: None,
            csrf_token: None,
            accounts: Vec::new(),
            auto_restore: false,
            max_accounts: 1,
            global_logout: false,
            logout_report: None,
        }
    }
}

fn status_of(oidc: &OidcProvider, browser: &Browser) -> WebSessionStatus {
    let active = browser.web_account();
    let active_id = active.map(|(s, _)| s.id);
    WebSessionStatus {
        mode: "cookie",
        authenticated: active.is_some(),
        user: active.map(|(_, u)| WebUserDto {
            id: u.id,
            username: u.username.clone(),
            nickname: u.nickname.clone(),
            avatar: u.avatar.clone(),
        }),
        csrf_token: browser.csrf_token(),
        accounts: browser
            .accounts
            .iter()
            .map(|(s, u)| WebAccountDto {
                id: s.id,
                user_id: u.id,
                username: u.username.clone(),
                nickname: u.nickname.clone(),
                avatar: u.avatar.clone(),
                active: Some(s.id) == active_id,
            })
            .collect(),
        auto_restore: active.is_none() && browser.restorable().len() == 1,
        max_accounts: oidc.config.max_browser_accounts,
        global_logout: oidc.config.backchannel_logout,
        logout_report: None,
    }
}

fn reply(oidc: &OidcProvider, browser: &Browser) -> HttpResponse {
    let mut resp = HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(UniResponse::ok(Some(status_of(oidc, browser))));
    for cookie in &browser.cookies {
        let _ = resp.add_cookie(cookie);
    }
    resp
}

fn unified(oidc: Option<WebOidc>) -> Result<WebOidc, AppError> {
    match oidc {
        Some(oidc) if oidc.config.unified_web_session => Ok(oidc),
        _ => Err(AppError::NotFound("主站统一会话未启用".into())),
    }
}

/// 状态变更请求的 CSRF 校验：第一方来源 + `X-CSRF-Token`。
///
/// 浏览器已有账号容器时，token 必须等于容器派生值；尚无容器（首次登录/迁移）时只要求存在该自定义头
/// ——跨站页面无法在不经 CORS 预检的情况下发送它，CORS 只放行运维配置的第一方 origin（防 Login CSRF）。
fn check_csrf(
    req: &HttpRequest,
    oidc: &OidcProvider,
    browser: &Browser,
    allow_without_container: bool,
) -> Result<(), AppError> {
    if !accounts::first_party_request(req, oidc) {
        return Err(AppError::Forbidden("跨站请求被拒绝".into()));
    }
    let presented = req
        .headers()
        .get(CSRF_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let ok = if browser.container.is_some() {
        browser.csrf_matches(presented)
    } else {
        allow_without_container && !presented.is_empty()
    };
    if ok {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "页面已过期（CSRF 校验失败），请刷新后重试".into(),
        ))
    }
}

/// GET /api/users/web-session —— 主站登录态（同源脚本读取；从不改变状态）。
#[get("/web-session")]
pub async fn status(
    req: HttpRequest,
    oidc: Option<WebOidc>,
    db: WebDb,
) -> Result<HttpResponse, AppError> {
    let Some(oidc) = oidc.filter(|o| o.config.unified_web_session) else {
        return Ok(HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .json(UniResponse::ok(Some(WebSessionStatus::legacy()))));
    };
    let browser = accounts::load(&db, &oidc, &req).await?;
    Ok(reply(&oidc, &browser))
}

#[derive(Debug, Deserialize)]
pub struct WebLoginRequest {
    pub username: String,
    pub password: String,
}

/// POST /api/users/web-session/login —— 主站口令登录。
///
/// 新账号加入本浏览器（已登录的其他账号保留），并成为主站活跃账号。响应体不含任何令牌。
#[post("/web-session/login")]
pub async fn login(
    req: HttpRequest,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    body: Json<WebLoginRequest>,
) -> Result<HttpResponse, AppError> {
    let oidc = unified(oidc)?;
    let mut browser = accounts::load(&db, &oidc, &req).await?;
    check_csrf(&req, &oidc, &browser, true)?;

    let username = body.username.trim().to_string();
    if username.is_empty()
        || body.password.is_empty()
        || username.chars().count() > 64
        || body.password.len() > 256
    {
        return Err(AppError::Validation("请输入用户名和密码".into()));
    }
    let ip = LogService::get_client_ip(&req).unwrap_or_else(|| "unknown".into());
    match login_rate_ok(&db, &oidc, &username, &ip).await {
        Ok(true) => {}
        Ok(false) => return Err(AppError::Conflict("尝试次数过多，请稍后再试".into())),
        Err(e) => {
            tracing::error!(error = %e, "web login rate limiter unavailable (fail-closed)");
            return Err(AppError::Internal("服务暂时不可用，请稍后重试".into()));
        }
    }
    let user = match crate::modules::identity::authentication::verify_user_credentials(
        &db,
        &username,
        &body.password,
    )
    .await?
    {
        (Some(user), true) => user,
        (maybe_user, _) => {
            audit(
                &log,
                "ERROR",
                "WEB_LOGIN_FAILED",
                "主站登录失败".to_string(),
                json!({}),
                maybe_user.map(|u| u.id),
                &req,
            );
            return Err(AppError::Unauthorized);
        }
    };

    let (session, _) = accounts::add_account(
        &db,
        &oidc,
        &mut browser,
        &user,
        chrono::Utc::now().fixed_offset(),
        oidc.config.session_ttl_secs,
        SessionOrigin::PasswordLogin,
    )
    .await?;
    if let Some(container) = browser.container.as_mut() {
        accounts::set_web_session(&db, container, Some(session.id), false).await?;
    }
    audit(
        &log,
        "INFO",
        "WEB_LOGIN",
        format!("{} 登录主站", user.username),
        json!({"accounts_in_browser": browser.accounts.len()}),
        Some(user.id),
        &req,
    );
    Ok(reply(&oidc, &browser))
}

/// POST /api/users/web-session/restore —— 主站从本浏览器已有的身份会话恢复登录。
///
/// 只有恰好一个可恢复账号时成功（例如刚在 LearnOS 的 FloatCTF 登录页输入过密码）；
/// 多个时返回 409，前端必须让用户选择（绝不按最近使用静默挑选）；
/// 主站明确退出之前就存在的会话不会被自动恢复。
#[post("/web-session/restore")]
pub async fn restore(
    req: HttpRequest,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
) -> Result<HttpResponse, AppError> {
    let oidc = unified(oidc)?;
    let mut browser = accounts::load(&db, &oidc, &req).await?;
    check_csrf(&req, &oidc, &browser, false)?;
    if browser.web_account().is_some() {
        return Ok(reply(&oidc, &browser));
    }
    let restorable: Vec<_> = browser.restorable().into_iter().cloned().collect();
    let [(session, user)] = restorable.as_slice() else {
        return Err(AppError::Conflict(if restorable.is_empty() {
            "没有可恢复的登录".into()
        } else {
            "本浏览器有多个已登录账号，请选择要使用的账号".into()
        }));
    };
    if let Some(container) = browser.container.as_mut() {
        accounts::set_web_session(&db, container, Some(session.id), false).await?;
    }
    audit(
        &log,
        "INFO",
        "WEB_SESSION_RESTORED",
        format!("{} 从统一身份会话恢复主站登录", user.username),
        json!({"established_via": session.established_via}),
        Some(user.id),
        &req,
    );
    Ok(reply(&oidc, &browser))
}

#[derive(Debug, Deserialize)]
pub struct WebSelectRequest {
    pub account_id: Uuid,
}

/// POST /api/users/web-session/select —— 主站切换到本浏览器中的另一个已登录账号（用户明确选择）。
#[post("/web-session/select")]
pub async fn select(
    req: HttpRequest,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    body: Json<WebSelectRequest>,
) -> Result<HttpResponse, AppError> {
    let oidc = unified(oidc)?;
    let mut browser = accounts::load(&db, &oidc, &req).await?;
    check_csrf(&req, &oidc, &browser, false)?;
    let Some((session, user)) = browser.find(body.account_id).cloned() else {
        return Err(AppError::NotFound(
            "该账号在本浏览器中的登录已失效，请重新登录".into(),
        ));
    };
    if let Some(container) = browser.container.as_mut() {
        accounts::set_web_session(&db, container, Some(session.id), false).await?;
    }
    audit(
        &log,
        "INFO",
        "WEB_ACCOUNT_SWITCH",
        format!("主站切换到账号 {}", user.username),
        json!({}),
        Some(user.id),
        &req,
    );
    Ok(reply(&oidc, &browser))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebLogoutScope {
    /// 退出本浏览器中的一个账号（默认主站当前账号）。
    Account,
    /// 退出本浏览器中的全部账号。
    Browser,
    /// 退出当前账号并通知关联应用（Back-Channel Logout；部署开启时可用，需用户在界面上明确确认）。
    Global,
}

#[derive(Debug, Deserialize)]
pub struct WebLogoutRequest {
    pub scope: WebLogoutScope,
    #[serde(default)]
    pub account_id: Option<Uuid>,
}

/// POST /api/users/web-session/logout —— 主站退出。
///
/// 撤销成功（数据库已提交）才返回 200；任何失败都返回错误，前端必须如实提示「未完成退出」。
/// `account` / `browser`（普通退出）**不**通知第三方应用：LearnOS 等已建立的本地会话保持不变，之后再经 OIDC 登录需要重新认证。
/// `global`：撤销当前账号在本浏览器的身份会话，并向拿到过该会话令牌、登记了 Back-Channel 的应用投递 Logout Token；
/// 响应的 `logout_report` 如实列出每个应用的结果与不支持即时通知的应用。
#[post("/web-session/logout")]
pub async fn logout(
    req: HttpRequest,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
    body: Json<WebLogoutRequest>,
) -> Result<HttpResponse, AppError> {
    let oidc = unified(oidc)?;
    let mut browser = accounts::load(&db, &oidc, &req).await?;
    check_csrf(&req, &oidc, &browser, false)?;
    match body.scope {
        WebLogoutScope::Account => {
            let target = body
                .account_id
                .or_else(|| browser.web_account().map(|(s, _)| s.id));
            if let Some(id) = target
                && let Some((_, user)) = browser.find(id).cloned()
            {
                accounts::sign_out_account(&db, &mut browser, id, RevokeReason::Logout).await?;
                audit(
                    &log,
                    "INFO",
                    "WEB_LOGOUT",
                    format!("{} 在主站退出", user.username),
                    json!({"scope": "account"}),
                    Some(user.id),
                    &req,
                );
            }
        }
        WebLogoutScope::Global => {
            if !oidc.config.backchannel_logout {
                return Err(AppError::NotFound("部署未开启「退出所有关联应用」".into()));
            }
            let Some(target) = browser.web_account().cloned() else {
                return Ok(reply(&oidc, &browser));
            };
            let user = target.1.clone();
            let report = super::logout::global_logout(
                &db,
                &oidc,
                &mut browser,
                std::slice::from_ref(&target),
                crate::modules::identity::oidc::backchannel::Reason::MainSite,
            )
            .await?;
            audit(
                &log,
                "INFO",
                "OIDC_GLOBAL_LOGOUT",
                format!("{} 在主站退出所有关联应用", user.username),
                json!({
                    "deliveries": report.deliveries,
                    "without_backchannel": report.without_backchannel,
                }),
                Some(user.id),
                &req,
            );
            let mut current = status_of(&oidc, &browser);
            current.logout_report = Some(report);
            let mut resp = HttpResponse::Ok()
                .insert_header(("Cache-Control", "no-store"))
                .json(UniResponse::ok(Some(current)));
            for cookie in &browser.cookies {
                let _ = resp.add_cookie(cookie);
            }
            return Ok(resp);
        }
        WebLogoutScope::Browser => {
            let revoked =
                accounts::sign_out_browser(&db, &oidc, &mut browser, RevokeReason::LogoutBrowser)
                    .await?;
            for (_, user) in &revoked {
                audit(
                    &log,
                    "INFO",
                    "WEB_LOGOUT",
                    format!("{} 随「退出本浏览器全部账号」退出", user.username),
                    json!({"scope": "browser"}),
                    Some(user.id),
                    &req,
                );
            }
        }
    }
    Ok(reply(&oidc, &browser))
}

/// POST /api/users/web-session/migrate —— 把旧版 localStorage 平台 JWT 换成服务端 Cookie 会话。
///
/// 只接受 `Authorization: Bearer <平台 JWT>`（不能用 Cookie 调用）；认证时间取 JWT 签发时间，
/// 会话不比 JWT 活得更久。主站已有活跃账号时不覆盖（前端随后丢弃旧令牌）。
#[post("/web-session/migrate")]
pub async fn migrate(
    req: HttpRequest,
    user: UserJwtGuard,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
) -> Result<HttpResponse, AppError> {
    let oidc = unified(oidc)?;
    if !req
        .headers()
        .contains_key(actix_web::http::header::AUTHORIZATION)
    {
        return Err(AppError::Unauthorized);
    }
    let user = user.into_inner();
    let mut browser = accounts::load(&db, &oidc, &req).await?;
    check_csrf(&req, &oidc, &browser, true)?;
    if browser.web_account().is_some() {
        return Ok(reply(&oidc, &browser));
    }
    let session_id = match browser.accounts.iter().find(|(_, u)| u.id == user.id) {
        Some((s, _)) => s.id,
        None => {
            let (auth_time, ttl) = super::session::bridge_window(&req, &oidc)?;
            let (session, _) = accounts::add_account(
                &db,
                &oidc,
                &mut browser,
                &user,
                auth_time,
                ttl,
                SessionOrigin::JwtBridge,
            )
            .await?;
            session.id
        }
    };
    if let Some(container) = browser.container.as_mut() {
        accounts::set_web_session(&db, container, Some(session_id), false).await?;
    }
    audit(
        &log,
        "INFO",
        "WEB_SESSION_MIGRATED",
        format!("{} 的旧版浏览器令牌迁移为服务端会话", user.username),
        json!({}),
        Some(user.id),
        &req,
    );
    Ok(reply(&oidc, &browser))
}
