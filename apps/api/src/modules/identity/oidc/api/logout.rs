//! OpenID Connect RP-Initiated Logout 1.0（`end_session_endpoint`）与「退出所有关联应用」的执行。
//!
//! 安全要点：
//! - 只有 `[oidc].backchannel_logout = true` 时存在（Discovery 同步宣告）；
//! - `id_token_hint` 必须由本 OP 签发（允许已过期，§2）；`client_id` 与其 `aud` 不一致即拒绝；
//! - `post_logout_redirect_uri` 必须与应用登记值逐字相等，否则**不重定向**；
//! - 只作用于**本浏览器** Cookie 中、与 hint 的 `sid`/`sub` 匹配的会话：第三方无法借此退出别人的身份；
//! - 退出前必须在 OP 确认页上由用户确认（一次性事务 + 浏览器绑定 + CSRF）；GET 从不改变状态；
//! - 结果页如实列出每个应用的通知结果；未接入标准退出的应用明确提示「可能仍保持登录」。

use actix_web::{HttpRequest, HttpResponse, http::StatusCode, post, route, web};
use sea_orm::EntityTrait;
use serde_json::json;
use uuid::Uuid;

use super::protocol::{
    audit, binding_cookie_name, build_cookie, redirect, removal_cookie, server_error_page,
    sso_cookie_name,
};
use super::{not_enabled, pages};
use crate::infrastructure::{LogService, WebDb, WebLog};
use crate::modules::identity::oidc::{
    OidcProvider, WebOidc,
    accounts::{self, Browser, RevokeReason},
    backchannel::{self, AffectedClient, DeliveryOutcome, Reason},
    clients,
    domain::{
        authorize::RawParams, claims::IdTokenClaims, metadata::PATH_END_SESSION_CONFIRM,
        redirect_uri, secret,
    },
    flow::{self, SessionUser},
    store,
};

const LOGOUT_REQUEST_TTL_SECS: i64 = 600;
const BINDING_COOKIE_MAX_AGE_SECS: i64 = 30 * 24 * 3600;

fn parse_pairs(raw: &[u8]) -> RawParams {
    RawParams::from_pairs(
        url::form_urlencoded::parse(raw).map(|(k, v)| (k.into_owned(), v.into_owned())),
    )
}

fn is_form(req: &HttpRequest) -> bool {
    req.headers()
        .get(actix_web::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|v| {
            v.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
}

fn logout_error(message: &str) -> HttpResponse {
    pages::html(
        StatusCode::BAD_REQUEST,
        "error.html",
        minijinja::context! { title => "无法完成退出", message => message },
        &[],
    )
}

/// 退出后回跳地址（带 RP 的 state）。
fn back_to_client(uri: &str, state: Option<&str>) -> String {
    match state {
        Some(state) => redirect_uri::append_query(uri, &[("state", state)]),
        None => uri.to_string(),
    }
}

/// 本次退出作用的会话：hint 的 `sid` 优先、其次 `sub`；没有 hint 时为本浏览器全部账号。
fn targets(browser: &Browser, hint_sid: Option<&str>, hint_sub: Option<&str>) -> Vec<SessionUser> {
    browser
        .accounts
        .iter()
        .filter(|(s, u)| match (hint_sid, hint_sub) {
            (Some(sid), _) => s.sid == sid,
            (None, Some(sub)) => u.id.to_string() == sub,
            (None, None) => true,
        })
        .cloned()
        .collect()
}

#[derive(serde::Serialize)]
struct AccountChip {
    nickname: String,
    username: String,
    initial: String,
}

fn chip((_, user): &SessionUser) -> AccountChip {
    AccountChip {
        nickname: user.nickname.clone(),
        username: user.username.clone(),
        initial: user
            .nickname
            .trim()
            .chars()
            .next()
            .or_else(|| user.username.chars().next())
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".into()),
    }
}

/// GET|POST /oauth/end-session
#[route("/oauth/end-session", method = "GET", method = "POST")]
pub async fn end_session(
    req: HttpRequest,
    body: web::Bytes,
    oidc: Option<WebOidc>,
    db: WebDb,
) -> HttpResponse {
    let Some(oidc) = oidc.filter(|o| o.config.backchannel_logout) else {
        return not_enabled();
    };
    let params = if req.method() == actix_web::http::Method::POST {
        if !is_form(&req) {
            return logout_error("退出请求必须使用 application/x-www-form-urlencoded。");
        }
        parse_pairs(&body)
    } else {
        parse_pairs(req.query_string().as_bytes())
    };
    let single = |name: &str| params.single(name).map(|v| v.map(str::to_string));
    let (Ok(hint), Ok(client_id), Ok(redirect_to), Ok(state)) = (
        single("id_token_hint"),
        single("client_id"),
        single("post_logout_redirect_uri"),
        single("state"),
    ) else {
        return logout_error("退出请求参数重复。");
    };

    let ip = LogService::get_client_ip(&req).unwrap_or_else(|| "unknown".into());
    match oidc.rate.hit("oidc_end_session", &ip, 60, 300).await {
        Ok(true) => {}
        Ok(false) => return logout_error("请求过于频繁，请稍后再试。"),
        Err(e) => {
            tracing::error!(error = %e, "end_session rate limiter unavailable (fail-closed)");
            return pages::error_page(503, "服务暂时不可用，请稍后重试。");
        }
    }

    let hint = match hint.as_deref() {
        None => None,
        Some(token) => match oidc
            .keys
            .verify::<IdTokenClaims>(token, oidc.issuer(), true)
        {
            Ok(claims) => Some(claims),
            Err(_) => {
                return logout_error("退出请求无效：id_token_hint 不是本站签发的有效 ID Token。");
            }
        },
    };
    if let (Some(h), Some(cid)) = (&hint, &client_id)
        && h.aud != *cid
    {
        return logout_error("退出请求无效：client_id 与 id_token_hint 不一致。");
    }
    let client_key = client_id.or_else(|| hint.as_ref().map(|h| h.aud.clone()));
    let client = match &client_key {
        Some(id) => match clients::find_by_client_id(&db, id).await {
            Ok(c) => c,
            Err(_) => return server_error_page(),
        },
        None => None,
    };
    if client_key.is_some() && client.is_none() {
        return logout_error("退出请求无效：应用不存在。");
    }
    let redirect_to = match redirect_to {
        None => None,
        Some(uri) => {
            let registered = client
                .as_ref()
                .filter(|c| {
                    c.status == crate::entity::sea_orm_active_enums::OidcClientStatus::Active
                })
                .map(clients::post_logout_redirect_uris_of)
                .unwrap_or_default();
            if !registered.contains(&uri) {
                // 未登记 / 应用未启用：绝不重定向到这个地址（防开放重定向）。
                return logout_error("退出后回跳地址未在该应用中登记，FloatCTF 不会跳转到该地址。");
            }
            Some(uri)
        }
    };

    let secure = oidc.secure_cookies();
    let mut cookies = Vec::new();
    let binding = match req
        .cookie(binding_cookie_name(secure))
        .map(|c| c.value().to_string())
        .filter(|v| secret::is_plausible_token(v))
    {
        Some(v) => v,
        None => {
            let v = flow::new_browser_binding();
            cookies.push(build_cookie(
                binding_cookie_name(secure),
                v.clone(),
                BINDING_COOKIE_MAX_AGE_SECS,
                secure,
            ));
            v
        }
    };
    let result = async {
        let browser = accounts::load(&db, &oidc, &req).await?;
        let hint_sid = hint.as_ref().and_then(|h| h.sid.clone());
        let hint_sub = hint.as_ref().map(|h| h.sub.clone());
        let found = targets(&browser, hint_sid.as_deref(), hint_sub.as_deref());
        let affected = backchannel::affected_clients(
            &db,
            &found.iter().map(|(s, _)| s.id).collect::<Vec<_>>(),
        )
        .await?;
        let request = if found.is_empty() {
            None
        } else {
            Some(
                store::create_logout_request(
                    &db,
                    store::NewLogoutRequest {
                        browser_binding: &binding,
                        client_pk: client.as_ref().map(|c| c.id),
                        hint_sub,
                        hint_sid,
                        post_logout_redirect_uri: redirect_to.clone(),
                        state: state.clone(),
                        ttl_secs: LOGOUT_REQUEST_TTL_SECS,
                    },
                )
                .await?,
            )
        };
        Ok::<_, crate::api::AppError>((browser, found, affected, request))
    }
    .await;
    let (browser, found, affected, request) = match result {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "end_session failed");
            return server_error_page();
        }
    };
    cookies.extend(browser.cookies);
    let client_name = client.as_ref().map(|c| c.name.clone());
    let form_origins = redirect_to
        .as_deref()
        .and_then(pages::origin_of)
        .into_iter()
        .collect::<Vec<_>>();
    // 无事可做时回到应用但**不带 state**：RP 据此知道 FloatCTF 侧没有发生退出。
    let continue_url = redirect_to.as_deref().map(|u| back_to_client(u, None));

    let mut resp = match request {
        // 本浏览器中没有与之匹配的 FloatCTF 登录：无事可做（也不会去动别的浏览器/设备）。
        None => pages::html(
            StatusCode::OK,
            "logged_out.html",
            minijinja::context! {
                nothing_to_do => true,
                client_name => client_name,
                continue_url => continue_url,
            },
            &form_origins,
        ),
        Some((request, csrf)) => pages::html(
            StatusCode::OK,
            "end_session.html",
            minijinja::context! {
                action => PATH_END_SESSION_CONFIRM,
                request => request.id.to_string(),
                csrf => csrf,
                client_name => client_name,
                accounts => found.iter().map(chip).collect::<Vec<_>>(),
                notified => affected.iter().filter(|a| a.supports_backchannel).collect::<Vec<_>>(),
                unsupported => affected.iter().filter(|a| !a.supports_backchannel).collect::<Vec<_>>(),
                all_accounts => hint.is_none(),
            },
            &form_origins,
        ),
    };
    for cookie in &cookies {
        let _ = resp.add_cookie(cookie);
    }
    resp
}

/// 撤销本次退出作用的会话（多账号：逐个或整个容器；旧版单会话：撤销并清除 Cookie）。
pub(crate) async fn revoke_for_logout(
    db: &sea_orm::DatabaseConnection,
    oidc: &OidcProvider,
    browser: &mut Browser,
    targets: &[SessionUser],
) -> Result<(), crate::api::AppError> {
    if oidc.config.multi_account {
        if !targets.is_empty() && targets.len() == browser.accounts.len() {
            accounts::sign_out_browser(db, oidc, browser, RevokeReason::GlobalLogout).await?;
        } else {
            for (session, _) in targets {
                accounts::sign_out_account(db, browser, session.id, RevokeReason::GlobalLogout)
                    .await?;
            }
        }
    } else {
        let ids: Vec<Uuid> = targets.iter().map(|(s, _)| s.id).collect();
        accounts::revoke_sessions(db, &ids, RevokeReason::GlobalLogout).await?;
        browser.accounts.retain(|(s, _)| !ids.contains(&s.id));
        browser.cookies.push(removal_cookie(
            sso_cookie_name(oidc.secure_cookies()),
            oidc.secure_cookies(),
        ));
    }
    Ok(())
}

/// 「退出所有关联应用」的执行结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct GlobalLogoutReport {
    /// 每个已接入标准退出的应用的真实投递结果。
    pub deliveries: Vec<DeliveryOutcome>,
    /// 拿到过该会话令牌、但没有登记 Back-Channel 的应用：它们的会话可能仍然有效。
    pub without_backchannel: Vec<String>,
}

impl GlobalLogoutReport {
    pub fn fully_synced(&self) -> bool {
        self.deliveries.iter().all(|d| d.status == "delivered")
            && self.without_backchannel.is_empty()
    }
}

/// 撤销会话并通知关联应用（调用方已取得用户明确确认）。
pub(crate) async fn global_logout(
    db: &sea_orm::DatabaseConnection,
    oidc: &OidcProvider,
    browser: &mut Browser,
    targets: &[SessionUser],
    reason: Reason,
) -> Result<GlobalLogoutReport, crate::api::AppError> {
    let affected: Vec<AffectedClient> =
        backchannel::affected_clients(db, &targets.iter().map(|(s, _)| s.id).collect::<Vec<_>>())
            .await?;
    revoke_for_logout(db, oidc, browser, targets).await?;
    let (_, rows) = backchannel::enqueue(db, oidc.issuer(), targets, reason).await?;
    let deliveries = backchannel::deliver_batch(db, oidc, &rows).await;
    Ok(GlobalLogoutReport {
        deliveries,
        without_backchannel: affected
            .into_iter()
            .filter(|a| !a.supports_backchannel)
            .map(|a| a.name)
            .collect(),
    })
}

/// POST /oauth/end-session/confirm（action = logout | cancel）
#[post("/oauth/end-session/confirm")]
pub async fn end_session_confirm(
    req: HttpRequest,
    body: web::Bytes,
    oidc: Option<WebOidc>,
    db: WebDb,
    log: WebLog,
) -> HttpResponse {
    let Some(oidc) = oidc.filter(|o| o.config.backchannel_logout) else {
        return not_enabled();
    };
    if !is_form(&req) || !accounts::first_party_request(&req, &oidc) {
        return logout_error("请求来源无效。");
    }
    let form = parse_pairs(&body);
    let field = |name: &str| {
        form.single(name)
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_string()
    };
    let Ok(request_id) = Uuid::parse_str(field("request").trim()) else {
        return logout_error("退出请求已失效，请回到应用重新操作。");
    };
    let binding = req
        .cookie(binding_cookie_name(oidc.secure_cookies()))
        .map(|c| c.value().to_string());
    let request = match store::complete_logout_request(
        &db,
        request_id,
        binding.as_deref(),
        &field("csrf"),
    )
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return logout_error(
                "退出请求已失效（已完成、已超时、页面过期或不是在本浏览器中发起），请回到应用重新操作。",
            );
        }
        Err(_) => return server_error_page(),
    };
    let client = match request.oidc_client_id {
        Some(id) => crate::entity::oidc_clients::Entity::find_by_id(id)
            .one(db.get_ref())
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let client_name = client.as_ref().map(|c| c.name.clone());
    // 回跳前再确认一次仍在登记列表中（确认页停留期间应用可能修改了登记）。
    let registered_redirect = request.post_logout_redirect_uri.as_deref().filter(|uri| {
        client.as_ref().is_some_and(|c| {
            clients::post_logout_redirect_uris_of(c)
                .iter()
                .any(|u| u == uri)
        })
    });
    // 只有真正执行了退出才带回 state；取消时不带（RP 据此不会误报「已在 FloatCTF 退出」）。
    let continue_url = registered_redirect.map(|uri| back_to_client(uri, request.state.as_deref()));
    let cancel_url = registered_redirect.map(|uri| back_to_client(uri, None));
    let form_origins = request
        .post_logout_redirect_uri
        .as_deref()
        .and_then(pages::origin_of)
        .into_iter()
        .collect::<Vec<_>>();

    if field("action") != "logout" {
        audit(
            &log,
            "INFO",
            "OIDC_END_SESSION_CANCEL",
            "用户取消了应用发起的退出".to_string(),
            json!({"client": client.as_ref().map(|c| c.client_id.clone())}),
            None,
            &req,
        );
        return pages::html(
            StatusCode::OK,
            "logged_out.html",
            minijinja::context! { cancelled => true, client_name => client_name, continue_url => cancel_url },
            &form_origins,
        );
    }

    let result = async {
        let mut browser = accounts::load(&db, &oidc, &req).await?;
        let found = targets(
            &browser,
            request.hint_sid.as_deref(),
            request.hint_sub.as_deref(),
        );
        let report = global_logout(&db, &oidc, &mut browser, &found, Reason::EndSession).await?;
        Ok::<_, crate::api::AppError>((browser, found, report))
    }
    .await;
    let (browser, found, report) = match result {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "end_session confirm failed");
            return pages::error_page(
                500,
                "退出未完成：服务暂时不可用，你的 FloatCTF 登录可能仍然有效，请重试。",
            );
        }
    };
    for (_, user) in &found {
        audit(
            &log,
            "INFO",
            "OIDC_GLOBAL_LOGOUT",
            format!("{} 确认退出 FloatCTF 及关联应用（应用发起）", user.username),
            json!({
                "client": client.as_ref().map(|c| c.client_id.clone()),
                "deliveries": report.deliveries,
                "without_backchannel": report.without_backchannel,
            }),
            Some(user.id),
            &req,
        );
    }
    let mut resp = match (&continue_url, report.fully_synced()) {
        (Some(url), true) => redirect(url).finish(),
        _ => pages::html(
            StatusCode::OK,
            "logged_out.html",
            minijinja::context! {
                client_name => client_name,
                continue_url => continue_url,
                synced => report.fully_synced(),
                deliveries => report.deliveries,
                without_backchannel => report.without_backchannel,
                signed_out => found.iter().map(chip).collect::<Vec<_>>(),
            },
            &form_origins,
        ),
    };
    for cookie in &browser.cookies {
        let _ = resp.add_cookie(cookie);
    }
    resp
}
