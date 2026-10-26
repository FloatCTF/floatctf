//! 账号选择（`prompt=select_account`）与安全切换账号（进程内 HTTP + 真实 PostgreSQL）。
//!
//! 覆盖 ADR-oidc-account-chooser 的验收矩阵：无 SSO 直达登录、有 SSO 必现选择页且未点击不发码、
//! 继续当前账号免密码免重复同意、使用其他账号强制输入密码并独立同意、取消回传 access_denied、
//! select_account 与 consent/login/max_age/none 的组合、id_token_hint、会话失效、CSRF / 换浏览器 /
//! 跨站 / 过期 / 已完成事务、另一标签页切换账号、双击/并发继续只发一次码、应用中途被暂停、
//! 未传 prompt 的旧 RP 兼容，以及 `account_chooser = false` 回滚开关。
//!
//! 需要 `DATABASE_URL` 指向已应用迁移的测试库；不可达时 soft-skip（与其他 DB-gated 测试一致）。

mod oidc_support;

use actix_web::{App, test, web};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use oidc_support::*;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter,
};
use serde_json::Value;
use uuid::Uuid;

use floatctf::core::config::OidcConfig;
use floatctf::entity::sea_orm_active_enums::{OidcClientStatus, OidcClientType};
use floatctf::entity::{
    oidc_auth_transactions, oidc_authorization_codes, oidc_browser_sessions, oidc_clients,
    oidc_consents, users,
};
use floatctf::infrastructure::LogService;
use floatctf::modules::identity::oidc::clients::{self, ClientInput};

const PASSWORD: &str = "OidcChooser@2026";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const SSO: &str = "__Host-fctf_sso";
const BID: &str = "__Host-fctf_bid";

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
        username: Set(format!("chooser-{tag}-{short}")),
        nickname: Set(format!("Chooser {tag} {short}")),
        password: Set(hash),
        email: Set(format!("chooser-{tag}-{short}@example.com")),
        created_at: Set(now),
        updated_at: Set(now),
        avatar: Set(Some("/public/avatars/chooser.png".into())),
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
            name: "Chooser RP".into(),
            description: None,
            homepage_url: Some("https://learn.example.com".into()),
            logo_url: None,
            redirect_uris: vec![redirect.clone()],
            scopes: vec!["openid".into(), "profile".into(), "email".into()],
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
        ("state", "st-chooser"),
        ("nonce", "n-chooser"),
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

fn txn_of(location: &str) -> Uuid {
    Uuid::parse_str(&query_of(location)["txn"]).unwrap()
}

async fn txn_row(db: &DatabaseConnection, id: Uuid) -> oidc_auth_transactions::Model {
    oidc_auth_transactions::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
}

async fn codes_for(db: &DatabaseConnection, client: &TestClient) -> u64 {
    oidc_authorization_codes::Entity::find()
        .filter(oidc_authorization_codes::Column::OidcClientId.eq(client.model.id))
        .count(db)
        .await
        .unwrap()
}

async fn session_of(db: &DatabaseConnection, user: &users::Model) -> oidc_browser_sessions::Model {
    oidc_browser_sessions::Entity::find()
        .filter(oidc_browser_sessions::Column::UserId.eq(user.id))
        .filter(oidc_browser_sessions::Column::RevokedAt.is_null())
        .one(db)
        .await
        .unwrap()
        .expect("active session")
}

macro_rules! app {
    ($db:expr, $config:expr) => {{
        let provider = web::Data::new(provider($config, test_keys(false)));
        let db = web::Data::new($db.clone());
        test::init_service(
            App::new()
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

/// 打开同意页并允许（勾选 profile），返回回调地址。
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

/// 打开账号选择页，返回 (html, txn, csrf)。
macro_rules! open_chooser {
    ($app:expr, $browser:expr, $chooser_location:expr) => {{
        let resp = get!($app, $browser, $chooser_location);
        assert_eq!(resp.status(), 200, "chooser page");
        let html = body_string!(resp);
        let txn = hidden(&html, "txn");
        let csrf = hidden(&html, "csrf");
        (html, txn, csrf)
    }};
}

macro_rules! choose {
    ($app:expr, $browser:expr, $txn:expr, $csrf:expr, $action:expr) => {{
        post_form!(
            $app,
            $browser,
            "/oauth/select-account",
            &[
                ("txn", $txn.as_str()),
                ("csrf", $csrf.as_str()),
                ("action", $action)
            ]
        )
    }};
}

/// 兑换授权码并只凭 JWKS 验签 ID Token，返回 claims。
macro_rules! id_claims {
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
        let jwks: JwkSet = test::read_body_json(
            test::call_service(
                &$app,
                test::TestRequest::get()
                    .uri("/.well-known/jwks.json")
                    .to_request(),
            )
            .await,
        )
        .await;
        let token = body["id_token"].as_str().unwrap();
        let header = jsonwebtoken::decode_header(token).unwrap();
        let key = DecodingKey::from_jwk(jwks.find(&header.kid.unwrap()).unwrap()).unwrap();
        let mut v = Validation::new(Algorithm::RS256);
        v.set_audience(&[$client.model.client_id.as_str()]);
        v.set_issuer(&[ISSUER]);
        let claims = jsonwebtoken::decode::<Value>(token, &key, &v)
            .unwrap()
            .claims;
        (claims, token.to_string())
    }};
}

/// 回调断言：只带 code + state + iss，返回 code。
fn callback_code(client: &TestClient, loc: &str) -> String {
    assert!(
        loc.starts_with(&client.redirect),
        "expected RP callback: {loc}"
    );
    let q = query_of(loc);
    assert_eq!(q.get("state").map(String::as_str), Some("st-chooser"));
    assert_eq!(q.get("iss").map(String::as_str), Some(ISSUER));
    assert!(!q.contains_key("error"), "{loc}");
    let mut keys: Vec<_> = q.keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["code", "iss", "state"],
        "回调不得携带其他数据: {loc}"
    );
    q["code"].clone()
}

/// 在没有 SSO 的浏览器中通过 OIDC 登录页登录（必要时同意），返回 (auth_time, id_token)。
macro_rules! sign_in_first_time {
    ($app:expr, $browser:expr, $client:expr, $user:expr) => {{
        let resp = get!($app, $browser, auth_url($client, &[]));
        let loc = location(&resp);
        assert!(loc.starts_with("/oauth/login?txn="), "{loc}");
        let loc = do_login!($app, $browser, loc, $user.username.as_str());
        // 首次需要同意；该用户此前已授权过本应用时直接回调。
        let loc = if loc.starts_with("/oauth/consent?txn=") {
            allow_consent!($app, $browser, loc)
        } else {
            loc
        };
        let code = callback_code($client, &loc);
        let (claims, id_token) = id_claims!($app, $client, code.as_str());
        assert_eq!(claims["sub"], $user.id.to_string());
        (claims["auth_time"].as_i64().unwrap(), id_token)
    }};
}

// ── 01：无 SSO ───────────────────────────────────────────────────────────────

#[actix_web::test]
async fn select_account_without_sso_goes_straight_to_password_login() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/login?txn="),
        "无 SSO 直接登录页: {loc}"
    );
    // 输入密码即完成本次选择：之后不会再出现空的选择页，只按需同意。
    let loc = do_login!(app, b, loc, alice.username.as_str());
    assert!(loc.starts_with("/oauth/consent?txn="), "{loc}");
    let loc = allow_consent!(app, b, loc);
    let code = callback_code(&rp, &loc);
    let (claims, _) = id_claims!(app, rp, code.as_str());
    assert_eq!(claims["sub"], alice.id.to_string());
}

// ── 02 / 03：有 SSO → 选择页；继续当前账号免密码、免重复同意 ─────────────────

#[actix_web::test]
async fn chooser_is_shown_and_continue_needs_neither_password_nor_consent() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    let (first_auth_time, _) = sign_in_first_time!(app, b, &rp, alice);
    let codes_before = codes_for(&db, &rp).await;

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    assert_eq!(resp.status(), 302);
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/select-account?txn="),
        "有 SSO 时必须先选择账号: {loc}"
    );
    let txn_id = txn_of(&loc);

    let alice_session = session_of(&db, &alice).await;
    // GET 只渲染：可重复打开，从不发码、从不确认选择。
    for _ in 0..2 {
        let resp = get!(app, b, loc.clone());
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");
        let csp = resp
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(csp.contains(&format!(
            "form-action 'self' {}",
            query_origin(&rp.redirect)
        )));
        let html = body_string!(resp);
        assert!(html.contains("选择账号"));
        assert!(html.contains("Chooser RP"));
        assert!(html.contains(&alice.nickname));
        assert!(html.contains(&format!("@{}", alice.username)));
        // 整行账号即「以该账号继续」：提交的是会话 id（不是用户 ID、不是用户名）。
        assert!(html.contains(&format!("name=\"account\" value=\"{}\"", alice_session.id)));
        // 头像字段不是平台上传的 /public/images/… 时不直接引用（显示首字母，不出现破图）。
        assert!(!html.contains("public&#x2f;avatars"));
        assert!(html.contains("使用其他账号"));
        assert!(html.contains("取消"));
        assert!(!html.contains(&alice.email), "选择页不展示邮箱");
        assert!(!html.contains(&alice.id.to_string()), "选择页不展示用户 ID");
    }
    let row = txn_row(&db, txn_id).await;
    assert!(row.completed_at.is_none() && row.account_selected_at.is_none());
    assert_eq!(codes_for(&db, &rp).await, codes_before, "未点击不发码");

    let (_, txn, csrf) = open_chooser!(app, b, loc);
    let resp = choose!(app, b, txn, csrf, "continue");
    assert_eq!(resp.status(), 302);
    let code = callback_code(&rp, &location(&resp));
    let (claims, _) = id_claims!(app, rp, code.as_str());
    assert_eq!(claims["sub"], alice.id.to_string());
    assert_eq!(
        claims["auth_time"].as_i64().unwrap(),
        first_auth_time,
        "继续当前账号不重新认证"
    );
    let row = txn_row(&db, txn_id).await;
    assert!(row.account_selected_at.is_some() && row.completed_at.is_some());

    // 重放已完成事务
    let resp = choose!(app, b, txn, csrf, "continue");
    assert_eq!(resp.status(), 400);
}

fn query_origin(url: &str) -> String {
    url::Url::parse(url).unwrap().origin().ascii_serialization()
}

// ── 04：使用其他账号 → 强制密码 + 独立授权 ─────────────────────────────────

#[actix_web::test]
async fn other_account_requires_password_and_its_own_consent() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let bob = seed_user(&db, "bob").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);
    let alice_session = session_of(&db, &alice).await;

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let chooser = location(&resp);
    let txn_id = txn_of(&chooser);
    let (_, txn, csrf) = open_chooser!(app, b, chooser.clone());
    let resp = choose!(app, b, txn, csrf, "other");
    assert_eq!(resp.status(), 302);
    let login = location(&resp);
    assert!(login.starts_with("/oauth/login?txn="), "{login}");
    assert!(
        txn_row(&db, txn_id)
            .await
            .account_switch_requested_at
            .is_some()
    );

    // 登录页明确是「使用其他账号」，不预填当前账号
    let html = body_string!(get!(app, b, login.clone()));
    assert!(html.contains("使用其他账号登录"));
    assert!(!html.contains(&alice.username), "不得预填原账号");

    // 旧 Cookie 不能把「其他账号」短路回 A：回到选择页只会被送回登录页；旧 CSRF 失效
    let resp = get!(app, b, chooser.clone());
    assert_eq!(resp.status(), 302);
    assert!(location(&resp).starts_with("/oauth/login?txn="));
    let resp = choose!(app, b, txn, csrf, "continue");
    assert_eq!(resp.status(), 400, "已切换的事务不可用旧页面继续 A");
    // 直接 POST 同意页也不行（A 仍有 consent，但状态机要求先输入密码）
    let resp = post_form!(
        app,
        b,
        "/oauth/consent",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf.as_str()),
            ("action", "allow")
        ]
    );
    assert_eq!(
        resp.status(),
        400,
        "跳过密码不能发码（同意页 CSRF 已随切换作废）"
    );
    assert!(!location(&resp).starts_with(&rp.redirect));
    assert!(
        b.cookies.contains_key(SSO),
        "选择「其他账号」本身不清除 SSO"
    );

    // 输入 B 的密码 → B 没有授权记录 → 同意页显示 B → 回调 sub=B
    let loc = do_login!(app, b, login, bob.username.as_str());
    assert!(
        loc.starts_with("/oauth/consent?txn="),
        "B 需要独立授权: {loc}"
    );
    let consent_html = body_string!(get!(app, b, loc.clone()));
    assert!(consent_html.contains(&bob.username));
    assert!(!consent_html.contains(&alice.username));
    let loc = allow_consent!(app, b, loc);
    let code = callback_code(&rp, &loc);
    let (claims, _) = id_claims!(app, rp, code.as_str());
    assert_eq!(claims["sub"], bob.id.to_string(), "绝不串号");

    // 单当前会话模型：A 的旧 SSO 已撤销；两人的授权各自独立
    let old = oidc_browser_sessions::Entity::find_by_id(alice_session.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(old.revoked_at.is_some());
    let consents = oidc_consents::Entity::find()
        .filter(oidc_consents::Column::OidcClientId.eq(rp.model.id))
        .filter(oidc_consents::Column::RevokedAt.is_null())
        .count(&db)
        .await
        .unwrap();
    assert_eq!(consents, 2);

    // 之后 B 再次登录：选择页展示的是 B
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let (html, _, _) = open_chooser!(app, b, location(&resp));
    assert!(html.contains(&bob.username) && !html.contains(&alice.username));
}

// ── 05 / 06：consent / login 不显示选择页 ────────────────────────────────────

#[actix_web::test]
async fn consent_and_login_prompts_do_not_show_the_chooser() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    let (first_auth_time, _) = sign_in_first_time!(app, b, &rp, alice);

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "consent")]));
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/consent?txn="),
        "prompt=consent → 同意页: {loc}"
    );
    let loc = allow_consent!(app, b, loc);
    callback_code(&rp, &loc);

    std::thread::sleep(std::time::Duration::from_millis(1100));
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "login")]));
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/login?txn="),
        "prompt=login → 重新认证: {loc}"
    );
    let html = body_string!(get!(app, b, loc.clone()));
    assert!(html.contains("该应用要求你重新验证身份"));
    let loc = do_login!(app, b, loc, alice.username.as_str());
    let code = callback_code(&rp, &loc);
    let (claims, _) = id_claims!(app, rp, code.as_str());
    assert!(
        claims["auth_time"].as_i64().unwrap() > first_auth_time,
        "auth_time 更新"
    );
}

// ── 07 / 08：select_account 与 consent / login / max_age 组合 ─────────────────

#[actix_web::test]
async fn select_account_with_consent_and_login_are_separate_steps() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    let (first_auth_time, _) = sign_in_first_time!(app, b, &rp, alice);

    // select_account consent：选择页 → 同意页（两页）
    let resp = get!(
        app,
        b,
        auth_url(&rp, &[("prompt", "select_account consent")])
    );
    let loc = location(&resp);
    assert!(loc.starts_with("/oauth/select-account?txn="), "{loc}");
    let (_, txn, csrf) = open_chooser!(app, b, loc);
    let resp = choose!(app, b, txn, csrf, "continue");
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/consent?txn="),
        "选择后再同意: {loc}"
    );
    let loc = allow_consent!(app, b, loc);
    callback_code(&rp, &loc);

    // select_account login：选择页 → 重新认证（预填所选账号，仍必须输入密码）
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account login")]));
    let loc = location(&resp);
    assert!(loc.starts_with("/oauth/select-account?txn="), "{loc}");
    let (_, txn, csrf) = open_chooser!(app, b, loc);
    let resp = choose!(app, b, txn, csrf, "continue");
    let login = location(&resp);
    assert!(
        login.starts_with("/oauth/login?txn="),
        "选择后重新认证: {login}"
    );
    let html = body_string!(get!(app, b, login.clone()));
    assert!(html.contains("该应用要求你重新验证身份"));
    assert!(html.contains(&format!("value=\"{}\"", alice.username)));
    let loc = do_login!(app, b, login, alice.username.as_str());
    let code = callback_code(&rp, &loc);
    let (claims, _) = id_claims!(app, rp, code.as_str());
    assert!(claims["auth_time"].as_i64().unwrap() > first_auth_time);

    // select_account + max_age=0：同样先选择再重新认证
    let resp = get!(
        app,
        b,
        auth_url(&rp, &[("prompt", "select_account"), ("max_age", "0")])
    );
    let loc = location(&resp);
    assert!(loc.starts_with("/oauth/select-account?txn="), "{loc}");
    let (_, txn, csrf) = open_chooser!(app, b, loc);
    let resp = choose!(app, b, txn, csrf, "continue");
    assert!(location(&resp).starts_with("/oauth/login?txn="));
}

// ── 09 / 10：prompt=none 永不渲染页面 ────────────────────────────────────────

#[actix_web::test]
async fn prompt_none_never_renders_the_chooser() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let other_rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "none")]));
    callback_code(&rp, &location(&resp));

    let resp = get!(app, b, auth_url(&other_rp, &[("prompt", "none")]));
    let q = query_of(&location(&resp));
    assert_eq!(q["error"], "consent_required");
    assert_eq!(q["state"], "st-chooser");
    assert_eq!(q["iss"], ISSUER);

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "none select_account")]));
    let loc = location(&resp);
    assert!(loc.starts_with(&rp.redirect), "{loc}");
    assert_eq!(query_of(&loc)["error"], "invalid_request");

    let mut fresh = Browser::default();
    let resp = get!(app, fresh, auth_url(&rp, &[("prompt", "none")]));
    assert_eq!(query_of(&location(&resp))["error"], "login_required");
}

// ── 11：取消 ────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn cancel_returns_access_denied_with_state_and_issuer() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);
    let codes_before = codes_for(&db, &rp).await;

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let resp = choose!(app, b, txn, csrf, "cancel");
    assert_eq!(resp.status(), 302);
    let loc = location(&resp);
    assert!(loc.starts_with(&rp.redirect), "{loc}");
    let q = query_of(&loc);
    assert_eq!(q["error"], "access_denied");
    assert_eq!(q["state"], "st-chooser");
    assert_eq!(q["iss"], ISSUER);
    assert!(!q.contains_key("code"));
    assert_eq!(codes_for(&db, &rp).await, codes_before);
    // 取消不影响 FloatCTF SSO（不伪造退出）
    assert!(b.cookies.contains_key(SSO));
    // 已结束：不能再继续
    let resp = choose!(app, b, txn, csrf, "continue");
    assert_eq!(resp.status(), 400);
}

// ── 12：失效会话不能免密继续 ─────────────────────────────────────────────────

#[actix_web::test]
async fn revoked_or_expired_sessions_are_never_offered_or_continued() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);

    // 选择页已展示，提交前会话被撤销（例如在 FloatCTF 主站退出）→ 继续 = 登录页，不发码
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let session = session_of(&db, &alice).await;
    let mut active = session.clone().into_active_model();
    active.revoked_at = Set(Some(chrono::Utc::now().fixed_offset()));
    active.update(&db).await.unwrap();
    let resp = choose!(app, b, txn, csrf, "continue");
    assert_eq!(resp.status(), 302);
    assert!(
        location(&resp).starts_with("/oauth/login?txn="),
        "失效会话不能免密继续"
    );

    // 过期会话：不会被展示为可继续的账号
    let mut b2 = Browser::default();
    sign_in_first_time!(app, b2, &rp, alice);
    let session = session_of(&db, &alice).await;
    let mut active = session.into_active_model();
    active.expires_at = Set(chrono::Utc::now().fixed_offset() - chrono::Duration::seconds(5));
    active.update(&db).await.unwrap();
    let resp = get!(app, b2, auth_url(&rp, &[("prompt", "select_account")]));
    assert!(location(&resp).starts_with("/oauth/login?txn="));

    // 用户被删除：会话随之失效（外键级联），同样只能走登录页
    let carol = seed_user(&db, "carol").await;
    let mut b3 = Browser::default();
    sign_in_first_time!(app, b3, &rp, carol);
    users::Entity::delete_by_id(carol.id)
        .exec(&db)
        .await
        .unwrap();
    let resp = get!(app, b3, auth_url(&rp, &[("prompt", "select_account")]));
    assert!(location(&resp).starts_with("/oauth/login?txn="));
}

// ── 13 / 14：事务与表单防护 ─────────────────────────────────────────────────

#[actix_web::test]
async fn chooser_forms_resist_csrf_cross_browser_cross_site_and_expiry() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let mallory = seed_user(&db, "mallory").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);
    let codes_before = codes_for(&db, &rp).await;

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let chooser = location(&resp);
    let txn_id = txn_of(&chooser);
    let (_, txn, csrf) = open_chooser!(app, b, chooser.clone());

    // 伪造 CSRF
    let resp = choose!(app, b, txn, "forged".to_string(), "continue");
    assert_eq!(resp.status(), 400);
    // 另一个浏览器（无绑定 Cookie，但带着 A 的 SSO Cookie 与正确 CSRF）
    let mut thief = Browser::default();
    thief.cookies.insert(SSO.into(), b.cookies[SSO].clone());
    let resp = choose!(app, thief, txn, csrf, "continue");
    assert_eq!(resp.status(), 400);
    let resp = get!(app, thief, chooser.clone());
    assert_eq!(resp.status(), 400, "换浏览器打不开选择页");
    // 跨站 Origin
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/select-account")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header(("Origin", "https://evil.example"))
            .insert_header(("Cookie", b.header()))
            .set_payload(form(&[
                ("txn", txn.as_str()),
                ("csrf", csrf.as_str()),
                ("action", "continue"),
            ]))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    // 非表单 Content-Type
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/select-account")
            .insert_header(("Content-Type", "application/json"))
            .insert_header(("Origin", ISSUER))
            .insert_header(("Cookie", b.header()))
            .set_payload(format!(
                r#"{{"txn":"{txn}","csrf":"{csrf}","action":"continue"}}"#
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    // 未知动作 / 未知事务
    let resp = choose!(app, b, txn, csrf, "select");
    assert_eq!(resp.status(), 400);
    let resp = choose!(app, b, Uuid::new_v4().to_string(), csrf, "continue");
    assert_eq!(resp.status(), 400);
    let resp = get!(
        app,
        b,
        format!("/oauth/select-account?txn={}", Uuid::new_v4())
    );
    assert_eq!(resp.status(), 400);
    let resp = get!(app, b, "/oauth/select-account?txn=not-a-uuid");
    assert_eq!(resp.status(), 400);
    let row = txn_row(&db, txn_id).await;
    assert!(row.account_selected_at.is_none() && row.completed_at.is_none());

    // 伪造的用户 / 会话字段被完全忽略：提交 mallory 的 id 仍只会选中浏览器自己的 A
    let resp = post_form!(
        app,
        b,
        "/oauth/select-account",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf.as_str()),
            ("action", "continue"),
            ("user_id", mallory.id.to_string().as_str()),
            ("session_id", Uuid::new_v4().to_string().as_str()),
            ("username", mallory.username.as_str())
        ]
    );
    let code = callback_code(&rp, &location(&resp));
    let (claims, _) = id_claims!(app, rp, code.as_str());
    assert_eq!(claims["sub"], alice.id.to_string());
    assert_eq!(codes_for(&db, &rp).await, codes_before + 1);

    // 过期事务
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let chooser = location(&resp);
    let (_, txn, csrf) = open_chooser!(app, b, chooser.clone());
    let mut active = txn_row(&db, txn_of(&chooser)).await.into_active_model();
    active.expires_at = Set(chrono::Utc::now().fixed_offset() - chrono::Duration::seconds(1));
    active.update(&db).await.unwrap();
    assert_eq!(get!(app, b, chooser).status(), 400);
    assert_eq!(choose!(app, b, txn, csrf, "continue").status(), 400);
    assert_eq!(choose!(app, b, txn, csrf, "cancel").status(), 400);
    assert_eq!(codes_for(&db, &rp).await, codes_before + 1);
}

// ── 15：另一标签页切换账号 ──────────────────────────────────────────────────

#[actix_web::test]
async fn account_change_in_another_tab_forces_reconfirmation_and_never_mixes_users() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let bob = seed_user(&db, "bob").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);

    // 标签 1：选择页展示 A
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let tab1 = location(&resp);
    let (html, txn1, csrf1) = open_chooser!(app, b, tab1.clone());
    assert!(html.contains(&alice.username));

    // 标签 2（同一浏览器，共享 Cookie）：使用其他账号登录 B 并完成授权
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let (_, txn2, csrf2) = open_chooser!(app, b, location(&resp));
    let login = location(&choose!(app, b, txn2, csrf2, "other"));
    let loc = do_login!(app, b, login, bob.username.as_str());
    let loc = allow_consent!(app, b, loc);
    callback_code(&rp, &loc);

    // 回到标签 1 点「继续」：浏览器现在是 B → 不得发 A 的码，也不悄悄选中 B，而是要求重新确认
    let codes_before = codes_for(&db, &rp).await;
    let resp = choose!(app, b, txn1, csrf1, "continue");
    assert_eq!(resp.status(), 200);
    let html = body_string!(resp);
    assert!(html.contains("登录账号已发生变化"));
    assert!(html.contains(&bob.username) && !html.contains(&alice.username));
    assert_eq!(codes_for(&db, &rp).await, codes_before);
    assert!(
        txn_row(&db, Uuid::parse_str(&txn1).unwrap())
            .await
            .account_selected_at
            .is_none()
    );
    // 重新确认后得到的是 B
    let csrf1b = hidden(&html, "csrf");
    let resp = choose!(app, b, txn1, csrf1b, "continue");
    let code = callback_code(&rp, &location(&resp));
    let (claims, _) = id_claims!(app, rp, code.as_str());
    assert_eq!(claims["sub"], bob.id.to_string());

    // 已在标签 1 选中 A 并停在同意页，随后另一标签切换到 B：同意页提交不会发 A 的码，
    // 也不会把 A 的选择套到 B 上（回到选择页确认 B）。
    let carol = seed_user(&db, "carol").await;
    let dave = seed_user(&db, "dave").await;
    let mut c = Browser::default();
    sign_in_first_time!(app, c, &rp, carol);
    let resp = get!(
        app,
        c,
        auth_url(&rp, &[("prompt", "select_account consent")])
    );
    let (_, t1, k1) = open_chooser!(app, c, location(&resp));
    let consent = location(&choose!(app, c, t1, k1, "continue"));
    assert!(consent.starts_with("/oauth/consent?txn="));
    let consent_html = body_string!(get!(app, c, consent.clone()));
    let consent_csrf = hidden(&consent_html, "csrf");
    let resp = get!(app, c, auth_url(&rp, &[("prompt", "login")]));
    let loc = do_login!(app, c, location(&resp), dave.username.as_str());
    assert!(
        loc.starts_with("/oauth/consent?txn="),
        "dave 首次授权: {loc}"
    );
    let resp = post_form!(
        app,
        c,
        "/oauth/consent",
        &[
            ("txn", t1.as_str()),
            ("csrf", consent_csrf.as_str()),
            ("action", "allow"),
            ("scope", "profile")
        ]
    );
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/select-account?txn="),
        "账号变化后必须重新确认: {loc}"
    );
    let (html, _, _) = open_chooser!(app, c, loc);
    assert!(html.contains(&dave.username) && !html.contains(&carol.username));
}

// ── 16：双击 / 并发「继续」只发一次码 ─────────────────────────────────────────

#[actix_web::test]
async fn concurrent_continue_issues_exactly_one_code() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = std::rc::Rc::new(app!(db, config.clone()));
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);
    let codes_before = codes_for(&db, &rp).await;

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let mut futures = Vec::new();
    for _ in 0..8 {
        let app = app.clone();
        let cookie = b.header();
        let payload = form(&[
            ("txn", txn.as_str()),
            ("csrf", csrf.as_str()),
            ("action", "continue"),
        ]);
        futures.push(async move {
            let req = test::TestRequest::post()
                .uri("/oauth/select-account")
                .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
                .insert_header(("Origin", ISSUER))
                .insert_header(("Cookie", cookie))
                .set_payload(payload)
                .to_request();
            let resp = test::call_service(&*app, req).await;
            (resp.status().as_u16(), location(&resp))
        });
    }
    let results = futures_util::future::join_all(futures).await;
    let winners: Vec<_> = results
        .iter()
        .filter(|(status, loc)| *status == 302 && loc.contains("code="))
        .collect();
    assert_eq!(winners.len(), 1, "只有一次成功: {results:?}");
    assert!(
        results
            .iter()
            .all(|(status, loc)| *status == 400 || loc.contains("code=")),
        "{results:?}"
    );
    assert_eq!(codes_for(&db, &rp).await, codes_before + 1);
}

// ── id_token_hint / 应用中途暂停 / 同意页切换 ─────────────────────────────────

#[actix_web::test]
async fn id_token_hint_blocks_switching_to_a_different_account() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let bob = seed_user(&db, "bob").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    let (_, alice_id_token) = sign_in_first_time!(app, b, &rp, alice);
    let alice_cookie = b.cookies[SSO].clone();
    let codes_before = codes_for(&db, &rp).await;

    let resp = get!(
        app,
        b,
        auth_url(
            &rp,
            &[
                ("prompt", "select_account"),
                ("id_token_hint", alice_id_token.as_str())
            ]
        )
    );
    let (_, txn, csrf) = open_chooser!(app, b, location(&resp));
    let login = location(&choose!(app, b, txn, csrf, "other"));
    let html = body_string!(get!(app, b, login.clone()));
    let resp = post_form!(
        app,
        b,
        "/oauth/login",
        &[
            ("txn", hidden(&html, "txn").as_str()),
            ("csrf", hidden(&html, "csrf").as_str()),
            ("action", "login"),
            ("username", bob.username.as_str()),
            ("password", PASSWORD)
        ]
    );
    assert_eq!(resp.status(), 200, "留在登录页");
    let html = body_string!(resp);
    assert!(html.contains("该应用要求使用指定的 FloatCTF 账号登录"));
    assert_eq!(b.cookies[SSO], alice_cookie, "不建立 B 的会话、不替换 A");
    assert!(session_of(&db, &alice).await.revoked_at.is_none());
    assert_eq!(codes_for(&db, &rp).await, codes_before);
    // 用指定账号 A 的密码可以完成
    let resp = post_form!(
        app,
        b,
        "/oauth/login",
        &[
            ("txn", hidden(&html, "txn").as_str()),
            ("csrf", hidden(&html, "csrf").as_str()),
            ("action", "login"),
            ("username", alice.username.as_str()),
            ("password", PASSWORD)
        ]
    );
    let code = callback_code(&rp, &location(&resp));
    let (claims, _) = id_claims!(app, rp, code.as_str());
    assert_eq!(claims["sub"], alice.id.to_string());
}

#[actix_web::test]
async fn client_suspended_mid_transaction_cannot_continue() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);
    let codes_before = codes_for(&db, &rp).await;

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let chooser = location(&resp);
    let (_, txn, csrf) = open_chooser!(app, b, chooser.clone());
    let client = oidc_clients::Entity::find_by_id(rp.model.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    clients::set_status(&db, client, OidcClientStatus::Suspended, None, None)
        .await
        .unwrap();
    assert_eq!(choose!(app, b, txn, csrf, "continue").status(), 400);
    assert_eq!(get!(app, b, chooser).status(), 400);
    assert_eq!(codes_for(&db, &rp).await, codes_before);
}

#[actix_web::test]
async fn consent_page_switch_goes_through_the_switch_state() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let bob = seed_user(&db, "bob").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "consent")]));
    let consent = location(&resp);
    let html = body_string!(get!(app, b, consent.clone()));
    assert!(html.contains("使用其他账号"));
    assert!(!html.contains("继续使用此账号"), "同意页不是账号选择页");
    let txn = hidden(&html, "txn");
    let csrf = hidden(&html, "csrf");
    let resp = post_form!(
        app,
        b,
        "/oauth/consent",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf.as_str()),
            ("action", "switch")
        ]
    );
    let login = location(&resp);
    assert!(login.starts_with("/oauth/login?txn="), "{login}");
    // 切换后旧同意页表单已失效，旧 SSO 也不能免密完成
    let resp = post_form!(
        app,
        b,
        "/oauth/consent",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf.as_str()),
            ("action", "allow")
        ]
    );
    assert_eq!(resp.status(), 400);
    let resp = get!(app, b, consent);
    assert!(location(&resp).starts_with("/oauth/login?txn="));
    let loc = do_login!(app, b, login, bob.username.as_str());
    let loc = allow_consent!(app, b, loc);
    let code = callback_code(&rp, &loc);
    let (claims, _) = id_claims!(app, rp, code.as_str());
    assert_eq!(claims["sub"], bob.id.to_string());
}

// ── 20 / 21：旧 RP 兼容与跨 RP ──────────────────────────────────────────────

#[actix_web::test]
async fn legacy_rp_without_prompt_keeps_silent_sso() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let learnos = seed_client(&db, &config, owner.id).await;
    let other = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &learnos, alice);

    // 同一 RP、不传 prompt：与旧版一致，合法 SSO 直接回调
    let resp = get!(app, b, auth_url(&learnos, &[]));
    callback_code(&learnos, &location(&resp));
    // 另一个 RP、不传 prompt：只需要该 RP 自己的同意，不出现选择页
    let resp = get!(app, b, auth_url(&other, &[]));
    let loc = location(&resp);
    assert!(loc.starts_with("/oauth/consent?txn="), "{loc}");
    let loc = allow_consent!(app, b, loc);
    let code = callback_code(&other, &loc);
    let (claims, _) = id_claims!(app, other, code.as_str());
    assert_eq!(claims["sub"], alice.id.to_string());
    assert_eq!(claims["aud"], other.model.client_id);
}

#[actix_web::test]
async fn rollback_switch_restores_legacy_consent_page_for_select_account() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = OidcConfig {
        account_chooser: false,
        ..test_config()
    };
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id).await;
    let mut b = Browser::default();
    sign_in_first_time!(app, b, &rp, alice);

    let resp = get!(app, b, auth_url(&rp, &[("prompt", "select_account")]));
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/consent?txn="),
        "回滚模式：旧同意页 {loc}"
    );
    assert!(b.cookies.contains_key(BID));
}
