//! OIDC 授权码流程编排（应用层）：授权决策、发码、令牌兑换、UserInfo。
//!
//! HTTP 细节（Cookie、表单、页面）在 `api::protocol`；这里只依赖数据库、配置与 key ring，
//! 返回"下一步做什么"，便于集成测试逐步断言。

use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::Serialize;
use uuid::Uuid;

use crate::api::AppError;
use crate::entity::sea_orm_active_enums::{OidcClientStatus, OidcClientType};
use crate::entity::{oidc_auth_transactions, oidc_browser_sessions, oidc_clients, users};

use super::OidcProvider;
use super::clients;
use super::domain::authorize::{self, AuthorizationRequest, Prompt, RawParams};
use super::domain::claims::{self, IdTokenClaims, UserSnapshot};
use super::domain::client_auth::PresentedClientAuth;
use super::domain::error::OAuthError;
use super::domain::metadata::{PATH_CONSENT, PATH_LOGIN, PATH_SELECT_ACCOUNT};
use super::domain::{pkce, redirect_uri, scope, secret};
use super::store;

/// 授权端点/页面处理后的下一步。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// 302 到已验证的 RP 回调（成功或标准错误）。
    RedirectToClient(String),
    /// 302 到本 OP 的登录/同意页。
    RedirectLocal(String),
    /// 渲染 FloatCTF 错误页（**不重定向**：client/redirect_uri 未验证或事务失效）。
    ErrorPage { status: u16, message: String },
}

impl Next {
    fn error_page(status: u16, message: impl Into<String>) -> Self {
        Self::ErrorPage {
            status,
            message: message.into(),
        }
    }
}

/// 当前浏览器带来的 SSO 会话（若有效）。
pub type SessionUser = (oidc_browser_sessions::Model, users::Model);

pub const TXN_EXPIRED_MESSAGE: &str =
    "授权请求已过期或无效（可能已完成、已超时，或不是在本浏览器中发起）。请返回应用重新登录。";

/// 带 `iss`（RFC 9207）的授权响应重定向。
fn client_redirect(oidc: &OidcProvider, redirect: &str, params: &[(&str, &str)]) -> String {
    let mut all: Vec<(&str, &str)> = params.to_vec();
    all.push(("iss", oidc.issuer()));
    redirect_uri::append_query(redirect, &all)
}

fn error_redirect(
    oidc: &OidcProvider,
    redirect: &str,
    err: &OAuthError,
    state: Option<&str>,
) -> Next {
    let mut params = vec![
        ("error", err.error),
        ("error_description", err.description.as_str()),
    ];
    if let Some(state) = state {
        params.push(("state", state));
    }
    Next::RedirectToClient(client_redirect(oidc, redirect, &params))
}

/// `GET|POST /oauth/authorize`：校验请求并创建授权事务，然后推进。
pub async fn start(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    params: &RawParams,
    browser_binding: &str,
    accounts: &[SessionUser],
) -> Result<Next, AppError> {
    let (client_id, redirect) = match authorize::client_and_redirect(params) {
        Ok(v) => v,
        Err(e) => return Ok(Next::error_page(400, e.message())),
    };
    let client = match clients::find_by_client_id(db, &client_id).await? {
        Some(c) if c.status == OidcClientStatus::Active => c,
        Some(_) => {
            return Ok(Next::error_page(
                400,
                "该应用当前未启用（待审批、已暂停或已被封禁）。",
            ));
        }
        None => return Ok(Next::error_page(400, "应用不存在（client_id 无效）。")),
    };
    if !redirect_uri::is_registered(&clients::redirect_uris_of(&client), &redirect) {
        return Ok(Next::error_page(
            400,
            "回调地址（redirect_uri）与该应用登记的地址不一致。",
        ));
    }

    // 从这里开始回调已验证，错误按规范回传给 RP。
    let state = params.echo_state();
    let policy = authorize::ClientPolicy {
        allowed_scopes: clients::scopes_of(&client),
        require_pkce: client.require_pkce || client.client_type == OidcClientType::Public,
    };
    let request = match authorize::validate(params, &client_id, &redirect, &policy) {
        Ok(r) => r,
        Err(e) => return Ok(error_redirect(oidc, &redirect, &e, state.as_deref())),
    };
    let hint_sub = match &request.id_token_hint {
        None => None,
        Some(hint) => match oidc.keys.verify::<IdTokenClaims>(hint, oidc.issuer(), true) {
            Ok(c) => Some(c.sub),
            Err(_) => {
                return Ok(error_redirect(
                    oidc,
                    &redirect,
                    &OAuthError::invalid_request("invalid id_token_hint"),
                    request.state.as_deref(),
                ));
            }
        },
    };
    let txn = store::create_transaction(
        db,
        &client,
        &request,
        hint_sub,
        browser_binding,
        oidc.config.transaction_ttl_secs,
    )
    .await?;
    advance(db, oidc, txn, &client, accounts).await
}

fn txn_prompt(txn: &oidc_auth_transactions::Model) -> Prompt {
    Prompt::from_values(&scope::from_json(&txn.prompt))
}

fn ts(t: &DateTime<FixedOffset>) -> i64 {
    t.timestamp()
}

// ── 授权事务状态机 ──────────────────────────────────────────────────────────

/// 授权事务的下一步。由 [`decide`] 纯函数给出，[`advance`] 负责执行。
///
/// 阶段顺序固定为「账号选择 → 认证 → 授权同意 → 发码」，三种交互页互相独立：
/// `select_account` 只决定是否展示账号选择页，绝不会导向同意页。
/// 浏览器里有多个可用账号且本事务尚未选定时，无论 prompt 为何都先选择账号（`prompt=none` → 标准错误）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// 输入密码：无可用 SSO，或 `prompt=login` / `max_age` 要求重新认证。
    Login,
    /// 有可用 SSO 且 `prompt=select_account`：等待用户在账号选择页明确选择。
    SelectAccount,
    /// 需要授权同意（`prompt=consent` 或已有授权不覆盖本次 scope）。
    Consent,
    /// 条件全部满足，直接发码。
    Issue,
    /// `prompt=none` 下无法无交互完成，返回对应标准错误（永不渲染页面）。
    Required(Required),
}

/// `prompt=none` 时的标准错误（OIDC Core §3.1.2.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Required {
    Login,
    AccountSelection,
    Consent,
}

impl Required {
    fn error(self) -> OAuthError {
        match self {
            Self::Login => OAuthError::login_required("login required"),
            Self::AccountSelection => {
                OAuthError::account_selection_required("account selection required")
            }
            Self::Consent => OAuthError::consent_required("consent required"),
        }
    }
}

/// [`decide`] 的输入：当前可用 SSO 会话在本事务中的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionFacts {
    /// 本事务内用户已确认该会话（选择页「继续」或本事务内输入密码）。
    pub confirmed_in_txn: bool,
    /// 本事务内刚为该会话输入过密码（满足 `prompt=login` / `max_age`）。
    pub fresh_in_txn: bool,
    /// 会话的认证时间（Unix 秒）。
    pub auth_time: i64,
    /// 用户对该应用的有效授权覆盖本次申请的 scope。
    pub consented: bool,
}

/// 纯决策：给定 prompt / max_age 与（经 [`usable_session`] 过滤后的）会话状态，返回下一步。
///
/// `ambiguous`：浏览器有多个可用账号、本事务尚未选定其中之一（此时 `session` 为 `None`）。
pub fn decide(
    prompt: &Prompt,
    max_age: Option<i64>,
    account_chooser: bool,
    session: Option<SessionFacts>,
    ambiguous: bool,
    now: i64,
) -> Step {
    let Some(s) = session else {
        if ambiguous {
            // 多个账号时绝不替用户挑选（不按最近使用、不按创建顺序）。
            return if prompt.none {
                Step::Required(Required::AccountSelection)
            } else {
                Step::SelectAccount
            };
        }
        // 没有可用 SSO：输入密码本身就是本次身份选择，不再额外弹选择页。
        return if prompt.none {
            Step::Required(Required::Login)
        } else {
            Step::Login
        };
    };
    if account_chooser && prompt.select_account && !s.confirmed_in_txn {
        // prompt=none 与 select_account 在解析期就互斥；这里防御性地返回规范错误。
        return if prompt.none {
            Step::Required(Required::AccountSelection)
        } else {
            Step::SelectAccount
        };
    }
    if !s.fresh_in_txn && authorize::requires_fresh_login(prompt, max_age, s.auth_time, now) {
        return if prompt.none {
            Step::Required(Required::Login)
        } else {
            Step::Login
        };
    }
    // 回滚模式（account_chooser = false）保留旧语义：select_account 导向同意页。
    let legacy_select_account = !account_chooser && prompt.select_account;
    if prompt.consent || legacy_select_account || !s.consented {
        return if prompt.none {
            Step::Required(Required::Consent)
        } else {
            Step::Consent
        };
    }
    Step::Issue
}

/// 本事务内用户是否已确认该会话。
pub fn account_confirmed(txn: &oidc_auth_transactions::Model, session_id: Uuid) -> bool {
    txn.account_selected_at.is_some() && txn.account_selected_session_id == Some(session_id)
}

/// 当前浏览器的 SSO 会话能否用于本事务：
/// - `id_token_hint` 指向别的用户 → 不可用；
/// - 用户已选择「使用其他账号」，但该会话不是本事务内重新输入密码得到的 → 不可用
///   （旧 Cookie 不能把「使用其他账号」短路回原账号）。
pub fn usable_session(
    txn: &oidc_auth_transactions::Model,
    session: Option<SessionUser>,
) -> Option<SessionUser> {
    session.filter(|s| is_usable(txn, s))
}

fn is_usable(txn: &oidc_auth_transactions::Model, (s, user): &SessionUser) -> bool {
    let hint_ok = txn
        .id_token_hint_sub
        .as_deref()
        .is_none_or(|sub| sub == user.id.to_string());
    let switch_ok = txn.account_switch_requested_at.is_none() || account_confirmed(txn, s.id);
    hint_ok && switch_ok
}

/// 本浏览器账号中可用于本事务的候选（顺序与输入一致：最近使用在前）。
pub fn candidates(
    txn: &oidc_auth_transactions::Model,
    accounts: &[SessionUser],
) -> Vec<SessionUser> {
    accounts
        .iter()
        .filter(|s| is_usable(txn, s))
        .cloned()
        .collect()
}

/// 从候选中确定本事务的会话：
/// 1. 本事务内已确认 / 刚输入密码得到的会话优先；
/// 2. 只有一个候选 → 就是它（单账号浏览器与旧版行为一致）；
/// 3. 多个候选：`login_hint` 恰好等于其中一个用户名 → 它；否则无法确定（`ambiguous = true`）。
pub fn resolve_session(
    txn: &oidc_auth_transactions::Model,
    candidates: &[SessionUser],
) -> (Option<SessionUser>, bool) {
    if let Some(bound) = candidates.iter().find(|(s, _)| {
        account_confirmed(txn, s.id)
            || (txn.authenticated_at.is_some() && txn.session_id == Some(s.id))
    }) {
        return (Some(bound.clone()), false);
    }
    match candidates {
        [] => (None, false),
        [only] => (Some(only.clone()), false),
        many => {
            let hinted: Vec<_> = txn
                .login_hint
                .as_deref()
                .map(|hint| many.iter().filter(|(_, u)| u.username == hint).collect())
                .unwrap_or_default();
            match hinted.as_slice() {
                [one] => (Some((*one).clone()), false),
                _ => (None, true),
            }
        }
    }
}

/// [`evaluate`] 的结果：下一步、本事务确定的会话、可供选择的候选账号。
#[derive(Debug, Clone)]
pub struct Evaluation {
    pub step: Step,
    pub session: Option<SessionUser>,
    pub candidates: Vec<SessionUser>,
}

/// 收集决策所需事实并计算下一步。
pub async fn evaluate(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    txn: &oidc_auth_transactions::Model,
    client: &oidc_clients::Model,
    accounts: &[SessionUser],
) -> Result<Evaluation, AppError> {
    let candidates = candidates(txn, accounts);
    let (session, ambiguous) = resolve_session(txn, &candidates);
    let facts = match &session {
        None => None,
        Some((s, user)) => {
            let requested = scope::from_json(&txn.scopes);
            let consented = store::find_active_consent(db, user.id, client.id)
                .await?
                .is_some_and(|c| scope::covers(&scope::from_json(&c.scopes), &requested));
            Some(SessionFacts {
                confirmed_in_txn: account_confirmed(txn, s.id),
                fresh_in_txn: txn.authenticated_at.is_some() && txn.session_id == Some(s.id),
                auth_time: ts(&s.auth_time),
                consented,
            })
        }
    };
    let step = decide(
        &txn_prompt(txn),
        txn.max_age.map(i64::from),
        oidc.config.account_chooser,
        facts,
        ambiguous,
        Utc::now().timestamp(),
    );
    Ok(Evaluation {
        step,
        session,
        candidates,
    })
}

/// 推进事务：执行 [`decide`] 的结果。`prompt=none` 时不展示任何页面。
pub async fn advance(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    txn: oidc_auth_transactions::Model,
    client: &oidc_clients::Model,
    accounts: &[SessionUser],
) -> Result<Next, AppError> {
    let Evaluation { step, session, .. } = evaluate(db, oidc, &txn, client, accounts).await?;
    match step {
        Step::Required(required) => finish_with_error(db, oidc, &txn, required.error()).await,
        Step::Login => Ok(Next::RedirectLocal(format!("{PATH_LOGIN}?txn={}", txn.id))),
        Step::SelectAccount => Ok(Next::RedirectLocal(format!(
            "{PATH_SELECT_ACCOUNT}?txn={}",
            txn.id
        ))),
        Step::Consent | Step::Issue => {
            let (session, user) = session.expect("decide: Consent/Issue require a usable session");
            let txn = if txn.user_id != Some(user.id) || txn.session_id != Some(session.id) {
                let fresh = txn.authenticated_at.is_some() && txn.session_id == Some(session.id);
                store::bind_transaction_user(db, txn, user.id, session.id, fresh).await?
            } else {
                txn
            };
            if step == Step::Consent {
                return Ok(Next::RedirectLocal(format!(
                    "{PATH_CONSENT}?txn={}",
                    txn.id
                )));
            }
            // 已有授权覆盖本次 scope：直接发码（只发本次申请的 scope）。
            let requested = scope::from_json(&txn.scopes);
            issue_code(db, oidc, &txn, client, &session, &user, &requested).await
        }
    }
}

/// 同意页可以展示/提交的前提：状态机确实停在同意阶段，且事务已绑定到当前会话。
///
/// 否则（会话变化、跳过了账号选择、用户已要求切换账号……）调用方应改走 [`advance`]。
pub async fn consent_ready(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    txn: &oidc_auth_transactions::Model,
    client: &oidc_clients::Model,
    accounts: &[SessionUser],
) -> Result<Option<SessionUser>, AppError> {
    let Evaluation { step, session, .. } = evaluate(db, oidc, txn, client, accounts).await?;
    Ok(session.filter(|(s, user)| {
        step == Step::Consent && txn.user_id == Some(user.id) && txn.session_id == Some(s.id)
    }))
}

/// 结束事务并把错误带回 RP。
pub async fn finish_with_error(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    txn: &oidc_auth_transactions::Model,
    err: OAuthError,
) -> Result<Next, AppError> {
    if !store::complete_transaction(db, txn.id).await? {
        return Ok(Next::error_page(400, TXN_EXPIRED_MESSAGE));
    }
    Ok(error_redirect(
        oidc,
        &txn.redirect_uri,
        &err,
        txn.state.as_deref(),
    ))
}

/// 发码并结束事务。事务只能完成一次（双击/重放返回错误页）。
pub async fn issue_code(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    txn: &oidc_auth_transactions::Model,
    client: &oidc_clients::Model,
    session: &oidc_browser_sessions::Model,
    user: &users::Model,
    granted: &[String],
) -> Result<Next, AppError> {
    if !store::complete_transaction(db, txn.id).await? {
        return Ok(Next::error_page(400, TXN_EXPIRED_MESSAGE));
    }
    let code = store::insert_code(
        db,
        store::NewCode {
            client_pk: client.id,
            user_id: user.id,
            session_id: Some(session.id),
            redirect_uri: &txn.redirect_uri,
            scopes: granted,
            nonce: txn.nonce.clone(),
            code_challenge: txn.code_challenge.clone(),
            auth_time: session.auth_time,
            ttl_secs: oidc.config.authorization_code_ttl_secs,
        },
    )
    .await?;
    store::touch_session(db, session.id).await?;
    let mut params = vec![("code", code.as_str())];
    if let Some(state) = txn.state.as_deref() {
        params.push(("state", state));
    }
    Ok(Next::RedirectToClient(client_redirect(
        oidc,
        &txn.redirect_uri,
        &params,
    )))
}

/// 用户在同意页的选择 → 实际授予的 scope：`openid` 必含，其余取"本次申请 ∩ 勾选"。
pub fn granted_from_selection(requested: &[String], selected: &[String]) -> Vec<String> {
    requested
        .iter()
        .filter(|s| s.as_str() == scope::OPENID || selected.contains(s))
        .cloned()
        .collect()
}

// ── Token 端点 ──────────────────────────────────────────────────────────────

/// Token 端点成功响应（RFC 6749 §5.1 + OIDC Core §3.1.3.3）。
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub id_token: String,
    pub scope: String,
}

/// Token 端点失败：错误 + 是否使用了 Basic（决定 `WWW-Authenticate`）。
#[derive(Debug)]
pub struct TokenFailure {
    pub error: OAuthError,
    pub status: u16,
}

impl TokenFailure {
    fn new(error: OAuthError) -> Self {
        let status = if error.error == "invalid_client" {
            401
        } else {
            400
        };
        Self { error, status }
    }
}

impl From<OAuthError> for TokenFailure {
    fn from(error: OAuthError) -> Self {
        Self::new(error)
    }
}

/// 客户端认证：认证方式必须与登记一致；机密客户端校验 secret（含轮换宽限期）。
pub async fn authenticate_client(
    db: &DatabaseConnection,
    presented: &PresentedClientAuth,
) -> Result<oidc_clients::Model, TokenFailure> {
    let invalid = || TokenFailure::new(OAuthError::invalid_client("client authentication failed"));
    let client = clients::find_by_client_id(db, presented.client_id())
        .await
        .map_err(|_| TokenFailure {
            error: OAuthError::server_error("temporarily unavailable"),
            status: 500,
        })?
        .ok_or_else(invalid)?;
    if client.status != OidcClientStatus::Active {
        return Err(invalid());
    }
    let registered = match client.token_endpoint_auth_method {
        crate::entity::sea_orm_active_enums::OidcTokenAuthMethod::ClientSecretBasic => {
            "client_secret_basic"
        }
        crate::entity::sea_orm_active_enums::OidcTokenAuthMethod::ClientSecretPost => {
            "client_secret_post"
        }
        crate::entity::sea_orm_active_enums::OidcTokenAuthMethod::None => "none",
    };
    if presented.method() != registered {
        return Err(invalid());
    }
    match presented {
        PresentedClientAuth::Basic { secret, .. } | PresentedClientAuth::Post { secret, .. } => {
            if !clients::verify_secret(&client, secret) {
                return Err(invalid());
            }
        }
        PresentedClientAuth::None { .. } => {
            if client.client_type != OidcClientType::Public {
                return Err(invalid());
            }
        }
    }
    Ok(client)
}

/// `grant_type=authorization_code` 兑换。客户端已通过认证。
pub async fn exchange_code(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    client: &oidc_clients::Model,
    params: &RawParams,
) -> Result<TokenResponse, TokenFailure> {
    let single = |name: &str| {
        params.single(name).map_err(|_| {
            TokenFailure::new(OAuthError::invalid_request(format!(
                "duplicate parameter: {name}"
            )))
        })
    };
    match single("grant_type")? {
        None => return Err(OAuthError::invalid_request("missing grant_type").into()),
        Some("authorization_code") => {}
        Some(_) => {
            return Err(
                OAuthError::unsupported_grant_type("only authorization_code is supported").into(),
            );
        }
    }
    let code_value = single("code")?
        .ok_or_else(|| TokenFailure::new(OAuthError::invalid_request("missing code")))?;
    let redirect = single("redirect_uri")?;
    let verifier = single("code_verifier")?;
    let invalid_grant =
        || TokenFailure::new(OAuthError::invalid_grant("invalid authorization code"));
    let db_error = |_| TokenFailure {
        error: OAuthError::server_error("temporarily unavailable"),
        status: 500,
    };

    let code = store::find_code(db, code_value)
        .await
        .map_err(db_error)?
        .ok_or_else(invalid_grant)?;
    // 绑定校验先于消费：错误的 client 拿到被盗的 code 也无法让合法 client 的兑换失败。
    if code.oidc_client_id != client.id {
        return Err(invalid_grant());
    }
    if code.consumed_at.is_some() {
        // 重放：撤销该授权码已签发的令牌（RFC 6749 §4.1.2 / RFC 9700 §4.5.3.1）。
        let revoked = store::revoke_tokens_of_code(db, code.id)
            .await
            .map_err(db_error)?;
        tracing::warn!(client_id = %client.client_id, revoked, "OIDC authorization code replay detected");
        return Err(invalid_grant());
    }
    if code.expires_at.with_timezone(&Utc) <= Utc::now() {
        return Err(invalid_grant());
    }
    match redirect {
        None => return Err(OAuthError::invalid_request("missing redirect_uri").into()),
        Some(r) if r != code.redirect_uri => return Err(invalid_grant()),
        Some(_) => {}
    }
    match (&code.code_challenge, verifier) {
        (Some(challenge), Some(v)) => {
            if !pkce::verify_s256(v, challenge) {
                return Err(TokenFailure::new(OAuthError::invalid_grant(
                    "PKCE verification failed",
                )));
            }
        }
        (Some(_), None) => {
            return Err(TokenFailure::new(OAuthError::invalid_grant(
                "code_verifier required",
            )));
        }
        // 防 PKCE 降级：发码时没有 challenge，兑换时却带 verifier → 拒绝（RFC 9700 §2.1.1）。
        (None, Some(_)) => {
            return Err(TokenFailure::new(OAuthError::invalid_grant(
                "code_verifier sent but no code_challenge was used",
            )));
        }
        (None, None) => {}
    }
    let user = users::Entity::find_by_id(code.user_id)
        .one(db)
        .await
        .map_err(|e| db_error(AppError::from(e)))?
        .ok_or_else(invalid_grant)?;
    let consent = store::find_active_consent(db, user.id, client.id)
        .await
        .map_err(db_error)?;
    let granted = scope::from_json(&code.scopes);
    if !consent.is_some_and(|c| scope::covers(&scope::from_json(&c.scopes), &granted)) {
        return Err(invalid_grant());
    }

    let Some((access_token, _)) =
        store::consume_code_and_issue_token(db, &code, oidc.config.access_token_ttl_secs)
            .await
            .map_err(db_error)?
    else {
        // 并发兑换中落败 / 恰好过期：按重放处理。
        let _ = store::revoke_tokens_of_code(db, code.id).await;
        return Err(invalid_grant());
    };

    // 记下「这个身份会话向这个应用签发过令牌」：用户明确「退出所有关联应用」时据此通知 RP。
    // 记录失败不影响登录（只会让该次全局退出少通知一个应用，并在结果页如实显示）。
    let mut sid = None;
    if let Some(session_id) = code.session_id {
        if let Err(e) = store::record_session_client(db, session_id, client.id).await {
            tracing::warn!(error = %e, "failed to record session ↔ client binding");
        }
        if oidc.config.backchannel_logout {
            sid = store::session_sid(db, session_id).await.unwrap_or(None);
        }
    }

    let now = Utc::now().timestamp();
    let id_claims = IdTokenClaims {
        iss: oidc.issuer().to_string(),
        sub: user.id.to_string(),
        aud: client.client_id.clone(),
        azp: client.client_id.clone(),
        exp: now + oidc.config.id_token_ttl_secs,
        iat: now,
        auth_time: code.auth_time.timestamp(),
        nonce: code.nonce.clone(),
        at_hash: Some(claims::at_hash(&access_token)),
        sid,
    };
    let id_token = oidc.keys.sign(&id_claims).map_err(|e| {
        tracing::error!(error = %e, "OIDC id_token signing failed");
        TokenFailure {
            error: OAuthError::server_error("token signing unavailable"),
            status: 500,
        }
    })?;
    Ok(TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in: oidc.config.access_token_ttl_secs,
        id_token,
        scope: scope::join(&granted),
    })
}

pub fn snapshot(user: &users::Model) -> UserSnapshot {
    UserSnapshot {
        id: user.id,
        username: user.username.clone(),
        nickname: user.nickname.clone(),
        email: user.email.clone(),
        avatar: user.avatar.clone(),
        updated_at: user.updated_at.timestamp(),
    }
}

/// UserInfo：返回 `None` 表示令牌不可用（401 invalid_token）。
pub async fn userinfo(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    access_token: &str,
) -> Result<Option<serde_json::Map<String, serde_json::Value>>, AppError> {
    let Some(ctx) = store::find_usable_token(db, access_token).await? else {
        return Ok(None);
    };
    let scopes = scope::from_json(&ctx.token.scopes);
    Ok(Some(claims::userinfo(
        &snapshot(&ctx.user),
        &scopes,
        oidc.issuer(),
    )))
}

/// 由 SSO Cookie 解析当前会话。
pub async fn current_session(
    db: &DatabaseConnection,
    cookie_value: Option<&str>,
) -> Result<Option<SessionUser>, AppError> {
    match cookie_value {
        Some(v) => store::find_valid_session(db, v).await,
        None => Ok(None),
    }
}

/// 解析页面表单中的事务 id。
pub fn parse_txn_id(raw: Option<&str>) -> Option<Uuid> {
    raw.and_then(|v| Uuid::parse_str(v).ok())
}

/// 便于测试的类型别名：授权请求校验结果。
pub type ValidatedRequest = AuthorizationRequest;

/// 生成新的浏览器绑定值。
pub fn new_browser_binding() -> String {
    secret::new_opaque_secret()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn consent_selection_keeps_openid_and_never_adds_unrequested() {
        let requested = s(&["openid", "profile", "email"]);
        assert_eq!(granted_from_selection(&requested, &[]), s(&["openid"]));
        assert_eq!(
            granted_from_selection(&requested, &s(&["email"])),
            s(&["openid", "email"])
        );
        // 勾选未申请的 scope 不会被授予
        assert_eq!(
            granted_from_selection(&s(&["openid"]), &s(&["profile", "email"])),
            s(&["openid"])
        );
    }

    // ── decide()：prompt 组合矩阵 ──────────────────────────────────────────

    const NOW: i64 = 1_800_000_000;

    fn prompt(raw: &str) -> Prompt {
        Prompt::parse(Some(raw)).unwrap()
    }

    /// 有效 SSO：一小时前认证、已同意、本事务内未确认 / 未重新认证。
    fn sso() -> SessionFacts {
        SessionFacts {
            confirmed_in_txn: false,
            fresh_in_txn: false,
            auth_time: NOW - 3600,
            consented: true,
        }
    }

    fn step(raw: &str, session: Option<SessionFacts>) -> Step {
        decide(&prompt(raw), None, true, session, false, NOW)
    }

    #[test]
    fn no_prompt_keeps_legacy_silent_sso() {
        assert_eq!(
            step("", Some(sso())),
            Step::Issue,
            "已登录且已同意的 RP 不受影响"
        );
        assert_eq!(step("", None), Step::Login);
        let not_consented = SessionFacts {
            consented: false,
            ..sso()
        };
        assert_eq!(step("", Some(not_consented)), Step::Consent);
    }

    #[test]
    fn select_account_shows_chooser_before_anything_else_and_never_consent() {
        assert_eq!(step("select_account", Some(sso())), Step::SelectAccount);
        // 无 SSO：直接登录页，不多一层空选择页
        assert_eq!(step("select_account", None), Step::Login);
        // 选择后：已同意 → 直接发码，不重复同意
        let confirmed = SessionFacts {
            confirmed_in_txn: true,
            ..sso()
        };
        assert_eq!(step("select_account", Some(confirmed)), Step::Issue);
        // 选择后：未同意 → 同意页
        let fresh_user = SessionFacts {
            confirmed_in_txn: true,
            consented: false,
            ..sso()
        };
        assert_eq!(step("select_account", Some(fresh_user)), Step::Consent);
    }

    #[test]
    fn select_account_combines_with_consent_and_login_as_separate_steps() {
        let confirmed = SessionFacts {
            confirmed_in_txn: true,
            ..sso()
        };
        assert_eq!(
            step("select_account consent", Some(sso())),
            Step::SelectAccount
        );
        assert_eq!(
            step("select_account consent", Some(confirmed)),
            Step::Consent
        );
        assert_eq!(
            step("select_account login", Some(sso())),
            Step::SelectAccount
        );
        assert_eq!(step("select_account login", Some(confirmed)), Step::Login);
        let reauthenticated = SessionFacts {
            confirmed_in_txn: true,
            fresh_in_txn: true,
            ..sso()
        };
        assert_eq!(
            step("select_account login", Some(reauthenticated)),
            Step::Issue
        );
    }

    #[test]
    fn login_and_consent_prompts_do_not_show_chooser() {
        assert_eq!(step("login", Some(sso())), Step::Login);
        assert_eq!(step("consent", Some(sso())), Step::Consent);
        let fresh = SessionFacts {
            fresh_in_txn: true,
            ..sso()
        };
        assert_eq!(step("login", Some(fresh)), Step::Issue);
    }

    #[test]
    fn max_age_forces_reauthentication_after_selection() {
        let confirmed = SessionFacts {
            confirmed_in_txn: true,
            ..sso()
        };
        let p = prompt("select_account");
        assert_eq!(
            decide(&p, Some(60), true, Some(confirmed), false, NOW),
            Step::Login
        );
        assert_eq!(
            decide(&p, Some(7200), true, Some(confirmed), false, NOW),
            Step::Issue
        );
        assert_eq!(
            decide(&p, Some(0), true, Some(confirmed), false, NOW),
            Step::Login
        );
    }

    #[test]
    fn prompt_none_never_renders_ui() {
        assert_eq!(step("none", Some(sso())), Step::Issue);
        assert_eq!(step("none", None), Step::Required(Required::Login));
        let not_consented = SessionFacts {
            consented: false,
            ..sso()
        };
        assert_eq!(
            step("none", Some(not_consented)),
            Step::Required(Required::Consent)
        );
        let p = prompt("none");
        assert_eq!(
            decide(&p, Some(0), true, Some(sso()), false, NOW),
            Step::Required(Required::Login)
        );
        // 防御：即使 none 与 select_account 被构造在一起，也只返回错误而不渲染选择页。
        let forged = Prompt {
            none: true,
            select_account: true,
            ..Prompt::default()
        };
        assert_eq!(
            decide(&forged, None, true, Some(sso()), false, NOW),
            Step::Required(Required::AccountSelection)
        );
        assert_eq!(
            Required::AccountSelection.error().error,
            "account_selection_required"
        );
    }

    #[test]
    fn rollback_switch_restores_legacy_select_account_behaviour() {
        let p = prompt("select_account");
        assert_eq!(
            decide(&p, None, false, Some(sso()), false, NOW),
            Step::Consent
        );
        assert_eq!(decide(&p, None, false, None, false, NOW), Step::Login);
    }

    fn txn_row() -> oidc_auth_transactions::Model {
        let now = Utc::now().fixed_offset();
        oidc_auth_transactions::Model {
            id: Uuid::new_v4(),
            oidc_client_id: Uuid::new_v4(),
            browser_binding_hash: String::new(),
            csrf_hash: String::new(),
            redirect_uri: "https://rp.example.com/cb".into(),
            scopes: serde_json::json!(["openid"]),
            state: None,
            nonce: None,
            code_challenge: None,
            code_challenge_method: None,
            prompt: serde_json::json!(["select_account"]),
            max_age: None,
            login_hint: None,
            id_token_hint_sub: None,
            user_id: None,
            session_id: None,
            authenticated_at: None,
            created_at: now,
            expires_at: now,
            completed_at: None,
            account_choice_session_id: None,
            account_selected_at: None,
            account_selected_session_id: None,
            account_switch_requested_at: None,
        }
    }

    fn session_user(user_id: Uuid) -> SessionUser {
        let now = Utc::now().fixed_offset();
        (
            oidc_browser_sessions::Model {
                id: Uuid::new_v4(),
                session_hash: String::new(),
                user_id,
                auth_time: now,
                established_via: "oidc_login".into(),
                created_at: now,
                last_seen_at: now,
                expires_at: now,
                revoked_at: None,
                container_id: None,
                sid: String::new(),
                revoked_reason: None,
            },
            users::Model {
                id: user_id,
                username: "alice".into(),
                nickname: "Alice".into(),
                password: String::new(),
                email: String::new(),
                created_at: now,
                updated_at: now,
                avatar: None,
            },
        )
    }

    #[test]
    fn switch_request_hides_old_sso_until_password_login_in_txn() {
        let alice = session_user(Uuid::new_v4());
        let mut txn = txn_row();
        assert!(usable_session(&txn, Some(alice.clone())).is_some());

        txn.account_switch_requested_at = Some(Utc::now().fixed_offset());
        assert!(
            usable_session(&txn, Some(alice.clone())).is_none(),
            "选择「使用其他账号」后旧 Cookie 不能短路"
        );

        // 在本事务内输入密码得到的新会话可用
        let bob = session_user(Uuid::new_v4());
        txn.account_selected_at = Some(Utc::now().fixed_offset());
        txn.account_selected_session_id = Some(bob.0.id);
        assert!(usable_session(&txn, Some(bob.clone())).is_some());
        assert!(usable_session(&txn, Some(alice)).is_none());
        assert!(account_confirmed(&txn, bob.0.id));
    }

    #[test]
    fn several_accounts_require_explicit_selection_for_every_prompt() {
        // 多账号且未选定：任何交互 prompt 都先进选择页；prompt=none 只返回标准错误。
        for raw in ["", "consent", "login", "select_account"] {
            assert_eq!(
                decide(&prompt(raw), None, true, None, true, NOW),
                Step::SelectAccount,
                "prompt={raw}"
            );
        }
        assert_eq!(
            decide(&prompt("none"), None, true, None, true, NOW),
            Step::Required(Required::AccountSelection)
        );
        // 回滚开关只影响 select_account 的语义，不能让多账号被静默挑选。
        assert_eq!(
            decide(&prompt(""), None, false, None, true, NOW),
            Step::SelectAccount
        );
    }

    #[test]
    fn resolve_session_never_guesses_between_accounts() {
        let alice = session_user(Uuid::new_v4());
        let mut bob = session_user(Uuid::new_v4());
        bob.1.username = "bob".into();
        let mut txn = txn_row();
        txn.prompt = serde_json::json!([]);

        assert_eq!(resolve_session(&txn, &[]).0, None);
        let (one, ambiguous) = resolve_session(&txn, std::slice::from_ref(&alice));
        assert_eq!(one.map(|s| s.0.id), Some(alice.0.id));
        assert!(!ambiguous);

        let both = vec![alice.clone(), bob.clone()];
        let (none, ambiguous) = resolve_session(&txn, &both);
        assert!(
            none.is_none() && ambiguous,
            "两个账号且无提示：必须让用户选"
        );

        // login_hint 恰好匹配一个已认证账号 → 采用（仍不跳过 select_account 的显式确认）。
        txn.login_hint = Some("bob".into());
        let (hinted, ambiguous) = resolve_session(&txn, &both);
        assert_eq!(hinted.map(|s| s.0.id), Some(bob.0.id));
        assert!(!ambiguous);

        // 本事务内已确认的会话优先于任何提示。
        txn.account_selected_at = Some(Utc::now().fixed_offset());
        txn.account_selected_session_id = Some(alice.0.id);
        assert_eq!(
            resolve_session(&txn, &both).0.map(|s| s.0.id),
            Some(alice.0.id)
        );
    }

    #[test]
    fn id_token_hint_filters_candidates_to_the_hinted_user() {
        let alice = session_user(Uuid::new_v4());
        let bob = session_user(Uuid::new_v4());
        let mut txn = txn_row();
        txn.id_token_hint_sub = Some(bob.1.id.to_string());
        let found = candidates(&txn, &[alice, bob.clone()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.id, bob.0.id);
    }

    #[test]
    fn id_token_hint_for_another_user_hides_session() {
        let alice = session_user(Uuid::new_v4());
        let mut txn = txn_row();
        txn.id_token_hint_sub = Some(Uuid::new_v4().to_string());
        assert!(usable_session(&txn, Some(alice.clone())).is_none());
        txn.id_token_hint_sub = Some(alice.1.id.to_string());
        assert!(usable_session(&txn, Some(alice)).is_some());
    }

    #[test]
    fn confirmation_is_bound_to_the_exact_session() {
        let alice = session_user(Uuid::new_v4());
        let mut txn = txn_row();
        txn.account_selected_session_id = Some(alice.0.id);
        assert!(!account_confirmed(&txn, alice.0.id), "缺少确认时间不算确认");
        txn.account_selected_at = Some(Utc::now().fixed_offset());
        assert!(account_confirmed(&txn, alice.0.id));
        assert!(!account_confirmed(&txn, Uuid::new_v4()));
    }
}
