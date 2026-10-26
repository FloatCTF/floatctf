//! G2：Authorization Code + PKCE + ID Token + UserInfo 端到端（进程内 HTTP + 真实 PostgreSQL）。
//!
//! 用最小"浏览器"（Cookie jar + 表单 CSRF 抓取）走完：authorize → 登录页 → 同意页 → 回调 →
//! token（client_secret_basic / post / none）→ JWKS 验签 → UserInfo，以及反例矩阵：
//! 授权码重放与并发兑换、跨 client / redirect、PKCE 错误/缺失/降级、过期、prompt=none、拒绝、
//! 撤销授权、禁用应用、伪造令牌、CSRF / 跨站 Origin / 换浏览器、max_age 强制重新认证。
//!
//! 需要 `DATABASE_URL` 指向已应用迁移的测试库；不可达时 soft-skip（与其他 DB-gated 测试一致）。

mod oidc_support;

use actix_web::{App, dev::ServiceResponse, test, web};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use oidc_support::*;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait, IntoActiveModel,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use floatctf::core::config::OidcConfig;
use floatctf::entity::sea_orm_active_enums::{
    OidcClientStatus, OidcClientType, OidcTokenAuthMethod,
};
use floatctf::entity::{oidc_authorization_codes, oidc_clients, users};
use floatctf::infrastructure::LogService;
use floatctf::modules::identity::oidc::clients::{self, ClientInput};

const PASSWORD: &str = "OidcTest@2026";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

// ── 测试夹具 ────────────────────────────────────────────────────────────────

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
        username: Set(format!("oidc-{tag}-{short}")),
        nickname: Set(format!("OIDC {tag} {short}")),
        password: Set(hash),
        email: Set(format!("oidc-{tag}-{short}@example.com")),
        created_at: Set(now),
        updated_at: Set(now),
        avatar: Set(Some("/public/avatars/test.png".into())),
    }
    .insert(db)
    .await
    .unwrap()
}

struct TestClient {
    model: oidc_clients::Model,
    secret: Option<String>,
    redirect: String,
}

async fn seed_client(
    db: &DatabaseConnection,
    config: &OidcConfig,
    owner: Uuid,
    client_type: OidcClientType,
    method: Option<OidcTokenAuthMethod>,
) -> TestClient {
    let redirect = format!(
        "https://rp-{}.example.com/callback",
        &Uuid::new_v4().simple().to_string()[..8]
    );
    let (model, secret) = clients::create(
        db,
        config,
        owner,
        client_type,
        method,
        &ClientInput {
            name: "Test RP".into(),
            description: None,
            homepage_url: Some("https://rp.example.com".into()),
            logo_url: None,
            redirect_uris: vec![redirect.clone()],
            scopes: vec!["openid".into(), "profile".into(), "email".into()],
            ..ClientInput::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(model.status, OidcClientStatus::Pending, "默认需要审批");
    let model = clients::set_status(db, model, OidcClientStatus::Active, None, None)
        .await
        .unwrap();
    TestClient {
        model,
        secret,
        redirect,
    }
}

fn basic(client: &TestClient) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!(
            "{}:{}",
            client.model.client_id,
            client.secret.as_deref().unwrap()
        ))
    )
}

fn auth_url(client: &TestClient, extra: &[(&str, &str)]) -> String {
    let mut pairs = vec![
        ("response_type", "code"),
        ("client_id", client.model.client_id.as_str()),
        ("redirect_uri", client.redirect.as_str()),
        ("scope", "openid profile email"),
        ("state", "st-123"),
        ("nonce", "n-456"),
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

/// GET 并吸收 Cookie。
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

/// 同源表单 POST 并吸收 Cookie。
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

/// 走完登录页（返回登录后的下一跳 Location）。
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

/// 在同意页做出选择（返回回调 Location）。
macro_rules! do_consent {
    ($app:expr, $browser:expr, $consent_location:expr, $action:expr, $scopes:expr) => {{
        let resp = get!($app, $browser, $consent_location);
        assert_eq!(resp.status(), 200, "consent page");
        let html = body_string!(resp);
        let txn = hidden(&html, "txn");
        let csrf = hidden(&html, "csrf");
        let mut pairs: Vec<(&str, &str)> = vec![
            ("txn", txn.as_str()),
            ("csrf", csrf.as_str()),
            ("action", $action),
        ];
        for s in $scopes {
            pairs.push(("scope", s));
        }
        let resp = post_form!($app, $browser, "/oauth/consent", &pairs);
        assert_eq!(resp.status(), 302, "consent submit");
        location(&resp)
    }};
}

macro_rules! token_request {
    ($app:expr, $auth:expr, $pairs:expr) => {{
        let mut req = test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form($pairs));
        if let Some(auth) = $auth {
            req = req.insert_header(("Authorization", auth));
        }
        test::call_service(&$app, req.to_request()).await
    }};
}

macro_rules! userinfo {
    ($app:expr, $token:expr) => {{
        test::call_service(
            &$app,
            test::TestRequest::get()
                .uri("/oauth/userinfo")
                .insert_header(("Authorization", format!("Bearer {}", $token)))
                .to_request(),
        )
        .await
    }};
}

/// 完整授权（登录 + 同意），返回 code。
macro_rules! full_authorization {
    ($app:expr, $browser:expr, $client:expr, $user:expr, $scopes:expr) => {{
        let resp = get!($app, $browser, auth_url($client, &[]));
        assert_eq!(resp.status(), 302);
        let loc = location(&resp);
        let loc = if loc.starts_with("/oauth/login") {
            do_login!($app, $browser, loc, $user.username.as_str())
        } else {
            loc
        };
        let loc = if loc.starts_with("/oauth/consent") {
            do_consent!($app, $browser, loc, "allow", $scopes)
        } else {
            loc
        };
        assert!(loc.starts_with(&$client.redirect), "callback: {loc}");
        let q = query_of(&loc);
        assert_eq!(q.get("state").map(String::as_str), Some("st-123"));
        assert_eq!(q.get("iss").map(String::as_str), Some(ISSUER));
        q["code"].clone()
    }};
}

async fn jwks_of(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = ServiceResponse,
        Error = actix_web::Error,
    >,
) -> JwkSet {
    let resp = test::call_service(
        app,
        test::TestRequest::get()
            .uri("/.well-known/jwks.json")
            .to_request(),
    )
    .await;
    test::read_body_json(resp).await
}

fn verify_id_token(jwks: &JwkSet, token: &str, client_id: &str) -> Value {
    let header = jsonwebtoken::decode_header(token).unwrap();
    assert_eq!(header.alg, Algorithm::RS256);
    let kid = header.kid.expect("kid");
    let key = DecodingKey::from_jwk(jwks.find(&kid).expect("kid in JWKS")).unwrap();
    let mut v = Validation::new(Algorithm::RS256);
    v.set_audience(&[client_id]);
    v.set_issuer(&[ISSUER]);
    v.set_required_spec_claims(&["iss", "sub", "aud", "exp", "iat"]);
    jsonwebtoken::decode::<Value>(token, &key, &v)
        .unwrap()
        .claims
}

// ── 正例 ────────────────────────────────────────────────────────────────────

#[actix_web::test]
async fn confidential_client_full_flow_with_sso_and_consent_memory() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    let mut browser = Browser::default();

    // 1) 未登录 → 登录页 → 同意页（勾选 profile，不勾 email）→ 回调
    let resp = get!(app, browser, auth_url(&rp, &[]));
    assert_eq!(resp.status(), 302);
    let loc = location(&resp);
    assert!(loc.starts_with("/oauth/login?txn="), "{loc}");
    let loc = do_login!(app, browser, loc, alice.username.as_str());
    assert!(loc.starts_with("/oauth/consent?txn="), "{loc}");
    let consent_html = {
        let resp = get!(app, browser, loc.clone());
        body_string!(resp)
    };
    assert!(consent_html.contains("Test RP"));
    assert!(consent_html.contains("rp.example.com"));
    assert!(consent_html.contains("第三方应用"));
    assert!(
        !consent_html.contains("value=\"email\" checked"),
        "email 默认不勾选"
    );
    let loc = do_consent!(app, browser, loc, "allow", ["profile"]);
    assert!(loc.starts_with(&rp.redirect), "{loc}");
    let q = query_of(&loc);
    assert_eq!(q["state"], "st-123");
    assert_eq!(q["iss"], ISSUER);
    let code = q["code"].clone();
    assert!(code.len() >= 43);

    // 2) client_secret_basic + PKCE 兑换
    let resp = token_request!(
        app,
        Some(basic(&rp)),
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", rp.redirect.as_str()),
            ("code_verifier", VERIFIER)
        ]
    );
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");
    assert_eq!(resp.headers().get("pragma").unwrap(), "no-cache");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["scope"], "openid profile");
    assert!(body["expires_in"].as_i64().unwrap() > 0);
    let access_token = body["access_token"].as_str().unwrap().to_string();
    let id_token = body["id_token"].as_str().unwrap();

    // 3) 只凭 JWKS 验签 ID Token；claims 正确
    let jwks = jwks_of(&app).await;
    let claims = verify_id_token(&jwks, id_token, &rp.model.client_id);
    assert_eq!(claims["iss"], ISSUER);
    assert_eq!(claims["sub"], alice.id.to_string());
    assert_eq!(claims["aud"], rp.model.client_id);
    assert_eq!(claims["azp"], rp.model.client_id);
    assert_eq!(claims["nonce"], "n-456");
    assert!(claims["auth_time"].as_i64().unwrap() <= claims["iat"].as_i64().unwrap());
    let expected_at_hash = URL_SAFE_NO_PAD.encode(&Sha256::digest(access_token.as_bytes())[..16]);
    assert_eq!(claims["at_hash"], expected_at_hash);
    // 资料 claims 只经 UserInfo 返回，ID Token 保持最小披露（OIDC Core §5.4）。
    assert!(claims.get("preferred_username").is_none());
    assert!(
        claims.get("email").is_none(),
        "资料 claims 不得出现在 ID Token"
    );
    for forbidden in ["role", "password", "admin"] {
        assert!(claims.get(forbidden).is_none());
    }

    // 4) UserInfo：sub 一致、按 scope 最小披露
    let resp = userinfo!(app, access_token);
    assert_eq!(resp.status(), 200);
    let info: Value = test::read_body_json(resp).await;
    assert_eq!(info["sub"], alice.id.to_string());
    assert_eq!(info["preferred_username"], alice.username);
    assert_eq!(info["picture"], format!("{ISSUER}/public/avatars/test.png"));
    assert!(info.get("email").is_none());
    assert!(info.get("password").is_none());

    // 5) SSO：同一浏览器再次授权（同 scope）→ 不再出现登录页与同意页，直接回调
    let resp = get!(app, browser, auth_url(&rp, &[("scope", "openid profile")]));
    assert_eq!(resp.status(), 302);
    let loc = location(&resp);
    assert!(
        loc.starts_with(&rp.redirect),
        "SSO + 已授权应直接回调: {loc}"
    );
    assert!(query_of(&loc).contains_key("code"));

    // 6) 申请新增 email → 需要重新同意（但无需再次登录）；勾选后 email_verified=false
    let resp = get!(app, browser, auth_url(&rp, &[]));
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/consent"),
        "新增 scope 需同意: {loc}"
    );
    let loc = do_consent!(app, browser, loc, "allow", ["profile", "email"]);
    let code = query_of(&loc)["code"].clone();
    let resp = token_request!(
        app,
        Some(basic(&rp)),
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", rp.redirect.as_str()),
            ("code_verifier", VERIFIER)
        ]
    );
    let body: Value = test::read_body_json(resp).await;
    let info: Value =
        test::read_body_json(userinfo!(app, body["access_token"].as_str().unwrap())).await;
    assert_eq!(info["email"], alice.email);
    assert_eq!(info["email_verified"], false);
}

#[actix_web::test]
async fn post_and_public_clients_and_subject_stability() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let bob = seed_user(&db, "bob").await;
    let carol = seed_user(&db, "carol").await;
    let post_rp = seed_client(
        &db,
        &config,
        owner.id,
        OidcClientType::Confidential,
        Some(OidcTokenAuthMethod::ClientSecretPost),
    )
    .await;
    let spa = seed_client(&db, &config, owner.id, OidcClientType::Public, None).await;
    assert!(spa.secret.is_none(), "公开客户端不发 secret");

    let mut b = Browser::default();
    let code = full_authorization!(app, b, &post_rp, bob, ["profile"]);
    let resp = token_request!(
        app,
        None::<String>,
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", post_rp.redirect.as_str()),
            ("code_verifier", VERIFIER),
            ("client_id", post_rp.model.client_id.as_str()),
            ("client_secret", post_rp.secret.as_deref().unwrap()),
        ]
    );
    assert_eq!(resp.status(), 200, "client_secret_post");
    let post_body: Value = test::read_body_json(resp).await;

    // 公开客户端：同一浏览器 SSO，第一次仍需同意
    let resp = get!(app, b, auth_url(&spa, &[]));
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/consent"),
        "同用户新应用需同意（不需登录）: {loc}"
    );
    let loc = do_consent!(app, b, loc, "allow", ["profile"]);
    let code = query_of(&loc)["code"].clone();
    let resp = token_request!(
        app,
        None::<String>,
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", spa.redirect.as_str()),
            ("code_verifier", VERIFIER),
            ("client_id", spa.model.client_id.as_str()),
        ]
    );
    assert_eq!(resp.status(), 200, "public client none + PKCE");
    let spa_body: Value = test::read_body_json(resp).await;

    let jwks = jwks_of(&app).await;
    let c1 = verify_id_token(
        &jwks,
        post_body["id_token"].as_str().unwrap(),
        &post_rp.model.client_id,
    );
    let c2 = verify_id_token(
        &jwks,
        spa_body["id_token"].as_str().unwrap(),
        &spa.model.client_id,
    );
    assert_eq!(
        c1["sub"], c2["sub"],
        "public subject：同一用户跨应用 sub 一致"
    );
    assert_ne!(c1["aud"], c2["aud"]);

    // 另一个用户 sub 不同
    let mut b2 = Browser::default();
    let code = full_authorization!(app, b2, &post_rp, carol, ["profile"]);
    let resp = token_request!(
        app,
        None::<String>,
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", post_rp.redirect.as_str()),
            ("code_verifier", VERIFIER),
            ("client_id", post_rp.model.client_id.as_str()),
            ("client_secret", post_rp.secret.as_deref().unwrap()),
        ]
    );
    let body: Value = test::read_body_json(resp).await;
    let c3 = verify_id_token(
        &jwks,
        body["id_token"].as_str().unwrap(),
        &post_rp.model.client_id,
    );
    assert_ne!(c1["sub"], c3["sub"]);
    assert_eq!(c3["sub"], carol.id.to_string());

    // 公开客户端不带 PKCE → 带 state 与 iss 的 invalid_request 回调
    let resp = get!(
        app,
        b,
        auth_url(
            &spa,
            &[("code_challenge", ""), ("code_challenge_method", "")]
        )
    );
    let q = query_of(&location(&resp));
    assert_eq!(q["error"], "invalid_request");
    assert_eq!(q["state"], "st-123");
    assert_eq!(q["iss"], ISSUER);
}

// ── 反例：授权端点 ──────────────────────────────────────────────────────────

#[actix_web::test]
async fn authorization_endpoint_never_redirects_to_unverified_targets() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    let mut b = Browser::default();

    let attacker = "https://attacker.example.net/cb";
    for uri in [
        auth_url(&rp, &[("redirect_uri", attacker)]),
        auth_url(&rp, &[("redirect_uri", &format!("{}/extra", rp.redirect))]),
        auth_url(&rp, &[("redirect_uri", "")]),
        auth_url(&rp, &[("client_id", "fctf_does_not_exist")]),
        auth_url(&rp, &[("client_id", "")]),
        format!("{}&redirect_uri={}", auth_url(&rp, &[]), enc(attacker)),
    ] {
        let resp = get!(app, b, uri.clone());
        assert_eq!(resp.status(), 400, "{uri}");
        assert!(
            resp.headers().get("location").is_none(),
            "must not redirect: {uri}"
        );
    }

    // 待审批 / 被暂停的应用同样不重定向
    let pending = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    clients::set_status(
        &db,
        pending.model.clone(),
        OidcClientStatus::Suspended,
        None,
        None,
    )
    .await
    .unwrap();
    let resp = get!(app, b, auth_url(&pending, &[]));
    assert_eq!(resp.status(), 400);
    assert!(resp.headers().get("location").is_none());

    // 回调合法后的参数错误 → 标准错误回调
    for (extra, expected) in [
        (
            vec![("response_type", "token")],
            "unsupported_response_type",
        ),
        (vec![("scope", "profile")], "invalid_scope"),
        (vec![("code_challenge_method", "plain")], "invalid_request"),
        (
            vec![("request", "eyJhbGciOiJub25lIn0.e30.")],
            "request_not_supported",
        ),
        (vec![("prompt", "none login")], "invalid_request"),
    ] {
        let resp = get!(app, b, auth_url(&rp, &extra));
        assert_eq!(resp.status(), 302);
        let loc = location(&resp);
        assert!(loc.starts_with(&rp.redirect), "{loc}");
        assert_eq!(query_of(&loc)["error"], expected, "{extra:?}");
    }

    // prompt=none 未登录 → login_required
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "none")]));
    let q = query_of(&location(&resp));
    assert_eq!(q["error"], "login_required");
    assert_eq!(q["state"], "st-123");
}

#[actix_web::test]
async fn prompt_none_consent_required_deny_and_max_age() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let dave = seed_user(&db, "dave").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    let mut b = Browser::default();

    // 登录但拒绝 → access_denied
    let resp = get!(app, b, auth_url(&rp, &[]));
    let loc = do_login!(app, b, location(&resp), dave.username.as_str());
    let loc = do_consent!(app, b, loc, "deny", Vec::<&str>::new());
    let q = query_of(&loc);
    assert!(loc.starts_with(&rp.redirect));
    assert_eq!(q["error"], "access_denied");
    assert_eq!(q["state"], "st-123");
    assert!(!q.contains_key("code"));

    // 已登录、未授权 + prompt=none → consent_required
    let resp = get!(app, b, auth_url(&rp, &[("prompt", "none")]));
    assert_eq!(query_of(&location(&resp))["error"], "consent_required");

    // 授权后 prompt=none 静默成功
    let code = full_authorization!(app, b, &rp, dave, ["profile"]);
    let _ = code;
    let resp = get!(
        app,
        b,
        auth_url(&rp, &[("prompt", "none"), ("scope", "openid profile")])
    );
    assert!(query_of(&location(&resp)).contains_key("code"));

    // max_age=0 / prompt=login → 即使已登录也必须重新输入密码，并刷新 auth_time
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let resp = get!(
        app,
        b,
        auth_url(&rp, &[("max_age", "0"), ("scope", "openid profile")])
    );
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/login"),
        "max_age=0 必须重新登录: {loc}"
    );
    let before = chrono::Utc::now().timestamp();
    let loc = do_login!(app, b, loc, dave.username.as_str());
    assert!(
        loc.starts_with(&rp.redirect),
        "已授权，登录后直接回调: {loc}"
    );
    let code = query_of(&loc)["code"].clone();
    let resp = token_request!(
        app,
        Some(basic(&rp)),
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", rp.redirect.as_str()),
            ("code_verifier", VERIFIER)
        ]
    );
    let body: Value = test::read_body_json(resp).await;
    let claims = verify_id_token(
        &jwks_of(&app).await,
        body["id_token"].as_str().unwrap(),
        &rp.model.client_id,
    );
    assert!(
        claims["auth_time"].as_i64().unwrap() >= before,
        "auth_time 必须是新认证时间"
    );

    let resp = get!(
        app,
        b,
        auth_url(&rp, &[("prompt", "login"), ("scope", "openid profile")])
    );
    assert!(
        location(&resp).starts_with("/oauth/login"),
        "prompt=login 必须重新登录"
    );
}

#[actix_web::test]
async fn login_and_consent_forms_resist_csrf_and_cross_browser_use() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let erin = seed_user(&db, "erin").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;

    let mut victim = Browser::default();
    let resp = get!(app, victim, auth_url(&rp, &[]));
    let login_loc = location(&resp);
    let html = body_string!(get!(app, victim, login_loc.clone()));
    let txn = hidden(&html, "txn");
    let csrf = hidden(&html, "csrf");

    // 错误 CSRF
    let resp = post_form!(
        app,
        victim,
        "/oauth/login",
        &[
            ("txn", txn.as_str()),
            ("csrf", "forged"),
            ("action", "login"),
            ("username", erin.username.as_str()),
            ("password", PASSWORD)
        ]
    );
    assert_eq!(resp.status(), 400);
    // 另一个浏览器（没有绑定 Cookie）拿着事务 id + 正确 CSRF 也无法推进
    let mut other = Browser::default();
    let resp = post_form!(
        app,
        other,
        "/oauth/login",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf.as_str()),
            ("action", "login"),
            ("username", erin.username.as_str()),
            ("password", PASSWORD)
        ]
    );
    assert_eq!(resp.status(), 400);
    assert!(
        !other.cookies.keys().any(|k| k.contains("sso")),
        "不得建立会话"
    );
    // 跨站 Origin
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/login")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header(("Origin", "https://evil.example"))
            .insert_header(("Cookie", victim.header()))
            .set_payload(form(&[
                ("txn", txn.as_str()),
                ("csrf", csrf.as_str()),
                ("action", "login"),
                ("username", erin.username.as_str()),
                ("password", PASSWORD),
            ]))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    // 错误密码：留在登录页，不发会话
    let resp = post_form!(
        app,
        victim,
        "/oauth/login",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf.as_str()),
            ("action", "login"),
            ("username", erin.username.as_str()),
            ("password", "wrong-password")
        ]
    );
    assert_eq!(resp.status(), 200);
    let html = body_string!(resp);
    assert!(html.contains("用户名或密码错误"));
    assert!(!victim.cookies.keys().any(|k| k.contains("sso")));

    // 正常完成后，同一事务不可再次使用（双击/重放）
    let csrf2 = hidden(&html, "csrf");
    let resp = post_form!(
        app,
        victim,
        "/oauth/login",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf2.as_str()),
            ("action", "login"),
            ("username", erin.username.as_str()),
            ("password", PASSWORD)
        ]
    );
    let consent_loc = location(&resp);
    assert!(consent_loc.starts_with("/oauth/consent"));
    let html = body_string!(get!(app, victim, consent_loc.clone()));
    let csrf3 = hidden(&html, "csrf");
    let resp = post_form!(
        app,
        victim,
        "/oauth/consent",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf3.as_str()),
            ("action", "allow")
        ]
    );
    assert!(location(&resp).starts_with(&rp.redirect));
    let resp = post_form!(
        app,
        victim,
        "/oauth/consent",
        &[
            ("txn", txn.as_str()),
            ("csrf", csrf3.as_str()),
            ("action", "allow")
        ]
    );
    assert_eq!(resp.status(), 400, "已完成的事务不可重放");
}

// ── 反例：Token 端点 ────────────────────────────────────────────────────────

#[actix_web::test]
async fn token_endpoint_negative_matrix() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let frank = seed_user(&db, "frank").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    let other = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    let mut b = Browser::default();

    let code = full_authorization!(app, b, &rp, frank, ["profile"]);
    let good = |code: &str| {
        vec![
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("code".to_string(), code.to_string()),
            ("redirect_uri".to_string(), rp.redirect.clone()),
            ("code_verifier".to_string(), VERIFIER.to_string()),
        ]
    };
    macro_rules! expect_error {
        ($auth:expr, $pairs:expr, $status:expr, $error:expr) => {{
            let pairs: Vec<(String, String)> = $pairs;
            let refs: Vec<(&str, &str)> = pairs
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let resp = token_request!(app, $auth, &refs);
            assert_eq!(resp.status().as_u16(), $status, "{:?}", refs);
            assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(body["error"], $error, "{:?}", refs);
        }};
    }
    let with = |mut v: Vec<(String, String)>, k: &str, val: &str| {
        v.retain(|(pk, _)| pk != k);
        if !val.is_empty() {
            v.push((k.to_string(), val.to_string()));
        }
        v
    };

    // 客户端认证失败
    let wrong_secret = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD
            .encode(format!("{}:fcs_wrong", rp.model.client_id))
    );
    let resp = token_request!(
        app,
        Some(wrong_secret.clone()),
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str())
        ]
    );
    assert_eq!(resp.status(), 401);
    assert!(
        resp.headers()
            .get("www-authenticate")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("Basic")
    );
    // 登记 basic 却用 post
    expect_error!(
        None::<String>,
        with(
            with(good(&code), "client_id", &rp.model.client_id),
            "client_secret",
            rp.secret.as_deref().unwrap()
        ),
        401,
        "invalid_client"
    );
    // 无认证
    expect_error!(None::<String>, good(&code), 401, "invalid_client");
    // 跨 client：另一个合法 client 拿这个 code → invalid_grant，且不会把 code 消耗掉
    expect_error!(
        Some(basic(&other)),
        with(good(&code), "redirect_uri", &other.redirect),
        400,
        "invalid_grant"
    );
    // redirect_uri 不一致 / 缺失
    expect_error!(
        Some(basic(&rp)),
        with(good(&code), "redirect_uri", "https://rp.example.com/other"),
        400,
        "invalid_grant"
    );
    expect_error!(
        Some(basic(&rp)),
        with(good(&code), "redirect_uri", ""),
        400,
        "invalid_request"
    );
    // PKCE：错误 verifier / 缺失 verifier
    expect_error!(
        Some(basic(&rp)),
        with(good(&code), "code_verifier", &"a".repeat(43)),
        400,
        "invalid_grant"
    );
    expect_error!(
        Some(basic(&rp)),
        with(good(&code), "code_verifier", ""),
        400,
        "invalid_grant"
    );
    // grant_type
    expect_error!(
        Some(basic(&rp)),
        with(good(&code), "grant_type", "password"),
        400,
        "unsupported_grant_type"
    );
    expect_error!(
        Some(basic(&rp)),
        with(good(&code), "grant_type", "client_credentials"),
        400,
        "unsupported_grant_type"
    );
    expect_error!(
        Some(basic(&rp)),
        with(good(&code), "grant_type", ""),
        400,
        "invalid_request"
    );
    // 伪造 code
    expect_error!(
        Some(basic(&rp)),
        with(
            good(&code),
            "code",
            "forged-code-value-forged-code-value-forged"
        ),
        400,
        "invalid_grant"
    );
    // 非表单
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Authorization", basic(&rp)))
            .set_json(serde_json::json!({"grant_type": "authorization_code", "code": code}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    // GET 不接受
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/oauth/token?code={code}"))
            .to_request(),
    )
    .await;
    assert_ne!(resp.status(), 200);

    // 经过以上失败，正确请求仍然成功（绑定校验失败不消耗 code）
    let refs: Vec<(String, String)> = good(&code);
    let refs: Vec<(&str, &str)> = refs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let resp = token_request!(app, Some(basic(&rp)), &refs);
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let first_token = body["access_token"].as_str().unwrap().to_string();
    assert_eq!(userinfo!(app, first_token).status(), 200);

    // 重放：拒绝，并撤销第一次签发的 access token
    expect_error!(Some(basic(&rp)), good(&code), 400, "invalid_grant");
    assert_eq!(
        userinfo!(app, first_token).status(),
        401,
        "重放后已签发令牌必须被撤销"
    );

    // 过期 code
    let code2 = full_authorization!(app, b, &rp, frank, ["profile"]);
    let row = oidc_authorization_codes::Entity::find()
        .all(&db)
        .await
        .unwrap()
        .into_iter()
        .find(|r| {
            r.code_hash == floatctf::modules::identity::oidc::domain::secret::sha256_hex(&code2)
        })
        .unwrap();
    let mut active = row.into_active_model();
    active.expires_at = Set((chrono::Utc::now() - chrono::Duration::seconds(5)).fixed_offset());
    active.update(&db).await.unwrap();
    expect_error!(Some(basic(&rp)), good(&code2), 400, "invalid_grant");
}

#[actix_web::test]
async fn concurrent_code_exchange_succeeds_at_most_once() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = std::rc::Rc::new(app!(db, config.clone()));
    let owner = seed_user(&db, "owner").await;
    let gina = seed_user(&db, "gina").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    let mut b = Browser::default();
    let code = full_authorization!(app, b, &rp, gina, ["profile"]);

    let mut futures = Vec::new();
    for _ in 0..8 {
        let app = app.clone();
        let auth = basic(&rp);
        let payload = form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", rp.redirect.as_str()),
            ("code_verifier", VERIFIER),
        ]);
        futures.push(async move {
            let req = test::TestRequest::post()
                .uri("/oauth/token")
                .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
                .insert_header(("Authorization", auth))
                .set_payload(payload)
                .to_request();
            test::call_service(&*app, req).await.status().as_u16()
        });
    }
    let statuses = futures_util::future::join_all(futures).await;
    let ok = statuses.iter().filter(|s| **s == 200).count();
    assert!(ok <= 1, "授权码并发兑换最多成功一次: {statuses:?}");
    assert!(
        statuses.iter().all(|s| *s == 200 || *s == 400),
        "{statuses:?}"
    );
}

#[actix_web::test]
async fn pkce_downgrade_rejected_for_nonce_only_confidential_client() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let hank = seed_user(&db, "hank").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    // 管理员为该机密客户端关闭强制 PKCE（例如 OIDF 测试客户端）。
    let mut active = rp.model.clone().into_active_model();
    active.require_pkce = Set(false);
    active.update(&db).await.unwrap();
    let mut b = Browser::default();

    // 不带 PKCE 也不带 nonce → invalid_request
    let resp = get!(
        app,
        b,
        auth_url(
            &rp,
            &[
                ("code_challenge", ""),
                ("code_challenge_method", ""),
                ("nonce", "")
            ]
        )
    );
    assert_eq!(query_of(&location(&resp))["error"], "invalid_request");

    // 不带 PKCE、带 nonce → 可以授权；但兑换时附带 code_verifier 视为降级攻击
    let resp = get!(
        app,
        b,
        auth_url(
            &rp,
            &[("code_challenge", ""), ("code_challenge_method", "")]
        )
    );
    let loc = do_login!(app, b, location(&resp), hank.username.as_str());
    let loc = do_consent!(app, b, loc, "allow", ["profile"]);
    let code = query_of(&loc)["code"].clone();
    let resp = token_request!(
        app,
        Some(basic(&rp)),
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", rp.redirect.as_str()),
            ("code_verifier", VERIFIER)
        ]
    );
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["error"], "invalid_grant");
    // 正确地不带 verifier 兑换成功，nonce 写入 ID Token
    let resp = token_request!(
        app,
        Some(basic(&rp)),
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", rp.redirect.as_str())
        ]
    );
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let claims = verify_id_token(
        &jwks_of(&app).await,
        body["id_token"].as_str().unwrap(),
        &rp.model.client_id,
    );
    assert_eq!(claims["nonce"], "n-456");
}

// ── 反例：UserInfo / 撤销 ───────────────────────────────────────────────────

#[actix_web::test]
async fn userinfo_rejects_forged_revoked_and_disabled_access() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let ivy = seed_user(&db, "ivy").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    let mut b = Browser::default();

    // 无令牌 / 伪造令牌 / 把 ID Token 当 access token
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/oauth/userinfo").to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);
    assert_eq!(
        resp.headers().get("www-authenticate").unwrap(),
        "Bearer realm=\"floatctf\""
    );
    let resp = userinfo!(app, "fat_forged");
    assert_eq!(resp.status(), 401);
    assert!(
        resp.headers()
            .get("www-authenticate")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("invalid_token")
    );

    let code = full_authorization!(app, b, &rp, ivy, ["profile"]);
    let resp = token_request!(
        app,
        Some(basic(&rp)),
        &[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", rp.redirect.as_str()),
            ("code_verifier", VERIFIER)
        ]
    );
    let body: Value = test::read_body_json(resp).await;
    let access = body["access_token"].as_str().unwrap().to_string();
    let id_token = body["id_token"].as_str().unwrap().to_string();
    assert_eq!(
        userinfo!(app, id_token).status(),
        401,
        "ID Token 不是 access token"
    );
    // ID Token / access token 都不是 FloatCTF API 凭证（平台 API 只认 HS512 jwt_secret 签的 JWT）
    floatctf::core::security::jwt::configure_jwt_secret(floatctf::core::secret::Secret::new(
        "oidc-flow-test-platform-jwt-secret",
    ));
    for token in [&id_token, &access] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/users/me")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 401, "OIDC 令牌不得通过平台 API 守卫");
    }

    // POST + 表单 access_token
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/userinfo")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form(&[("access_token", access.as_str())]))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    // 同时用两种方式携带 → invalid_request
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/userinfo")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header(("Authorization", format!("Bearer {access}")))
            .set_payload(form(&[("access_token", access.as_str())]))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // 应用被禁用 → 立即失效；恢复后可用
    let model = oidc_clients::Entity::find_by_id(rp.model.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let model = clients::set_status(&db, model, OidcClientStatus::Disabled, None, None)
        .await
        .unwrap();
    assert_eq!(userinfo!(app, access).status(), 401);
    clients::set_status(&db, model, OidcClientStatus::Active, None, None)
        .await
        .unwrap();
    assert_eq!(userinfo!(app, access).status(), 200);

    // 用户撤销授权 → 立即失效
    assert!(
        floatctf::modules::identity::oidc::store::revoke_consent(&db, ivy.id, rp.model.id)
            .await
            .unwrap()
    );
    assert_eq!(userinfo!(app, access).status(), 401);
    // 撤销后再次授权需要重新同意
    let resp = get!(app, b, auth_url(&rp, &[("scope", "openid profile")]));
    assert!(location(&resp).starts_with("/oauth/consent"));
}

#[actix_web::test]
async fn logout_revokes_sso_session_server_side() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let jack = seed_user(&db, "jack").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    let mut b = Browser::default();
    let _ = full_authorization!(app, b, &rp, jack, ["profile"]);
    let stolen_cookie = b.clone();

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/logout")
            .insert_header(("Origin", ISSUER))
            .insert_header(("Cookie", b.header()))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 204);
    b.absorb(&resp);
    assert!(
        !b.cookies.keys().any(|k| k.contains("sso")),
        "Cookie 被清除"
    );
    // 即使旧 Cookie 被保留（不依赖客户端清理），服务端也已撤销
    let mut replay = stolen_cookie;
    let resp = get!(
        app,
        replay,
        auth_url(&rp, &[("prompt", "none"), ("scope", "openid profile")])
    );
    assert_eq!(query_of(&location(&resp))["error"], "login_required");
    // 跨站 Origin 不能注销
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/logout")
            .insert_header(("Origin", "https://evil.example"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn cleanup_removes_only_long_expired_rows() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let kim = seed_user(&db, "kim").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    let mut b = Browser::default();
    let live_code = full_authorization!(app, b, &rp, kim, ["profile"]);
    let stale_code = full_authorization!(app, b, &rp, kim, ["profile"]);
    let hash = floatctf::modules::identity::oidc::domain::secret::sha256_hex;
    let rows = oidc_authorization_codes::Entity::find()
        .all(&db)
        .await
        .unwrap();
    let stale = rows
        .iter()
        .find(|r| r.code_hash == hash(&stale_code))
        .unwrap()
        .clone();
    let mut active = stale.into_active_model();
    active.expires_at = Set((chrono::Utc::now() - chrono::Duration::days(2)).fixed_offset());
    active.update(&db).await.unwrap();

    let stats = floatctf::modules::identity::oidc::store::cleanup_expired(&db)
        .await
        .unwrap();
    assert!(stats.codes >= 1);
    let rows = oidc_authorization_codes::Entity::find()
        .all(&db)
        .await
        .unwrap();
    assert!(
        !rows.iter().any(|r| r.code_hash == hash(&stale_code)),
        "过期 2 天的授权码被删除"
    );
    assert!(
        rows.iter().any(|r| r.code_hash == hash(&live_code)),
        "有效授权码保留"
    );
}

#[actix_web::test]
async fn oidc_login_is_rate_limited_per_username() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let config = test_config();
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let victim = seed_user(&db, "ratelimit").await;
    let rp = seed_client(&db, &config, owner.id, OidcClientType::Confidential, None).await;
    // 用设置表把每用户名配额调到 3（同时覆盖"运维可调"路径），结束后恢复原值。
    let previous = floatctf::infrastructure::get_setting(&db, "OIDC_LOGIN_RATE_PER_USER")
        .await
        .ok();
    floatctf::infrastructure::settings::upsert_setting(&db, "OIDC_LOGIN_RATE_PER_USER", "3")
        .await
        .unwrap();
    let mut b = Browser::default();
    let resp = get!(app, b, auth_url(&rp, &[]));
    let login = location(&resp);
    let mut statuses = Vec::new();
    for _ in 0..4 {
        let html = body_string!(get!(app, b, login.clone()));
        let txn = hidden(&html, "txn");
        let csrf = hidden(&html, "csrf");
        let resp = post_form!(
            app,
            b,
            "/oauth/login",
            &[
                ("txn", txn.as_str()),
                ("csrf", csrf.as_str()),
                ("action", "login"),
                ("username", victim.username.as_str()),
                ("password", "wrong-password")
            ]
        );
        statuses.push(resp.status().as_u16());
    }
    // 配额 3：前 3 次正常校验（口令错误 → 200 + 错误提示），第 4 次被拒绝（429）。
    assert!(statuses[..3].iter().all(|s| *s == 200), "{statuses:?}");
    assert_eq!(statuses[3], 429, "{statuses:?}");
    let html = body_string!(get!(app, b, login.clone()));
    let resp = post_form!(
        app,
        b,
        "/oauth/login",
        &[
            ("txn", hidden(&html, "txn").as_str()),
            ("csrf", hidden(&html, "csrf").as_str()),
            ("action", "login"),
            ("username", victim.username.as_str()),
            ("password", PASSWORD)
        ]
    );
    assert_eq!(resp.status(), 429);
    assert!(
        !b.cookies.keys().any(|k| k.contains("sso")),
        "限流期间不得建立会话"
    );
    floatctf::infrastructure::settings::upsert_setting(
        &db,
        "OIDC_LOGIN_RATE_PER_USER",
        previous.as_deref().unwrap_or("10"),
    )
    .await
    .unwrap();
}
