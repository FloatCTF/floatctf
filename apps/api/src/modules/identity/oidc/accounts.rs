//! 浏览器账号容器（多账号）与 FloatCTF 主站 Cookie 会话的服务端模型。
//!
//! 设计见 `docs/identity/ADR-unified-identity.md`：
//! - 浏览器只持有**一个** HttpOnly Cookie（`__Host-fctf_accounts`），值是容器随机秘密，库内只存摘要；
//! - 容器内每个已认证账号是一行 `oidc_browser_sessions`（独立认证时间、过期、撤销）；
//! - 主站当前活跃账号 = `oidc_browser_containers.web_session_id`。RP 授权时选择账号**不会**改动它；
//! - `[oidc].multi_account = false` 时退回旧版单会话 `__Host-fctf_sso`，本模块只做读取适配。
//!
//! 一切「当前浏览器有哪些账号」的判断都以服务端行为准：Cookie 只是指针，从不携带用户名或用户 ID。

use actix_web::{HttpRequest, cookie::Cookie, http::header};
use chrono::{DateTime, Duration, FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, sea_query::Expr,
};
use uuid::Uuid;

use crate::api::AppError;
use crate::entity::{oidc_browser_containers, oidc_browser_sessions, users};

use super::OidcProvider;
use super::api::protocol::{build_cookie, removal_cookie, sso_cookie_name, sso_cookie_value};
use super::domain::secret;
use super::flow::SessionUser;
use super::store::{self, SessionOrigin};

/// 账号容器 Cookie 名。https issuer 用 `__Host-` 前缀（强制 Secure、Path=/、无 Domain）。
pub fn container_cookie_name(secure: bool) -> &'static str {
    if secure {
        "__Host-fctf_accounts"
    } else {
        "fctf_accounts"
    }
}

/// 主站 Cookie 会话的 CSRF 请求头。
pub const CSRF_HEADER: &str = "X-CSRF-Token";
/// 主站请求声明「我以为当前是哪个账号」（防多标签串号）。
pub const EXPECTED_ACCOUNT_HEADER: &str = "X-FloatCTF-Account";

/// 会话撤销原因（与表 CHECK 约束一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokeReason {
    Logout,
    LogoutBrowser,
    GlobalLogout,
    Replaced,
    Evicted,
    LegacyLogout,
}

impl RevokeReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logout => "logout",
            Self::LogoutBrowser => "logout_browser",
            Self::GlobalLogout => "global_logout",
            Self::Replaced => "replaced",
            Self::Evicted => "evicted",
            Self::LegacyLogout => "legacy_logout",
        }
    }
}

fn now() -> DateTime<FixedOffset> {
    Utc::now().fixed_offset()
}

/// 由容器秘密派生的主站 CSRF token（域分隔 + SHA-256；不可逆推容器秘密）。
pub fn csrf_for(container_secret: &str) -> String {
    secret::sha256_hex(&format!("fctf-web-csrf\u{0}{container_secret}"))
}

/// 当前请求所在浏览器的账号视图。
#[derive(Debug, Default)]
pub struct Browser {
    /// 账号容器（`multi_account` 模式且 Cookie 有效时）。
    pub container: Option<oidc_browser_containers::Model>,
    /// 容器秘密（只用于派生 CSRF 与轮换，从不输出给前端）。
    secret: Option<String>,
    /// 本浏览器有效的账号会话（未过期、未撤销、用户仍存在），最近使用在前。
    pub accounts: Vec<SessionUser>,
    /// 处理过程中需要下发的 Cookie（迁移旧 Cookie、轮换秘密、退出清理）。
    pub cookies: Vec<Cookie<'static>>,
}

impl Browser {
    pub fn csrf_token(&self) -> Option<String> {
        self.secret.as_deref().map(csrf_for)
    }

    /// 常量时间比较请求携带的 CSRF token。
    pub fn csrf_matches(&self, presented: &str) -> bool {
        use subtle::ConstantTimeEq;
        match self.csrf_token() {
            Some(expected) => expected.as_bytes().ct_eq(presented.as_bytes()).into(),
            None => false,
        }
    }

    pub fn find(&self, session_id: Uuid) -> Option<&SessionUser> {
        self.accounts.iter().find(|(s, _)| s.id == session_id)
    }

    /// 主站当前活跃账号（必须仍是本浏览器的有效会话）。
    pub fn web_account(&self) -> Option<&SessionUser> {
        self.container
            .as_ref()
            .and_then(|c| c.web_session_id)
            .and_then(|id| self.find(id))
    }

    /// 主站可**自动**恢复的账号：主站最近一次明确退出之后才建立的会话。
    /// 只有恰好一个时才允许自动恢复（多个时必须由用户选择，绝不按最近使用静默挑选）。
    pub fn restorable(&self) -> Vec<&SessionUser> {
        let signed_out = self.container.as_ref().and_then(|c| c.web_signed_out_at);
        self.accounts
            .iter()
            .filter(|(s, _)| signed_out.is_none_or(|t| s.created_at > t))
            .collect()
    }
}

fn cookie_value(req: &HttpRequest, name: &str) -> Option<String> {
    req.cookie(name)
        .map(|c| c.value().to_string())
        .filter(|v| secret::is_plausible_token(v))
}

fn container_cookie(oidc: &OidcProvider, value: String) -> Cookie<'static> {
    build_cookie(
        container_cookie_name(oidc.secure_cookies()),
        value,
        oidc.config.browser_container_ttl_secs,
        oidc.secure_cookies(),
    )
}

async fn find_container(
    db: &DatabaseConnection,
    secret_value: &str,
) -> Result<Option<oidc_browser_containers::Model>, AppError> {
    Ok(oidc_browser_containers::Entity::find()
        .filter(oidc_browser_containers::Column::SecretHash.eq(secret::sha256_hex(secret_value)))
        .filter(oidc_browser_containers::Column::RevokedAt.is_null())
        .filter(oidc_browser_containers::Column::ExpiresAt.gt(now()))
        .one(db)
        .await?)
}

/// 容器内有效的账号会话（最近使用在前）。
pub async fn container_accounts(
    db: &DatabaseConnection,
    container_id: Uuid,
) -> Result<Vec<SessionUser>, AppError> {
    let rows = oidc_browser_sessions::Entity::find()
        .filter(oidc_browser_sessions::Column::ContainerId.eq(container_id))
        .filter(oidc_browser_sessions::Column::RevokedAt.is_null())
        .filter(oidc_browser_sessions::Column::ExpiresAt.gt(now()))
        .find_also_related(users::Entity)
        .order_by_desc(oidc_browser_sessions::Column::LastSeenAt)
        .all(db)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(s, u)| u.map(|u| (s, u)))
        .collect())
}

/// 读取当前浏览器的账号视图。
///
/// `multi_account` 模式下，若浏览器只有旧版 `__Host-fctf_sso`（本功能上线前登录），
/// 把该会话无损迁入新容器并下发新 Cookie、清除旧 Cookie。
pub async fn load(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    req: &HttpRequest,
) -> Result<Browser, AppError> {
    if !oidc.config.multi_account {
        let accounts = match sso_cookie_value(req, oidc) {
            Some(v) => store::find_valid_session(db, &v)
                .await?
                .into_iter()
                .collect(),
            None => Vec::new(),
        };
        return Ok(Browser {
            accounts,
            ..Browser::default()
        });
    }

    let secure = oidc.secure_cookies();
    if let Some(value) = cookie_value(req, container_cookie_name(secure))
        && let Some(container) = find_container(db, &value).await?
    {
        let accounts = container_accounts(db, container.id).await?;
        let mut browser = Browser {
            container: Some(container),
            secret: Some(value),
            accounts,
            cookies: Vec::new(),
        };
        renew_if_stale(db, oidc, &mut browser).await?;
        return Ok(browser);
    }

    // 旧版单会话 Cookie → 迁入新容器（会话行、认证时间、过期时间原样保留）。
    let mut browser = Browser::default();
    if let Some(old) = sso_cookie_value(req, oidc)
        && let Some((session, user)) = store::find_valid_session(db, &old).await?
    {
        ensure_container(db, oidc, &mut browser).await?;
        let container_id = browser.container.as_ref().map(|c| c.id);
        oidc_browser_sessions::Entity::update_many()
            .col_expr(
                oidc_browser_sessions::Column::ContainerId,
                Expr::value(container_id),
            )
            .filter(oidc_browser_sessions::Column::Id.eq(session.id))
            .exec(db)
            .await?;
        let session = oidc_browser_sessions::Model {
            container_id,
            ..session
        };
        browser.accounts.push((session, user));
        browser
            .cookies
            .push(removal_cookie(sso_cookie_name(secure), secure));
    }
    Ok(browser)
}

/// 容器滑动续期：距上次使用超过 1 小时才写库并重发 Cookie（避免每个请求都写）。
async fn renew_if_stale(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    browser: &mut Browser,
) -> Result<(), AppError> {
    let (Some(container), Some(value)) = (browser.container.as_mut(), browser.secret.clone())
    else {
        return Ok(());
    };
    if container.last_seen_at > now() - Duration::hours(1) {
        return Ok(());
    }
    let now = now();
    let expires = now + Duration::seconds(oidc.config.browser_container_ttl_secs);
    oidc_browser_containers::Entity::update_many()
        .col_expr(
            oidc_browser_containers::Column::LastSeenAt,
            Expr::value(now),
        )
        .col_expr(
            oidc_browser_containers::Column::ExpiresAt,
            Expr::value(expires),
        )
        .filter(oidc_browser_containers::Column::Id.eq(container.id))
        .exec(db)
        .await?;
    container.last_seen_at = now;
    container.expires_at = expires;
    browser.cookies.push(container_cookie(oidc, value));
    Ok(())
}

/// 确保浏览器有容器（没有则新建并下发 Cookie）。
pub async fn ensure_container(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    browser: &mut Browser,
) -> Result<(), AppError> {
    if browser.container.is_some() {
        return Ok(());
    }
    let value = secret::new_opaque_secret();
    let now = now();
    let container = oidc_browser_containers::ActiveModel {
        id: Set(Uuid::new_v4()),
        secret_hash: Set(secret::sha256_hex(&value)),
        web_session_id: Set(None),
        web_signed_out_at: Set(None),
        created_at: Set(now),
        last_seen_at: Set(now),
        expires_at: Set(now + Duration::seconds(oidc.config.browser_container_ttl_secs)),
        revoked_at: Set(None),
    }
    .insert(db)
    .await?;
    browser.cookies.push(container_cookie(oidc, value.clone()));
    browser.container = Some(container);
    browser.secret = Some(value);
    Ok(())
}

/// 轮换容器秘密（每次口令认证都轮换：防会话固定——预置的容器 Cookie 在受害者登录后即作废）。
async fn rotate_secret(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    browser: &mut Browser,
) -> Result<(), AppError> {
    let Some(container) = browser.container.as_mut() else {
        return Ok(());
    };
    let value = secret::new_opaque_secret();
    let hash = secret::sha256_hex(&value);
    let now = now();
    let expires = now + Duration::seconds(oidc.config.browser_container_ttl_secs);
    oidc_browser_containers::Entity::update_many()
        .col_expr(
            oidc_browser_containers::Column::SecretHash,
            Expr::value(hash.clone()),
        )
        .col_expr(
            oidc_browser_containers::Column::LastSeenAt,
            Expr::value(now),
        )
        .col_expr(
            oidc_browser_containers::Column::ExpiresAt,
            Expr::value(expires),
        )
        .filter(oidc_browser_containers::Column::Id.eq(container.id))
        .exec(db)
        .await?;
    container.secret_hash = hash;
    container.last_seen_at = now;
    container.expires_at = expires;
    browser
        .cookies
        .retain(|c| c.name() != container_cookie_name(oidc.secure_cookies()));
    browser.cookies.push(container_cookie(oidc, value.clone()));
    browser.secret = Some(value);
    Ok(())
}

/// 撤销一组会话（幂等），返回实际撤销的行数。
pub async fn revoke_sessions(
    db: &DatabaseConnection,
    ids: &[Uuid],
    reason: RevokeReason,
) -> Result<u64, AppError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let res = oidc_browser_sessions::Entity::update_many()
        .col_expr(oidc_browser_sessions::Column::RevokedAt, Expr::value(now()))
        .col_expr(
            oidc_browser_sessions::Column::RevokedReason,
            Expr::value(reason.as_str()),
        )
        .filter(oidc_browser_sessions::Column::Id.is_in(ids.to_vec()))
        .filter(oidc_browser_sessions::Column::RevokedAt.is_null())
        .exec(db)
        .await?;
    Ok(res.rows_affected)
}

/// 口令认证成功后把账号加入本浏览器（`multi_account` 模式）。
///
/// - 同一用户在本容器已有的会话被替换（`replaced`）——一个浏览器里每个账号只有一个会话；
/// - 其他账号的会话保留（多账号）；超过 `max_browser_accounts` 时淘汰最久未用的（`evicted`）；
/// - 轮换容器秘密；**不**改动主站活跃账号（由调用方按场景决定）。
pub async fn add_account(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    browser: &mut Browser,
    user: &users::Model,
    auth_time: DateTime<FixedOffset>,
    ttl_secs: i64,
    origin: SessionOrigin,
) -> Result<SessionUser, AppError> {
    ensure_container(db, oidc, browser).await?;
    rotate_secret(db, oidc, browser).await?;
    let container_id = browser.container.as_ref().map(|c| c.id);

    let replaced: Vec<Uuid> = browser
        .accounts
        .iter()
        .filter(|(_, u)| u.id == user.id)
        .map(|(s, _)| s.id)
        .collect();
    revoke_sessions(db, &replaced, RevokeReason::Replaced).await?;
    browser.accounts.retain(|(_, u)| u.id != user.id);

    let max = oidc.config.max_browser_accounts;
    if browser.accounts.len() >= max {
        // accounts 按最近使用排序：从尾部淘汰。
        let evicted: Vec<Uuid> = browser.accounts[max - 1..]
            .iter()
            .map(|(s, _)| s.id)
            .collect();
        revoke_sessions(db, &evicted, RevokeReason::Evicted).await?;
        browser.accounts.truncate(max - 1);
    }

    let (_, session) =
        store::create_session_in(db, user.id, auth_time, ttl_secs, origin, container_id).await?;
    let entry = (session, user.clone());
    browser.accounts.insert(0, entry.clone());

    // 被替换/淘汰的会话若正是主站活跃账号，指针随之清空（主站需要重新确定账号）。
    if let Some(container) = browser.container.as_mut()
        && container.web_session_id.is_some_and(|id| {
            replaced.contains(&id) || !browser.accounts.iter().any(|(s, _)| s.id == id)
        })
    {
        set_web_session(db, container, None, false).await?;
    }
    Ok(entry)
}

/// 设置主站活跃账号；`signed_out = true` 同时记录「主站明确退出」时间（抑制旧会话被自动恢复）。
pub async fn set_web_session(
    db: &DatabaseConnection,
    container: &mut oidc_browser_containers::Model,
    session_id: Option<Uuid>,
    signed_out: bool,
) -> Result<(), AppError> {
    let mut update = oidc_browser_containers::Entity::update_many().col_expr(
        oidc_browser_containers::Column::WebSessionId,
        Expr::value(session_id),
    );
    let now = now();
    if signed_out {
        update = update.col_expr(
            oidc_browser_containers::Column::WebSignedOutAt,
            Expr::value(Some(now)),
        );
        container.web_signed_out_at = Some(now);
    }
    update
        .filter(oidc_browser_containers::Column::Id.eq(container.id))
        .exec(db)
        .await?;
    container.web_session_id = session_id;
    Ok(())
}

/// 退出本浏览器中的一个账号：撤销该会话；若它是主站活跃账号，主站随之退出（并抑制自动恢复）。
pub async fn sign_out_account(
    db: &DatabaseConnection,
    browser: &mut Browser,
    session_id: Uuid,
    reason: RevokeReason,
) -> Result<bool, AppError> {
    if browser.find(session_id).is_none() {
        return Ok(false);
    }
    revoke_sessions(db, &[session_id], reason).await?;
    browser.accounts.retain(|(s, _)| s.id != session_id);
    if let Some(container) = browser.container.as_mut()
        && container.web_session_id == Some(session_id)
    {
        set_web_session(db, container, None, true).await?;
    }
    Ok(true)
}

/// 退出本浏览器全部账号：撤销所有会话与容器，清除 Cookie。返回被撤销的会话。
pub async fn sign_out_browser(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    browser: &mut Browser,
    reason: RevokeReason,
) -> Result<Vec<SessionUser>, AppError> {
    let secure = oidc.secure_cookies();
    let revoked = std::mem::take(&mut browser.accounts);
    let ids: Vec<Uuid> = revoked.iter().map(|(s, _)| s.id).collect();
    revoke_sessions(db, &ids, reason).await?;
    if let Some(container) = browser.container.take() {
        // 容器里可能还有已过期但未撤销的旧行：一并撤销，避免日后误认。
        oidc_browser_sessions::Entity::update_many()
            .col_expr(oidc_browser_sessions::Column::RevokedAt, Expr::value(now()))
            .col_expr(
                oidc_browser_sessions::Column::RevokedReason,
                Expr::value(reason.as_str()),
            )
            .filter(oidc_browser_sessions::Column::ContainerId.eq(container.id))
            .filter(oidc_browser_sessions::Column::RevokedAt.is_null())
            .exec(db)
            .await?;
        oidc_browser_containers::Entity::update_many()
            .col_expr(
                oidc_browser_containers::Column::RevokedAt,
                Expr::value(now()),
            )
            .col_expr(
                oidc_browser_containers::Column::WebSessionId,
                Expr::value(Option::<Uuid>::None),
            )
            .filter(oidc_browser_containers::Column::Id.eq(container.id))
            .exec(db)
            .await?;
    }
    browser.secret = None;
    browser.cookies.clear();
    browser
        .cookies
        .push(removal_cookie(container_cookie_name(secure), secure));
    browser
        .cookies
        .push(removal_cookie(sso_cookie_name(secure), secure));
    Ok(revoked)
}

// ── 主站 Cookie 会话请求校验 ────────────────────────────────────────────────

/// 请求是否来自第一方页面：有 `Origin` 必须是第一方 origin；否则有 `Sec-Fetch-Site` 必须同源。
/// 两者都没有（非浏览器客户端）时放行——这类请求不存在 CSRF 问题，凭据仍须匹配。
pub fn first_party_request(req: &HttpRequest, oidc: &OidcProvider) -> bool {
    if let Some(origin) = req.headers().get(header::ORIGIN) {
        return origin
            .to_str()
            .is_ok_and(|o| oidc.config.is_first_party_origin(o));
    }
    match req
        .headers()
        .get("Sec-Fetch-Site")
        .and_then(|v| v.to_str().ok())
    {
        Some(site) => matches!(site, "same-origin" | "none"),
        None => true,
    }
}

fn header_value<'a>(req: &'a HttpRequest, name: &str) -> Option<&'a str> {
    req.headers().get(name).and_then(|v| v.to_str().ok())
}

/// 主站 Cookie 会话认证失败的原因（都映射为 401，原因只进日志）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebAuthFailure {
    NoSession,
    CrossSite,
    Csrf,
    AccountChanged,
}

/// `UserJwtGuard` 的 Cookie 通道：容器 → 主站活跃会话 → 用户。
///
/// 只在 `unified_web_session` 开启时调用，且仅当请求**没有** `Authorization` 头（Bearer 优先，两种机制不混用）。
/// 每个请求都要求：第一方来源 + `X-CSRF-Token` 与容器派生值一致；
/// 带 `X-FloatCTF-Account` 时必须等于当前活跃用户（另一标签页切换了账号 → 401，前端重新同步）。
pub async fn resolve_web_user(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    req: &HttpRequest,
) -> Result<Result<users::Model, WebAuthFailure>, AppError> {
    let Some(value) = cookie_value(req, container_cookie_name(oidc.secure_cookies())) else {
        return Ok(Err(WebAuthFailure::NoSession));
    };
    if !first_party_request(req, oidc) {
        return Ok(Err(WebAuthFailure::CrossSite));
    }
    let csrf_ok = header_value(req, CSRF_HEADER).is_some_and(|presented| {
        use subtle::ConstantTimeEq;
        csrf_for(&value)
            .as_bytes()
            .ct_eq(presented.as_bytes())
            .into()
    });
    if !csrf_ok {
        return Ok(Err(WebAuthFailure::Csrf));
    }
    let Some(container) = find_container(db, &value).await? else {
        return Ok(Err(WebAuthFailure::NoSession));
    };
    let Some(session_id) = container.web_session_id else {
        return Ok(Err(WebAuthFailure::NoSession));
    };
    let found = oidc_browser_sessions::Entity::find_by_id(session_id)
        .filter(oidc_browser_sessions::Column::ContainerId.eq(container.id))
        .filter(oidc_browser_sessions::Column::RevokedAt.is_null())
        .filter(oidc_browser_sessions::Column::ExpiresAt.gt(now()))
        .find_also_related(users::Entity)
        .one(db)
        .await?;
    let Some((_, Some(user))) = found else {
        return Ok(Err(WebAuthFailure::NoSession));
    };
    if let Some(expected) = header_value(req, EXPECTED_ACCOUNT_HEADER)
        && expected != user.id.to_string()
    {
        return Ok(Err(WebAuthFailure::AccountChanged));
    }
    Ok(Ok(user))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csrf_is_bound_to_container_secret_and_not_reversible() {
        let a = csrf_for("secret-a");
        assert_eq!(a, csrf_for("secret-a"));
        assert_ne!(a, csrf_for("secret-b"));
        assert!(!a.contains("secret-a"));
        assert_eq!(a.len(), 64);
        // 与直接摘要不同（域分隔），容器摘要泄露也推不出 CSRF。
        assert_ne!(a, secret::sha256_hex("secret-a"));
    }

    #[test]
    fn container_cookie_uses_host_prefix_only_over_https() {
        assert_eq!(container_cookie_name(true), "__Host-fctf_accounts");
        assert_eq!(container_cookie_name(false), "fctf_accounts");
    }

    fn session(created_at: DateTime<FixedOffset>) -> SessionUser {
        let now = now();
        let user_id = Uuid::new_v4();
        (
            oidc_browser_sessions::Model {
                id: Uuid::new_v4(),
                session_hash: String::new(),
                user_id,
                auth_time: created_at,
                established_via: "oidc_login".into(),
                created_at,
                last_seen_at: now,
                expires_at: now,
                revoked_at: None,
                container_id: None,
                sid: String::new(),
                revoked_reason: None,
            },
            users::Model {
                id: user_id,
                username: "u".into(),
                nickname: "U".into(),
                password: String::new(),
                email: String::new(),
                created_at: now,
                updated_at: now,
                avatar: None,
            },
        )
    }

    #[test]
    fn explicit_sign_out_suppresses_auto_restore_of_older_sessions_only() {
        let t0 = now() - Duration::hours(2);
        let old = session(t0);
        let newer = session(t0 + Duration::hours(1));
        let container = oidc_browser_containers::Model {
            id: Uuid::new_v4(),
            secret_hash: String::new(),
            web_session_id: None,
            web_signed_out_at: Some(t0 + Duration::minutes(30)),
            created_at: t0,
            last_seen_at: t0,
            expires_at: t0,
            revoked_at: None,
        };
        let browser = Browser {
            container: Some(container),
            secret: None,
            accounts: vec![newer.clone(), old.clone()],
            cookies: vec![],
        };
        let restorable = browser.restorable();
        assert_eq!(restorable.len(), 1);
        assert_eq!(restorable[0].0.id, newer.0.id);
        assert!(browser.web_account().is_none());
    }
}
