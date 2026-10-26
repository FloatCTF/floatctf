//! 统一身份会话（主站 Cookie 会话 + 浏览器账号容器 + 多账号）进程内 HTTP 集成测试，真实 PostgreSQL。
//!
//! 覆盖 docs/identity/unified-identity-audit.md 的 A01/A02/A04/A05/A07/A08/A09/A10/A11/A19/A20：
//! 主站登录 → RP 选择页免密；OIDC 登录页登录 → 主站自动恢复；主站退出撤销 OP 会话且不静默恢复；
//! RP 选择其他账号不改动主站活跃账号；多账号必选、`prompt=none` 报错；选择页退出单个/全部账号；
//! 旧版 `__Host-fctf_sso` 与旧版 localStorage JWT 的无损迁移；并发继续只发一次码；口令认证轮换容器秘密；
//! 账号上限淘汰；`id_token_hint` / `max_age=0`；Cookie 请求的 CSRF / 跨站 / 期望账号校验；
//! Cookie 不能访问管理端、Bearer 兼容不变。
//!
//! 需要 `DATABASE_URL` 指向已应用迁移的测试库；不可达时 soft-skip（与其他 DB-gated 测试一致）。

mod oidc_support;

use actix_web::{App, test, web};
use oidc_support::*;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use uuid::Uuid;

use floatctf::core::config::OidcConfig;
use floatctf::core::secret::Secret;
use floatctf::core::security::jwt::{self, Role};
use floatctf::entity::sea_orm_active_enums::{OidcClientStatus, OidcClientType};
use floatctf::entity::{
    oidc_authorization_codes, oidc_browser_containers, oidc_browser_sessions, oidc_clients,
    super_admin, users,
};
use floatctf::infrastructure::LogService;
use floatctf::modules::identity::oidc::clients::{self, ClientInput};

const PASSWORD: &str = "Unified@2026";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const ACCOUNTS: &str = "__Host-fctf_accounts";
const SSO: &str = "__Host-fctf_sso";
const JWT_SECRET: &str = "oidc-management-test-jwt-secret";

// ── 夹具 ────────────────────────────────────────────────────────────────────

async fn seed_user(db: &DatabaseConnection, tag: &str) -> users::Model {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let hash = Argon2::default()
        .hash_password(PASSWORD.as_bytes(), &salt)
        .unwrap()
        .to_string();
    let id = Uuid::new_v4();
    let short = &id.simple().to_string()[..10];
    let now = chrono::Utc::now().fixed_offset();
    users::ActiveModel {
        id: Set(id),
        username: Set(format!("uni-{tag}-{short}")),
        nickname: Set(format!("Uni {tag} {short}")),
        password: Set(hash),
        email: Set(format!("uni-{tag}-{short}@example.com")),
        created_at: Set(now),
        updated_at: Set(now),
        avatar: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn seed_admin(db: &DatabaseConnection) -> super_admin::Model {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().fixed_offset();
    super_admin::ActiveModel {
        id: Set(id),
        username: Set(format!("uni-admin-{}", &id.simple().to_string()[..8])),
        password: Set("unused".into()),
        email: Set(format!(
            "uni-admin-{}@example.com",
            &id.simple().to_string()[..8]
        )),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .unwrap()
}

struct TestClient {
    model: oidc_clients::Model,
    secret: String,
    redirect: String,
}

async fn seed_client(db: &DatabaseConnection, config: &OidcConfig, owner: Uuid) -> TestClient {
    let redirect = format!(
        "https://rp-{}.example.com/callback",
        &Uuid::new_v4().simple().to_string()[..8]
    );
    let (model, secret) = clients::create(
        db,
        config,
        owner,
        OidcClientType::Confidential,
        None,
        &ClientInput {
            name: "Unified RP".into(),
            description: None,
            homepage_url: Some("https://learn.example.com".into()),
            logo_url: None,
            redirect_uris: vec![redirect.clone()],
            scopes: vec!["openid".into(), "profile".into()],
            ..ClientInput::default()
        },
    )
    .await
    .unwrap();
    let model = clients::set_status(db, model, OidcClientStatus::Active, None, None)
        .await
        .unwrap();
    TestClient {
        model,
        secret: secret.unwrap(),
        redirect,
    }
}

fn auth_url(client: &TestClient, extra: &[(&str, &str)]) -> String {
    let mut pairs = vec![
        ("response_type", "code"),
        ("client_id", client.model.client_id.as_str()),
        ("redirect_uri", client.redirect.as_str()),
        ("scope", "openid profile"),
        ("state", "st-uni"),
        ("nonce", "n-uni"),
        ("code_challenge", CHALLENGE),
        ("code_challenge_method", "S256"),
    ];
    for (k, v) in extra {
        pairs.retain(|(pk, _)| pk != k);
        if !v.is_empty() {
            pairs.push((k, v));
        }
    }
    format!("/oauth/authorize?{}", form(&pairs))
}

async fn codes_for(db: &DatabaseConnection, client: &TestClient) -> u64 {
    oidc_authorization_codes::Entity::find()
        .filter(oidc_authorization_codes::Column::OidcClientId.eq(client.model.id))
        .count(db)
        .await
        .unwrap()
}

async fn live_sessions(
    db: &DatabaseConnection,
    user: &users::Model,
) -> Vec<oidc_browser_sessions::Model> {
    oidc_browser_sessions::Entity::find()
        .filter(oidc_browser_sessions::Column::UserId.eq(user.id))
        .filter(oidc_browser_sessions::Column::RevokedAt.is_null())
        .all(db)
        .await
        .unwrap()
}

fn callback_code(client: &TestClient, loc: &str) -> String {
    assert!(
        loc.starts_with(&client.redirect),
        "expected RP callback, got {loc}"
    );
    let q = query_of(loc);
    assert!(!q.contains_key("error"), "unexpected error callback {loc}");
    q["code"].clone()
}

macro_rules! app {
    ($db:expr, $config:expr) => {{
        jwt::configure_jwt_secret(Secret::new(JWT_SECRET));
        let provider = web::Data::new(provider($config, test_keys(false)));
        let db = web::Data::new($db.clone());
        test::init_service(
            App::new()
                .app_data(app_state($db.clone()))
                .app_data(web::Data::new(lazy_docker()))
                .app_data(web::Data::new(aws_sdk_s3::Client::from_conf(
                    aws_sdk_s3::Config::builder()
                        .behavior_version(aws_config::BehaviorVersion::latest())
                        .region(aws_sdk_s3::config::Region::new("local"))
                        .endpoint_url("http://127.0.0.1:1")
                        .build(),
                )))
                .app_data(provider)
                .app_data(db.clone())
                .app_data(web::Data::new(LogService::new(db)))
                .configure(floatctf::bootstrap::configure_all_routes),
        )
        .await
    }};
}

macro_rules! get {
    ($app:expr, $browser:expr, $uri:expr) => {{
        let mut req = test::TestRequest::get().uri(&$uri);
        if !$browser.cookies.is_empty() {
            req = req.insert_header(("Cookie", $browser.header()));
        }
        let resp = test::call_service(&$app, req.to_request()).await;
        $browser.absorb(&resp);
        resp
    }};
}

macro_rules! post_form {
    ($app:expr, $browser:expr, $uri:expr, $pairs:expr) => {{
        let mut req = test::TestRequest::post()
            .uri($uri)
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header(("Origin", ISSUER))
            .set_payload(form($pairs));
        if !$browser.cookies.is_empty() {
            req = req.insert_header(("Cookie", $browser.header()));
        }
        let resp = test::call_service(&$app, req.to_request()).await;
        $browser.absorb(&resp);
        resp
    }};
}

macro_rules! body_string {
    ($resp:expr) => {{ String::from_utf8(test::read_body($resp).await.to_vec()).unwrap() }};
}

/// 主站 JSON 请求（同源浏览器形态：`Origin` + `Sec-Fetch-Site` + CSRF 头）。
macro_rules! web {
    ($app:expr, $browser:expr, $method:ident, $uri:expr, $csrf:expr, $body:expr) => {{
        let mut req = test::TestRequest::$method()
            .uri($uri)
            .insert_header(("Origin", ISSUER))
            .insert_header(("Sec-Fetch-Site", "same-origin"));
        let csrf: Option<String> = $csrf;
        if let Some(csrf) = csrf {
            req = req.insert_header(("X-CSRF-Token", csrf));
        }
        let body: Option<Value> = $body;
        if let Some(body) = body {
            req = req.set_json(body);
        }
        if !$browser.cookies.is_empty() {
            req = req.insert_header(("Cookie", $browser.header()));
        }
        let resp = test::call_service(&$app, req.to_request()).await;
        $browser.absorb(&resp);
        let status = resp.status().as_u16();
        let body: Value =
            serde_json::from_slice(&test::read_body(resp).await).unwrap_or(Value::Null);
        (status, body)
    }};
}

macro_rules! status_of {
    ($app:expr, $browser:expr) => {{
        let (code, body) = web!($app, $browser, get, "/api/users/web-session", None, None);
        assert_eq!(code, 200);
        body["data"].clone()
    }};
}

fn csrf(status: &Value) -> Option<String> {
    status["csrf_token"].as_str().map(str::to_string)
}

/// 主站口令登录，返回登录后的状态。
macro_rules! web_login {
    ($app:expr, $browser:expr, $user:expr) => {{
        let st = status_of!($app, $browser);
        let token = csrf(&st).unwrap_or_else(|| "anonymous".into());
        let (code, body) = web!(
            $app,
            $browser,
            post,
            "/api/users/web-session/login",
            Some(token),
            Some(json!({"username": $user.username, "password": PASSWORD}))
        );
        assert_eq!(code, 200, "web login: {body}");
        let data: Value = body["data"].clone();
        data
    }};
}

/// 打开登录页并输入某账号密码，返回下一跳。
macro_rules! do_login {
    ($app:expr, $browser:expr, $login_location:expr, $username:expr) => {{
        let resp = get!($app, $browser, $login_location);
        assert_eq!(resp.status(), 200, "login page");
        let html = body_string!(resp);
        let txn = hidden(&html, "txn");
        let csrf = hidden(&html, "csrf");
        let resp = post_form!(
            $app,
            $browser,
            "/oauth/login",
            &[
                ("txn", txn.as_str()),
                ("csrf", csrf.as_str()),
                ("action", "login"),
                ("username", $username),
                ("password", PASSWORD)
            ]
        );
        assert_eq!(resp.status(), 302, "login submit");
        location(&resp)
    }};
}

macro_rules! allow_consent {
    ($app:expr, $browser:expr, $consent_location:expr) => {{
        let resp = get!($app, $browser, $consent_location);
        assert_eq!(resp.status(), 200, "consent page");
        let html = body_string!(resp);
        let txn = hidden(&html, "txn");
        let csrf = hidden(&html, "csrf");
        let resp = post_form!(
            $app,
            $browser,
            "/oauth/consent",
            &[
                ("txn", txn.as_str()),
                ("csrf", csrf.as_str()),
                ("action", "allow"),
                ("scope", "profile")
            ]
        );
        assert_eq!(resp.status(), 302, "consent submit");
        location(&resp)
    }};
}

/// 打开选择页，返回 (html, txn, csrf)。
macro_rules! open_chooser {
    ($app:expr, $browser:expr, $loc:expr) => {{
        let resp = get!($app, $browser, $loc);
        assert_eq!(resp.status(), 200, "chooser page");
        let html = body_string!(resp);
        let txn = hidden(&html, "txn");
        let csrf = hidden(&html, "csrf");
        (html, txn, csrf)
    }};
}

macro_rules! choose_account {
    ($app:expr, $browser:expr, $txn:expr, $csrf:expr, $action:expr, $account:expr) => {{
        let account: String = $account;
        post_form!(
            $app,
            $browser,
            "/oauth/select-account",
            &[
                ("txn", $txn.as_str()),
                ("csrf", $csrf.as_str()),
                ("action", $action),
                ("account", account.as_str())
            ]
        )
    }};
}

macro_rules! token_sub {
    ($app:expr, $client:expr, $code:expr) => {{
        use base64::Engine;
        let resp = test::call_service(
            &$app,
            test::TestRequest::post()
                .uri("/oauth/token")
                .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
                .insert_header((
                    "Authorization",
                    format!(
                        "Basic {}",
                        base64::engine::general_purpose::STANDARD
                            .encode(format!("{}:{}", $client.model.client_id, $client.secret))
                    ),
                ))
                .set_payload(form(&[
                    ("grant_type", "authorization_code"),
                    ("code", $code),
                    ("redirect_uri", $client.redirect.as_str()),
                    ("code_verifier", VERIFIER),
                ]))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200, "token exchange");
        let body: Value = test::read_body_json(resp).await;
        let token = body["id_token"].as_str().unwrap().to_string();
        let payload = token.split('.').nth(1).unwrap();
        let claims: Value = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(payload)
                .unwrap(),
        )
        .unwrap();
        (claims, token)
    }};
}

/// RP 首次授权：登录页输入密码 + 同意，返回 sub。
macro_rules! rp_first_login {
    ($app:expr, $browser:expr, $rp:expr, $user:expr) => {{
        let resp = get!($app, $browser, auth_url(&$rp, &[]));
        let mut loc = location(&resp);
        if loc.starts_with("/oauth/login") {
            loc = do_login!($app, $browser, loc, &$user.username);
        }
        if loc.starts_with("/oauth/consent") {
            loc = allow_consent!($app, $browser, loc);
        }
        let code = callback_code(&$rp, &loc);
        let sub: Value = token_sub!($app, $rp, code.as_str()).0["sub"].clone();
        sub
    }};
}

fn account_id_of(status: &Value, user: &users::Model) -> String {
    status["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["user_id"] == user.id.to_string())
        .unwrap_or_else(|| panic!("{} not in accounts: {status}", user.username))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

// ── 主站登录：只有 HttpOnly 容器 Cookie，响应不含令牌 ─────────────────────

#[actix_web::test]
async fn web_login_sets_only_an_httponly_container_cookie_and_returns_no_token() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let app = app!(db, unified_config());
    let alice = seed_user(&db, "alice").await;
    let mut b = Browser::default();

    let st = status_of!(app, b);
    assert_eq!(st["mode"], "cookie");
    assert_eq!(st["authenticated"], false);
    assert!(st["csrf_token"].is_null(), "匿名浏览器不创建容器");

    // 原始响应：检查 Set-Cookie 属性。
    let req = test::TestRequest::post()
        .uri("/api/users/web-session/login")
        .insert_header(("Origin", ISSUER))
        .insert_header(("X-CSRF-Token", "anonymous"))
        .set_json(json!({"username": alice.username, "password": PASSWORD}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let cookie = resp
        .response()
        .cookies()
        .find(|c| c.name() == ACCOUNTS)
        .expect("container cookie")
        .into_owned();
    assert!(cookie.http_only().unwrap_or(false));
    assert!(cookie.secure().unwrap_or(false));
    assert_eq!(cookie.path(), Some("/"));
    assert!(cookie.domain().is_none(), "__Host- Cookie 不得带 Domain");
    assert_eq!(cookie.same_site(), Some(actix_web::cookie::SameSite::Lax));
    assert!(
        resp.response().cookies().all(|c| c.name() != SSO),
        "不再下发旧版 SSO Cookie"
    );
    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];
    assert_eq!(data["authenticated"], true);
    assert_eq!(data["user"]["id"], alice.id.to_string());
    let text = body.to_string();
    assert!(!text.contains("eyJ"), "响应体不得含任何 JWT");
    assert!(!text.contains(cookie.value()), "响应体不得回显容器秘密");
    assert_ne!(data["csrf_token"].as_str().unwrap(), cookie.value());
}

// ── Cookie 请求的 CSRF / 跨站 / 期望账号；管理端隔离；Bearer 兼容（A19/A20） ──

#[actix_web::test]
async fn cookie_requests_need_csrf_first_party_origin_and_matching_account() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let app = app!(db, unified_config());
    let alice = seed_user(&db, "alice").await;
    let admin = seed_admin(&db).await;
    let mut b = Browser::default();
    let st = web_login!(app, b, alice);
    let token = csrf(&st).unwrap();

    let (code, me) = web!(app, b, get, "/api/users/me", Some(token.clone()), None);
    assert_eq!(code, 200, "{me}");
    assert_eq!(me["data"]["id"], alice.id.to_string());

    // 缺 CSRF / 伪造 CSRF → 401
    let (code, _) = web!(app, b, get, "/api/users/me", None, None);
    assert_eq!(code, 401);
    let (code, _) = web!(app, b, get, "/api/users/me", Some("forged".into()), None);
    assert_eq!(code, 401);

    // 跨站 Origin（即便带了正确 CSRF）→ 401
    let req = test::TestRequest::get()
        .uri("/api/users/me")
        .insert_header(("Origin", "https://evil.example.com"))
        .insert_header(("X-CSRF-Token", token.clone()))
        .insert_header(("Cookie", b.header()))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 401);
    // 同站不同源（例如同主机另一端口的应用）→ 401
    let req = test::TestRequest::get()
        .uri("/api/users/me")
        .insert_header(("Sec-Fetch-Site", "same-site"))
        .insert_header(("X-CSRF-Token", token.clone()))
        .insert_header(("Cookie", b.header()))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 401);

    // 期望账号与服务端活跃账号不一致（另一标签页已切换）→ 401
    let req = test::TestRequest::get()
        .uri("/api/users/me")
        .insert_header(("Origin", ISSUER))
        .insert_header(("X-CSRF-Token", token.clone()))
        .insert_header(("X-FloatCTF-Account", Uuid::new_v4().to_string()))
        .insert_header(("Cookie", b.header()))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 401);

    // Cookie 永远得不到管理员身份
    let (code, _) = web!(app, b, get, "/api/admin/users", Some(token.clone()), None);
    assert_eq!(code, 401, "用户 Cookie 不能访问管理端");

    // Bearer 兼容：CLI / 第三方前端照旧
    let bob = seed_user(&db, "bob").await;
    let bearer = jwt::gen_jwt_token(bob.id, Role::User, None).unwrap();
    let req = test::TestRequest::get()
        .uri("/api/users/me")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .insert_header(("Cookie", b.header()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["id"],
        bob.id.to_string(),
        "Bearer 优先，不与 Cookie 混用"
    );
    // 无效 Bearer 不会回落到 Cookie
    let req = test::TestRequest::get()
        .uri("/api/users/me")
        .insert_header(("Authorization", "Bearer broken"))
        .insert_header(("X-CSRF-Token", token.clone()))
        .insert_header(("Origin", ISSUER))
        .insert_header(("Cookie", b.header()))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 401);
    let admin_token = jwt::gen_jwt_token(admin.id, Role::SuperAdmin, None).unwrap();
    let req = test::TestRequest::get()
        .uri("/api/admin/users")
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        200,
        "管理员 Bearer 不变"
    );

    // 状态变更：跨站与缺 CSRF 被拒（403），不改变任何状态
    let req = test::TestRequest::post()
        .uri("/api/users/web-session/logout")
        .insert_header(("Origin", "https://evil.example.com"))
        .insert_header(("X-CSRF-Token", token.clone()))
        .insert_header(("Cookie", b.header()))
        .set_json(json!({"scope": "browser"}))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 403);
    let (code, _) = web!(
        app,
        b,
        post,
        "/api/users/web-session/logout",
        None,
        Some(json!({"scope": "browser"}))
    );
    assert_eq!(code, 403);
    assert_eq!(
        live_sessions(&db, &alice).await.len(),
        1,
        "被拒请求不得撤销会话"
    );
}

// ── Login CSRF：无容器时登录也必须是同源 + 自定义头 ─────────────────────────

#[actix_web::test]
async fn web_login_rejects_cross_site_and_missing_custom_header() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let app = app!(db, unified_config());
    let alice = seed_user(&db, "alice").await;
    let body = json!({"username": alice.username, "password": PASSWORD});
    let req = test::TestRequest::post()
        .uri("/api/users/web-session/login")
        .insert_header(("Origin", "https://evil.example.com"))
        .insert_header(("X-CSRF-Token", "anonymous"))
        .set_json(body.clone())
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 403);
    let req = test::TestRequest::post()
        .uri("/api/users/web-session/login")
        .insert_header(("Origin", ISSUER))
        .set_json(body.clone())
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 403);
    assert!(live_sessions(&db, &alice).await.is_empty());

    // 错误密码：401，不建立会话
    let mut b = Browser::default();
    let (code, _) = web!(
        app,
        b,
        post,
        "/api/users/web-session/login",
        Some("anonymous".into()),
        Some(json!({"username": alice.username, "password": "wrong-password"}))
    );
    assert_eq!(code, 401);
    assert!(live_sessions(&db, &alice).await.is_empty());
}

// ── A01：主站密码登录 → RP 选择页 → 点账号即继续，无额外密码 ─────────────────

#[actix_web::test]
async fn main_site_login_then_rp_chooser_continues_without_password() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = unified_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    let st = web_login!(app, b, alice);
    let account = account_id_of(&st, &alice);

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let loc = location(&resp);
    assert!(loc.starts_with("/oauth/select-account"), "{loc}");
    let (html, txn, csrf) = open_chooser!(app, b, loc);
    assert!(html.contains(&format!("@{}", alice.username)));
    assert!(!html.contains("name=\"password\""), "选择页不收密码");
    let resp = choose_account!(app, b, txn, csrf, "continue", account.clone());
    let loc = location(&resp);
    // 首次授权该应用：同意页（独立页面），然后回调；全程未输入密码。
    assert!(loc.starts_with("/oauth/consent"), "{loc}");
    let loc = allow_consent!(app, b, loc);
    let code = callback_code(&rp, &loc);
    let (claims, _) = token_sub!(app, rp, code.as_str());
    assert_eq!(claims["sub"], alice.id.to_string());

    // 主站会话不受 RP 授权影响
    let st = status_of!(app, b);
    assert_eq!(st["user"]["id"], alice.id.to_string());
}

// ── A02 / U03：只在 OIDC 登录页输入过密码 → 主站自动恢复同一用户 ────────────

#[actix_web::test]
async fn oidc_login_page_restores_main_site_without_password() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = unified_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    let sub = rp_first_login!(app, b, rp, alice);
    assert_eq!(sub, alice.id.to_string());
    assert!(b.cookies.contains_key(ACCOUNTS));

    let st = status_of!(app, b);
    assert_eq!(st["authenticated"], false, "RP 登录不直接改动主站");
    assert_eq!(st["auto_restore"], true);
    let (code, body) = web!(
        app,
        b,
        post,
        "/api/users/web-session/restore",
        csrf(&st),
        None
    );
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["data"]["user"]["id"], alice.id.to_string());
    let (code, me) = web!(app, b, get, "/api/users/me", csrf(&body["data"]), None);
    assert_eq!(code, 200);
    assert_eq!(me["data"]["username"], alice.username);
}

// ── A04 / A05 / U12：主站普通退出撤销 OP 会话，不静默恢复，RP 再登录需密码 ──

#[actix_web::test]
async fn main_site_logout_revokes_identity_session_and_never_silently_restores() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = unified_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    let st = web_login!(app, b, alice);
    rp_first_login!(app, b, rp, alice);
    assert_eq!(
        live_sessions(&db, &alice).await.len(),
        1,
        "主站与 OP 共用同一身份会话"
    );

    let (code, body) = web!(
        app,
        b,
        post,
        "/api/users/web-session/logout",
        csrf(&st),
        Some(json!({"scope": "account"}))
    );
    assert_eq!(code, 200, "{body}");
    let data = &body["data"];
    assert_eq!(data["authenticated"], false);
    assert!(data["accounts"].as_array().unwrap().is_empty());
    assert!(live_sessions(&db, &alice).await.is_empty(), "OP 会话已撤销");
    let revoked = oidc_browser_sessions::Entity::find()
        .filter(oidc_browser_sessions::Column::UserId.eq(alice.id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revoked.revoked_reason.as_deref(), Some("logout"));

    // 不会自动恢复
    let (code, _) = web!(
        app,
        b,
        post,
        "/api/users/web-session/restore",
        csrf(data),
        None
    );
    assert_eq!(code, 409);
    let (code, _) = web!(app, b, get, "/api/users/me", csrf(data), None);
    assert_eq!(code, 401, "旧 Cookie 不能暗中复活");

    // RP 再次登录：必须输入密码（选择页不出现）
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    assert!(
        location(&resp).starts_with("/oauth/login"),
        "{}",
        location(&resp)
    );
}

// ── U06 / P3：RP 选择其他账号不改动主站活跃账号；多账号必选；授权独立 ──────

#[actix_web::test]
async fn rp_choosing_another_account_never_switches_the_main_site() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = unified_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let bob = seed_user(&db, "bob").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    web_login!(app, b, alice);
    rp_first_login!(app, b, rp, alice);

    // RP 中「使用其他账号」→ 输入 bob 的密码
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let resp = post_form!(
        app,
        b,
        "/oauth/select-account",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf.as_str()),
            ("action", "other")
        ]
    );
    let loc = do_login!(app, b, location(&resp), &bob.username);
    assert!(
        loc.starts_with("/oauth/consent"),
        "bob 对该应用没有授权，必须独立同意: {loc}"
    );
    let loc = allow_consent!(app, b, loc);
    let code = callback_code(&rp, &loc);
    assert_eq!(
        token_sub!(app, rp, code.as_str()).0["sub"],
        bob.id.to_string()
    );

    // 两个账号都在本浏览器，主站仍是 alice
    let st = status_of!(app, b);
    assert_eq!(st["user"]["id"], alice.id.to_string());
    assert_eq!(st["accounts"].as_array().unwrap().len(), 2);
    assert_eq!(
        live_sessions(&db, &alice).await.len(),
        1,
        "登录 bob 不撤销 alice"
    );

    // 之后不带 prompt 的授权也必须先选（多账号绝不静默挑选）；prompt=none → 标准错误
    let resp = get!(app, b, auth_url(&rp, &[]));
    assert!(location(&resp).starts_with("/oauth/select-account"));
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "none")]));
    let q = query_of(&location(&resp));
    assert_eq!(q["error"], "account_selection_required");
    assert_eq!(q["state"], "st-uni");

    // 选择页列出两个账号；选 alice → 已授权，直接回调
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let (html, txn, csrf) = open_chooser!(app, b, location(&resp));
    assert!(html.contains(&alice.username) && html.contains(&bob.username));
    let resp = choose_account!(app, b, txn, csrf, "continue", account_id_of(&st, &alice));
    let code = callback_code(&rp, &location(&resp));
    assert_eq!(
        token_sub!(app, rp, code.as_str()).0["sub"],
        alice.id.to_string()
    );

    // 主站切换到 bob；旧标签页仍以 alice 身份发请求 → 401（前端据此重新同步）
    let alice_csrf = csrf_of(&st);
    let (code, body) = web!(
        app,
        b,
        post,
        "/api/users/web-session/select",
        Some(alice_csrf.clone()),
        Some(json!({"account_id": account_id_of(&st, &bob)}))
    );
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["data"]["user"]["id"], bob.id.to_string());
    let req = test::TestRequest::get()
        .uri("/api/users/me")
        .insert_header(("Origin", ISSUER))
        .insert_header(("X-CSRF-Token", alice_csrf))
        .insert_header(("X-FloatCTF-Account", alice.id.to_string()))
        .insert_header(("Cookie", b.header()))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 401);
}

fn csrf_of(status: &Value) -> String {
    csrf(status).expect("csrf token")
}

// ── A09 / U07：选择页退出单个账号、退出全部账号；不发码 ─────────────────────

#[actix_web::test]
async fn chooser_signs_out_one_account_then_all_accounts() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = unified_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let bob = seed_user(&db, "bob").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    web_login!(app, b, bob);
    let st = web_login!(app, b, alice);
    assert_eq!(st["user"]["id"], alice.id.to_string(), "登录即切到新账号");
    assert_eq!(st["accounts"].as_array().unwrap().len(), 2);
    // 两个账号：即使 RP 没传 prompt 也先选择；选 alice 并首次同意。
    let resp = get!(app, b, auth_url(&rp, &[]));
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let resp = choose_account!(app, b, txn, csrf, "continue", account_id_of(&st, &alice));
    let loc = allow_consent!(app, b, location(&resp));
    callback_code(&rp, &loc);
    let codes_before = codes_for(&db, &rp).await;

    // 退出 alice（主站当前账号）：只撤销 alice；主站随之退出；仍有 bob → 重新显示选择页，不发码
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let resp = choose_account!(app, b, txn, csrf, "logout", account_id_of(&st, &alice));
    assert_eq!(resp.status(), 200);
    let html = body_string!(resp);
    assert!(html.contains(&bob.username) && !html.contains(&alice.username));
    assert_eq!(codes_for(&db, &rp).await, codes_before, "退出账号不发码");
    assert!(live_sessions(&db, &alice).await.is_empty());
    assert_eq!(live_sessions(&db, &bob).await.len(), 1);
    let st2 = status_of!(app, b);
    assert_eq!(st2["authenticated"], false, "主站中的 alice 一并退出");
    assert_eq!(
        st2["auto_restore"], false,
        "退出前就存在的 bob 不会被自动恢复"
    );

    // 旧 CSRF 已作废：重放退出请求失败
    let resp = choose_account!(app, b, txn, csrf, "logout", account_id_of(&st, &bob));
    assert_eq!(resp.status(), 400);
    assert_eq!(live_sessions(&db, &bob).await.len(), 1);

    // 退出全部 → 登录页，容器撤销、Cookie 清除
    let txn2 = hidden(&html, "txn");
    let csrf2 = hidden(&html, "csrf");
    let resp = post_form!(
        app,
        b,
        "/oauth/select-account",
        &[
            ("txn", txn2.as_str()),
            ("csrf", csrf2.as_str()),
            ("action", "logout_all")
        ]
    );
    assert!(location(&resp).starts_with("/oauth/login"));
    assert!(live_sessions(&db, &bob).await.is_empty());
    assert!(!b.cookies.contains_key(ACCOUNTS));
    let revoked_containers = oidc_browser_containers::Entity::find()
        .filter(oidc_browser_containers::Column::RevokedAt.is_not_null())
        .count(&db)
        .await
        .unwrap();
    assert!(revoked_containers >= 1);
}

// ── 旧版单会话 Cookie 无损迁入容器 ──────────────────────────────────────────

#[actix_web::test]
async fn legacy_sso_cookie_is_migrated_into_the_container() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    // 旧版本二进制建立的会话
    let legacy_app = app!(db, test_config());
    let rp = seed_client(&db, &test_config(), owner.id).await;
    let mut b = Browser::default();
    rp_first_login!(legacy_app, b, rp, alice);
    assert!(b.cookies.contains_key(SSO));
    let before = live_sessions(&db, &alice).await;
    assert_eq!(before.len(), 1);

    // 升级后：同一浏览器访问即迁移，会话行、认证时间不变，之后免密继续
    let app = app!(db, unified_config());
    let st = status_of!(app, b);
    assert!(b.cookies.contains_key(ACCOUNTS));
    assert!(!b.cookies.contains_key(SSO), "旧 Cookie 被清除");
    assert_eq!(st["auto_restore"], true);
    let after = live_sessions(&db, &alice).await;
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, before[0].id);
    assert_eq!(after[0].auth_time, before[0].auth_time);
    assert!(after[0].container_id.is_some());
    let resp = get!(app, b, auth_url(&rp, &[]));
    let code = callback_code(&rp, &location(&resp));
    assert_eq!(
        token_sub!(app, rp, code.as_str()).0["sub"],
        alice.id.to_string()
    );
}

// ── 旧版 localStorage JWT 迁移为服务端会话 ──────────────────────────────────

#[actix_web::test]
async fn legacy_jwt_is_migrated_into_a_cookie_session() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let app = app!(db, unified_config());
    let alice = seed_user(&db, "alice").await;
    let bearer = jwt::gen_jwt_token(alice.id, Role::User, None).unwrap();
    let mut b = Browser::default();

    // Cookie 不能调用迁移接口；必须是 Bearer
    let (code, _) = web!(
        app,
        b,
        post,
        "/api/users/web-session/migrate",
        Some("anonymous".into()),
        None
    );
    assert_eq!(code, 401);
    let req = test::TestRequest::post()
        .uri("/api/users/web-session/migrate")
        .insert_header(("Origin", ISSUER))
        .insert_header(("X-CSRF-Token", "anonymous"))
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    b.absorb(&resp);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["user"]["id"], alice.id.to_string());
    let sessions = live_sessions(&db, &alice).await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].established_via, "jwt_bridge");
    assert!(
        sessions[0].auth_time < chrono::Utc::now().fixed_offset(),
        "桥接不是新认证：auth_time 取 JWT 签发时间"
    );
    let (code, _) = web!(app, b, get, "/api/users/me", csrf(&body["data"]), None);
    assert_eq!(code, 200);
}

// ── A08：并发「继续」只发一次码 ─────────────────────────────────────────────

#[actix_web::test]
async fn concurrent_continue_issues_exactly_one_code() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = unified_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    let st = web_login!(app, b, alice);
    rp_first_login!(app, b, rp, alice);
    let before = codes_for(&db, &rp).await;
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let account = account_id_of(&st, &alice);
    let make = || {
        test::TestRequest::post()
            .uri("/oauth/select-account")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header(("Origin", ISSUER))
            .insert_header(("Cookie", b.header()))
            .set_payload(form(&[
                ("txn", txn.as_str()),
                ("csrf", csrf.as_str()),
                ("action", "continue"),
                ("account", account.as_str()),
            ]))
            .to_request()
    };
    let (r1, r2) = futures_util::join!(
        test::call_service(&app, make()),
        test::call_service(&app, make())
    );
    let to_rp = [&r1, &r2]
        .iter()
        .filter(|r| location(r).starts_with(&rp.redirect))
        .count();
    assert_eq!(to_rp, 1, "只有一个请求回到 RP");
    assert_eq!(codes_for(&db, &rp).await, before + 1);
}

// ── 会话固定：口令认证轮换容器秘密；容量上限淘汰最久未用 ──────────────────

#[actix_web::test]
async fn password_login_rotates_container_secret_and_caps_accounts() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = OidcConfig {
        max_browser_accounts: 2,
        ..unified_config()
    };
    let app = app!(db, config);
    let alice = seed_user(&db, "alice").await;
    let bob = seed_user(&db, "bob").await;
    let carol = seed_user(&db, "carol").await;
    let mut b = Browser::default();
    web_login!(app, b, alice);
    let planted = b.clone(); // 攻击者预置/窃取的旧容器 Cookie
    let first = b.cookies[ACCOUNTS].clone();
    web_login!(app, b, bob);
    assert_ne!(b.cookies[ACCOUNTS], first, "每次口令认证轮换容器秘密");
    let mut stale = planted;
    let st = status_of!(app, stale);
    assert!(st["accounts"].as_array().unwrap().is_empty(), "旧秘密失效");

    web_login!(app, b, carol);
    let st = status_of!(app, b);
    let names: Vec<&str> = st["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["username"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 2);
    assert!(
        !names.contains(&alice.username.as_str()),
        "淘汰最久未用的 alice: {names:?}"
    );
    let evicted = oidc_browser_sessions::Entity::find()
        .filter(oidc_browser_sessions::Column::UserId.eq(alice.id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(evicted.revoked_reason.as_deref(), Some("evicted"));

    // 同一账号重复登录：替换而不是重复
    web_login!(app, b, carol);
    assert_eq!(live_sessions(&db, &carol).await.len(), 1);
}

// ── A11：id_token_hint 在多账号中选中对应账号；max_age=0 必须输密码 ──────────

#[actix_web::test]
async fn id_token_hint_and_max_age_zero_with_several_accounts() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = unified_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let bob = seed_user(&db, "bob").await;
    let carol = seed_user(&db, "carol").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    web_login!(app, b, bob);
    web_login!(app, b, alice);
    // alice、bob 都对 RP 授权过
    let resp = get!(app, b, auth_url(&rp, &[]));
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let st = status_of!(app, b);
    let resp = choose_account!(app, b, txn, csrf, "continue", account_id_of(&st, &bob));
    let loc = allow_consent!(app, b, location(&resp));
    let (_, bob_hint) = token_sub!(app, rp, callback_code(&rp, &loc).as_str());

    // id_token_hint = bob → 候选只剩 bob，prompt=none 静默成功
    let resp = get!(
        app,
        b,
        auth_url(
            &rp,
            &[("prompt", "none"), ("id_token_hint", bob_hint.as_str())]
        )
    );
    let code = callback_code(&rp, &location(&resp));
    assert_eq!(
        token_sub!(app, rp, code.as_str()).0["sub"],
        bob.id.to_string()
    );

    // 登录页用 carol 的正确密码也不能满足 bob 的 hint
    let resp = get!(
        app,
        b,
        auth_url(
            &rp,
            &[("prompt", "login"), ("id_token_hint", bob_hint.as_str())]
        )
    );
    let login = location(&resp);
    assert!(login.starts_with("/oauth/login"));
    let resp = get!(app, b, login.clone());
    let html = body_string!(resp);
    let resp = post_form!(
        app,
        b,
        "/oauth/login",
        &[
            ("txn", hidden(&html, "txn").as_str()),
            ("csrf", hidden(&html, "csrf").as_str()),
            ("action", "login"),
            ("username", carol.username.as_str()),
            ("password", PASSWORD)
        ]
    );
    assert_eq!(resp.status(), 200);
    assert!(body_string!(resp).contains("指定的 FloatCTF 账号"));
    assert!(live_sessions(&db, &carol).await.is_empty());

    // max_age=0：选中账号后仍必须输入密码
    let resp = get!(app, b, auth_url(&rp, &[("max_age", "0")]));
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let resp = choose_account!(app, b, txn, csrf, "continue", account_id_of(&st, &alice));
    assert!(
        location(&resp).starts_with("/oauth/login"),
        "{}",
        location(&resp)
    );
}

// ── 两个浏览器（设备）互相隔离 ──────────────────────────────────────────────

#[actix_web::test]
async fn two_browsers_are_isolated() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let app = app!(db, unified_config());
    let alice = seed_user(&db, "alice").await;
    let mut laptop = Browser::default();
    let mut phone = Browser::default();
    let st_laptop = web_login!(app, laptop, alice);
    let st_phone = web_login!(app, phone, alice);
    assert_eq!(live_sessions(&db, &alice).await.len(), 2);
    assert_ne!(csrf(&st_laptop), csrf(&st_phone), "CSRF 绑定各自容器");

    // 跨设备 CSRF 无效
    let (code, _) = web!(app, laptop, get, "/api/users/me", csrf(&st_phone), None);
    assert_eq!(code, 401);

    let (code, _) = web!(
        app,
        laptop,
        post,
        "/api/users/web-session/logout",
        csrf(&st_laptop),
        Some(json!({"scope": "browser"}))
    );
    assert_eq!(code, 200);
    assert_eq!(
        live_sessions(&db, &alice).await.len(),
        1,
        "另一设备不受影响"
    );
    let (code, _) = web!(app, phone, get, "/api/users/me", csrf(&st_phone), None);
    assert_eq!(code, 200);
}

// ── 未开启统一会话：状态接口返回 legacy，写接口 404 ──────────────────────────

#[actix_web::test]
async fn unified_session_endpoints_are_inert_when_switched_off() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let app = app!(db, test_config());
    let mut b = Browser::default();
    let st = status_of!(app, b);
    assert_eq!(st["mode"], "legacy");
    let (code, _) = web!(
        app,
        b,
        post,
        "/api/users/web-session/login",
        Some("anonymous".into()),
        Some(json!({"username": "x", "password": "y"}))
    );
    assert_eq!(code, 404);
}
