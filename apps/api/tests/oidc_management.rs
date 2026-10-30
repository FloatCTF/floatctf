//! G3：浏览器 SSO 衔接 + 开发者中心 + 已授权应用 + 管理员审核（进程内 HTTP + 真实 PostgreSQL）。
//!
//! - 平台登录 `POST /api/users/session` 响应体不变，并下发 SSO Cookie → 随后授权第三方无需再输密码；
//! - 平台 JWT 桥接 `POST /api/users/sso`（幂等；旧 JWT 无 iat 时 auth_time 取签发时间）；
//! - `DELETE /api/users/sso` 服务端撤销；
//! - 开发者只能管理自己的应用；secret 只返回一次、轮换宽限期/立即作废；所有者暂停/恢复；
//! - 管理员审批/拒绝/封禁/恢复，所有者不能解除封禁；
//! - 用户列出/撤销授权，撤销后 UserInfo 立即失效；
//! - `[oidc]` 关闭时平台登录不写 Cookie、OIDC 管理接口 404。

mod oidc_support;

use actix_web::{App, test, web};
use oidc_support::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use serde_json::{Value, json};
use uuid::Uuid;

use floatctf::core::secret::Secret;
use floatctf::core::security::jwt;
use floatctf::entity::sea_orm_active_enums::AccountStatus;
use floatctf::entity::{user_platform_roles, users};
use floatctf::infrastructure::LogService;

const PASSWORD: &str = "OidcMgmt@2026";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const JWT_SECRET: &str = "oidc-management-test-jwt-secret";

fn configure_jwt() {
    jwt::configure_jwt_secret(Secret::new(JWT_SECRET));
}

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
        username: Set(format!("mgmt-{tag}-{short}")),
        nickname: Set(format!("Mgmt {tag} {short}")),
        password: Set(hash),
        email: Set(format!("mgmt-{tag}-{short}@example.com")),
        created_at: Set(now),
        updated_at: Set(now),
        avatar: Set(None),
        status: Set(AccountStatus::Active),
        token_version: Set(0),
    }
    .insert(db)
    .await
    .unwrap()
}

async fn seed_admin(db: &DatabaseConnection) -> users::Model {
    let user = seed_user(db, "admin").await;
    user_platform_roles::ActiveModel {
        user_id: Set(user.id),
        role_id: Set(floatctf::core::system_ids::PLATFORM_ROLE_SUPER_ADMIN),
        granted_by: Set(None),
        granted_at: Set(chrono::Utc::now().fixed_offset()),
    }
    .insert(db)
    .await
    .unwrap();
    user
}

fn user_jwt(user: &users::Model) -> String {
    jwt::gen_session_token(user.id, user.token_version).unwrap()
}

fn admin_jwt(admin: &users::Model) -> String {
    jwt::gen_session_token(admin.id, admin.token_version).unwrap()
}

macro_rules! app {
    ($db:expr, $oidc:expr) => {{
        let db = web::Data::new($db.clone());
        let mut app = App::new()
            .app_data(app_state($db.clone()))
            .app_data(db.clone())
            .app_data(web::Data::new(LogService::new(db.clone())))
            .app_data(web::Data::new(lazy_docker()))
            .app_data(web::Data::new(aws_sdk_s3::Client::from_conf(
                aws_sdk_s3::Config::builder()
                    .behavior_version(aws_config::BehaviorVersion::latest())
                    .region(aws_sdk_s3::config::Region::new("local"))
                    .endpoint_url("http://127.0.0.1:1")
                    .build(),
            )));
        let oidc: Option<floatctf::core::config::OidcConfig> = $oidc;
        if let Some(config) = oidc {
            app = app.app_data(web::Data::new(provider(config, test_keys(false))));
        }
        test::init_service(app.configure(floatctf::bootstrap::configure_all_routes)).await
    }};
}

macro_rules! api {
    ($app:expr, $method:ident, $uri:expr, $token:expr, $body:expr) => {{
        let mut req = test::TestRequest::$method().uri(&$uri);
        if let Some(token) = $token {
            req = req.insert_header(("Authorization", format!("Bearer {token}")));
        }
        let body: Option<Value> = $body;
        if let Some(body) = body {
            req = req.set_json(body);
        }
        let resp = test::call_service(&$app, req.to_request()).await;
        let status = resp.status().as_u16();
        let value: Value =
            serde_json::from_slice(&test::read_body(resp).await).unwrap_or(Value::Null);
        (status, value)
    }};
}

macro_rules! browser_get {
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

macro_rules! consent_allow {
    ($app:expr, $browser:expr, $loc:expr) => {{
        let resp = browser_get!($app, $browser, $loc);
        assert_eq!(resp.status(), 200);
        let html = String::from_utf8(test::read_body(resp).await.to_vec()).unwrap();
        let txn = hidden(&html, "txn");
        let csrf = hidden(&html, "csrf");
        let req = test::TestRequest::post()
            .uri("/oauth/consent")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header(("Origin", ISSUER))
            .insert_header(("Cookie", $browser.header()))
            .set_payload(form(&[
                ("txn", txn.as_str()),
                ("csrf", csrf.as_str()),
                ("action", "allow"),
                ("scope", "profile"),
            ]));
        let resp = test::call_service(&$app, req.to_request()).await;
        assert_eq!(resp.status(), 302);
        location(&resp)
    }};
}

fn authorize_url(client_id: &str, redirect: &str) -> String {
    format!(
        "/oauth/authorize?{}",
        form(&[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", redirect),
            ("scope", "openid profile"),
            ("state", "s1"),
            ("nonce", "n1"),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
        ])
    )
}

fn client_body(redirect: &str) -> Value {
    json!({
        "client_type": "confidential",
        "name": "Mgmt RP",
        "homepage_url": "https://mgmt.example.com",
        "redirect_uris": [redirect],
        "scopes": ["openid", "profile", "email"],
    })
}

// ── SSO：平台登录 / 桥接 / 登出 ──────────────────────────────────────────────

#[actix_web::test]
async fn platform_login_sets_sso_cookie_and_third_party_needs_no_password() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    configure_jwt();
    let app = app!(db, Some(test_config()));
    let owner = seed_user(&db, "owner").await;
    let user = seed_user(&db, "player").await;
    let admin = seed_admin(&db).await;

    // 开发者创建 + 管理员审批
    let redirect = format!(
        "https://rp-{}.example.com/cb",
        &Uuid::new_v4().simple().to_string()[..6]
    );
    let (status, created) = api!(
        app,
        post,
        "/api/oidc/clients",
        Some(user_jwt(&owner)),
        Some(client_body(&redirect))
    );
    assert_eq!(status, 200, "{created}");
    let client_id = created["data"]["client"]["client_id"]
        .as_str()
        .unwrap()
        .to_string();
    let secret = created["data"]["client_secret"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(created["data"]["client"]["status"], "pending");
    let (status, _) = api!(
        app,
        post,
        format!("/api/admin/oidc/clients/{client_id}/approve"),
        Some(admin_jwt(&admin)),
        Some(json!({"reason": "ok"}))
    );
    assert_eq!(status, 200);

    // 1) 平台登录：响应体与原来一致（code=0, data=JWT），额外下发 HttpOnly SSO Cookie
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/users/session")
            .set_json(json!({"username": user.username, "password": PASSWORD}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let set_cookie: Vec<String> = resp
        .headers()
        .get_all("set-cookie")
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    let sso = set_cookie
        .iter()
        .find(|c| c.contains("fctf_sso="))
        .expect("SSO cookie");
    assert!(
        sso.contains("HttpOnly") && sso.contains("SameSite=Lax") && sso.contains("Path=/"),
        "{sso}"
    );
    assert!(
        sso.contains("Secure") && sso.starts_with("__Host-"),
        "https issuer → __Host- + Secure: {sso}"
    );
    let mut browser = Browser::default();
    browser.absorb(&resp);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 0);
    let platform_jwt = body["data"].as_str().unwrap().to_string();
    assert!(
        jwt::validate_jwt(platform_jwt.clone()).is_ok(),
        "仍是平台 HS512 JWT"
    );

    // 2) 第三方授权：直接到同意页（不出现登录页），同意后回调
    let resp = browser_get!(app, browser, authorize_url(&client_id, &redirect));
    let loc = location(&resp);
    assert!(
        loc.starts_with("/oauth/consent"),
        "已登录 FloatCTF 不应再要求密码: {loc}"
    );
    let cb = consent_allow!(app, browser, loc);
    assert!(cb.starts_with(&redirect));
    let code = query_of(&cb)["code"].clone();
    // 3) RP 兑换得到的 sub 就是平台用户
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header((
                "Authorization",
                format!(
                    "Basic {}",
                    base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        format!("{client_id}:{secret}")
                    )
                ),
            ))
            .set_payload(form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", redirect.as_str()),
                ("code_verifier", VERIFIER),
            ]))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let tokens: Value = test::read_body_json(resp).await;
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/oauth/userinfo")
            .insert_header((
                "Authorization",
                format!("Bearer {}", tokens["access_token"].as_str().unwrap()),
            ))
            .to_request(),
    )
    .await;
    let info: Value = test::read_body_json(resp).await;
    assert_eq!(info["sub"], user.id.to_string());

    // 4) 已授权应用列表可见；撤销后 access token 立即失效
    let (status, list) = api!(
        app,
        get,
        "/api/oidc/authorizations",
        Some(platform_jwt.clone()),
        None
    );
    assert_eq!(status, 200);
    let entry = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["client_id"] == client_id)
        .expect("授权记录");
    assert_eq!(entry["scopes"], json!(["openid", "profile"]));
    let (status, _) = api!(
        app,
        delete,
        format!("/api/oidc/authorizations/{client_id}"),
        Some(platform_jwt.clone()),
        None
    );
    assert_eq!(status, 200);
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/oauth/userinfo")
            .insert_header((
                "Authorization",
                format!("Bearer {}", tokens["access_token"].as_str().unwrap()),
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);
    let (status, _) = api!(
        app,
        delete,
        format!("/api/oidc/authorizations/{client_id}"),
        Some(platform_jwt.clone()),
        None
    );
    assert_eq!(status, 404, "重复撤销");

    // 5) SPA 登出：DELETE /api/users/sso 服务端撤销，之后 prompt=none → login_required
    let req = test::TestRequest::delete()
        .uri("/api/users/sso")
        .insert_header(("Origin", ISSUER))
        .insert_header(("Cookie", browser.header()))
        .to_request();
    let stale = browser.clone();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    browser.absorb(&resp);
    assert!(browser.cookies.is_empty() || !browser.cookies.keys().any(|k| k.contains("sso")));
    let mut stale = stale;
    let resp = browser_get!(
        app,
        stale,
        format!("{}&prompt=none", authorize_url(&client_id, &redirect))
    );
    assert_eq!(query_of(&location(&resp))["error"], "login_required");
}

#[actix_web::test]
async fn jwt_bridge_is_idempotent_and_uses_real_issue_time() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    configure_jwt();
    let app = app!(db, Some(test_config()));
    let user = seed_user(&db, "bridge").await;
    let token = user_jwt(&user);

    // 匿名不可桥接
    let (status, _) = api!(app, post, "/api/users/sso", None::<String>, None);
    assert_eq!(status, 401);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/users/sso")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let mut browser = Browser::default();
    browser.absorb(&resp);
    assert!(browser.cookies.keys().any(|k| k.ends_with("fctf_sso")));
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["established"], true);

    // 同一浏览器、同一用户再次桥接 → 幂等，不新建会话
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/users/sso")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .insert_header(("Cookie", browser.header()))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["established"], false);

    // 旧 JWT（无 iat）：auth_time = exp - 8h，而不是"现在"
    // v2 令牌（有 purpose/token_version、无 iat）也必须可用，auth_time 同样按 exp - 8h 推算。
    #[derive(serde::Serialize)]
    struct Legacy {
        sub: Uuid,
        purpose: &'static str,
        token_version: i32,
        exp: usize,
    }
    let exp = chrono::Utc::now().timestamp() + 3 * 3600; // 5 小时前签发的 8h 令牌
    let legacy = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS512),
        &Legacy {
            sub: user.id,
            purpose: "session",
            token_version: user.token_version,
            exp: exp as usize,
        },
        &jsonwebtoken::EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .unwrap();
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/users/sso")
            .insert_header(("Authorization", format!("Bearer {legacy}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let cookie = resp
        .response()
        .cookies()
        .find(|c| c.name().ends_with("fctf_sso"))
        .unwrap()
        .into_owned();
    let max_age = cookie.max_age().unwrap().whole_seconds();
    assert!(
        max_age <= 3 * 3600 && max_age > 3 * 3600 - 60,
        "会话不超过 JWT 剩余寿命: {max_age}"
    );
    let (session, _) =
        floatctf::modules::identity::oidc::store::find_valid_session(&db, cookie.value())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(session.auth_time.timestamp(), exp - 8 * 3600);
    assert_eq!(session.established_via, "jwt_bridge");

    // 管理员 JWT 不能桥接（超管不是 OIDC 终端用户）
    let admin = seed_admin(&db).await;
    let (status, _) = api!(app, post, "/api/users/sso", Some(admin_jwt(&admin)), None);
    assert_eq!(status, 401);
}

// ── 开发者中心：归属隔离、secret、状态机 ─────────────────────────────────────

#[actix_web::test]
async fn developer_center_enforces_owner_isolation_and_secret_lifecycle() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    configure_jwt();
    let app = app!(db, Some(test_config()));
    let alice = seed_user(&db, "alice").await;
    let mallory = seed_user(&db, "mallory").await;
    let admin = seed_admin(&db).await;
    let (a, m) = (user_jwt(&alice), user_jwt(&mallory));

    // 校验：非 https 回调、缺 openid、未知 scope、公开客户端指定 secret 方式
    for bad in [
        json!({"client_type": "confidential", "name": "x", "redirect_uris": ["http://evil.example/cb"], "scopes": ["openid"]}),
        json!({"client_type": "confidential", "name": "x", "redirect_uris": ["https://a.example/cb"], "scopes": ["profile"]}),
        json!({"client_type": "confidential", "name": "x", "redirect_uris": ["https://a.example/cb"], "scopes": ["openid", "admin"]}),
        json!({"client_type": "public", "token_endpoint_auth_method": "client_secret_basic", "name": "x", "redirect_uris": ["https://a.example/cb"], "scopes": ["openid"]}),
        json!({"client_type": "confidential", "name": "x", "redirect_uris": ["https://*.example/cb"], "scopes": ["openid"]}),
    ] {
        let (status, body) = api!(
            app,
            post,
            "/api/oidc/clients",
            Some(a.clone()),
            Some(bad.clone())
        );
        assert_eq!(status, 400, "{bad} → {body}");
    }

    let (status, created) = api!(
        app,
        post,
        "/api/oidc/clients",
        Some(a.clone()),
        Some(client_body("https://alice-rp.example.com/cb"))
    );
    assert_eq!(status, 200);
    let client_id = created["data"]["client"]["client_id"]
        .as_str()
        .unwrap()
        .to_string();
    let first_secret = created["data"]["client_secret"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(first_secret.starts_with("fcs_"));

    // secret 只出现一次：详情/列表都只有 has_secret
    let (_, detail) = api!(
        app,
        get,
        format!("/api/oidc/clients/{client_id}"),
        Some(a.clone()),
        None
    );
    let text = detail.to_string();
    assert!(
        !text.contains(&first_secret) && !text.contains("secret_hash") && !text.contains("$argon2")
    );
    assert_eq!(detail["data"]["has_secret"], true);
    let (_, list) = api!(app, get, "/api/oidc/clients", Some(a.clone()), None);
    assert!(!list.to_string().contains(&first_secret));

    // 归属隔离：mallory 对 alice 的应用一律 404，且看不到
    for (method, uri, body) in [
        ("get", format!("/api/oidc/clients/{client_id}"), None),
        (
            "patch",
            format!("/api/oidc/clients/{client_id}"),
            Some(
                json!({"name": "pwned", "redirect_uris": ["https://evil.example/cb"], "scopes": ["openid"]}),
            ),
        ),
        (
            "post",
            format!("/api/oidc/clients/{client_id}/secret"),
            Some(json!({})),
        ),
        (
            "post",
            format!("/api/oidc/clients/{client_id}/disable"),
            None,
        ),
        ("delete", format!("/api/oidc/clients/{client_id}"), None),
    ] {
        let (status, _) = match method {
            "get" => api!(app, get, uri, Some(m.clone()), body),
            "patch" => api!(app, patch, uri, Some(m.clone()), body),
            "post" => api!(app, post, uri, Some(m.clone()), body),
            _ => api!(app, delete, uri, Some(m.clone()), body),
        };
        assert_eq!(status, 404, "{method} {uri}");
    }
    let (_, list) = api!(app, get, "/api/oidc/clients", Some(m.clone()), None);
    assert!(!list.to_string().contains(&client_id));
    // 普通玩家不能调用管理员审核
    let (status, _) = api!(
        app,
        post,
        format!("/api/admin/oidc/clients/{client_id}/approve"),
        Some(a.clone()),
        Some(json!({}))
    );
    assert_eq!(status, 401);

    // 待审批：所有者不能自行启用，授权端点不接受
    let (status, _) = api!(
        app,
        post,
        format!("/api/oidc/clients/{client_id}/enable"),
        Some(a.clone()),
        None
    );
    assert_eq!(status, 400);
    let mut b = Browser::default();
    let resp = browser_get!(
        app,
        b,
        authorize_url(&client_id, "https://alice-rp.example.com/cb")
    );
    assert_eq!(resp.status(), 400);

    // 管理员审批 → 所有者修改（回调精确保存）→ 暂停/恢复
    let (status, approved) = api!(
        app,
        post,
        format!("/api/admin/oidc/clients/{client_id}/approve"),
        Some(admin_jwt(&admin)),
        None
    );
    assert_eq!(status, 200);
    assert_eq!(approved["data"]["status"], "active");
    let (status, patched) = api!(
        app,
        patch,
        format!("/api/oidc/clients/{client_id}"),
        Some(a.clone()),
        Some(json!({
            "name": "Alice RP v2", "redirect_uris": ["https://alice-rp.example.com/cb", "https://alice-rp.example.com/cb2"], "scopes": ["openid", "profile"]
        }))
    );
    assert_eq!(status, 200);
    assert_eq!(
        patched["data"]["redirect_uris"],
        json!([
            "https://alice-rp.example.com/cb",
            "https://alice-rp.example.com/cb2"
        ])
    );
    assert_eq!(
        patched["data"]["allowed_scopes"],
        json!(["openid", "profile"])
    );
    let (status, disabled) = api!(
        app,
        post,
        format!("/api/oidc/clients/{client_id}/disable"),
        Some(a.clone()),
        None
    );
    assert_eq!(status, 200);
    assert_eq!(disabled["data"]["status"], "disabled");
    let resp = browser_get!(
        app,
        b,
        authorize_url(&client_id, "https://alice-rp.example.com/cb")
    );
    assert_eq!(resp.status(), 400, "暂停的应用不能发起授权");
    let (status, _) = api!(
        app,
        post,
        format!("/api/oidc/clients/{client_id}/enable"),
        Some(a.clone()),
        None
    );
    assert_eq!(status, 200);

    // secret 轮换：默认宽限期内新旧都可用；revoke_previous 立即作废旧 secret
    let model = floatctf::modules::identity::oidc::clients::find_by_client_id(&db, &client_id)
        .await
        .unwrap()
        .unwrap();
    assert!(floatctf::modules::identity::oidc::clients::verify_secret(
        &model,
        &first_secret
    ));
    let (status, rotated) = api!(
        app,
        post,
        format!("/api/oidc/clients/{client_id}/secret"),
        Some(a.clone()),
        Some(json!({}))
    );
    assert_eq!(status, 200);
    let second_secret = rotated["data"]["client_secret"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(second_secret, first_secret);
    assert!(rotated["data"]["client"]["previous_secret_expires_at"].is_string());
    let model = floatctf::modules::identity::oidc::clients::find_by_client_id(&db, &client_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        floatctf::modules::identity::oidc::clients::verify_secret(&model, &first_secret),
        "宽限期内旧 secret 仍可用"
    );
    assert!(floatctf::modules::identity::oidc::clients::verify_secret(
        &model,
        &second_secret
    ));
    let (_, rotated) = api!(
        app,
        post,
        format!("/api/oidc/clients/{client_id}/secret"),
        Some(a.clone()),
        Some(json!({"revoke_previous": true}))
    );
    let third_secret = rotated["data"]["client_secret"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(rotated["data"]["client"]["previous_secret_expires_at"].is_null());
    let model = floatctf::modules::identity::oidc::clients::find_by_client_id(&db, &client_id)
        .await
        .unwrap()
        .unwrap();
    assert!(!floatctf::modules::identity::oidc::clients::verify_secret(
        &model,
        &first_secret
    ));
    assert!(
        !floatctf::modules::identity::oidc::clients::verify_secret(&model, &second_secret),
        "立即作废"
    );
    assert!(floatctf::modules::identity::oidc::clients::verify_secret(
        &model,
        &third_secret
    ));

    // 管理员封禁：所有者无法恢复；管理员恢复后可用
    let (status, _) = api!(
        app,
        post,
        format!("/api/admin/oidc/clients/{client_id}/suspend"),
        Some(admin_jwt(&admin)),
        Some(json!({"reason": "phishing report"}))
    );
    assert_eq!(status, 200);
    let (status, _) = api!(
        app,
        post,
        format!("/api/oidc/clients/{client_id}/enable"),
        Some(a.clone()),
        None
    );
    assert_eq!(status, 400);
    let (_, list) = api!(
        app,
        get,
        "/api/admin/oidc/clients?status=suspended",
        Some(admin_jwt(&admin)),
        None
    );
    let row = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["client_id"] == client_id)
        .unwrap();
    assert_eq!(row["owner_username"], alice.username);
    assert_eq!(row["status_reason"], "phishing report");
    let (status, _) = api!(
        app,
        post,
        format!("/api/admin/oidc/clients/{client_id}/reinstate"),
        Some(admin_jwt(&admin)),
        None
    );
    assert_eq!(status, 200);

    // 管理员 PKCE 豁免：只允许机密客户端
    let (status, body) = api!(
        app,
        patch,
        format!("/api/admin/oidc/clients/{client_id}/pkce"),
        Some(admin_jwt(&admin)),
        Some(json!({"require_pkce": false}))
    );
    assert_eq!(status, 200);
    assert_eq!(body["data"]["require_pkce"], false);
    let (_, public) = api!(
        app,
        post,
        "/api/oidc/clients",
        Some(a.clone()),
        Some(json!({
            "client_type": "public", "name": "SPA", "redirect_uris": ["https://spa.example.com/cb"], "scopes": ["openid"]
        }))
    );
    assert!(public["data"]["client_secret"].is_null());
    assert_eq!(
        public["data"]["client"]["token_endpoint_auth_method"],
        "none"
    );
    let public_id = public["data"]["client"]["client_id"].as_str().unwrap();
    let (status, _) = api!(
        app,
        patch,
        format!("/api/admin/oidc/clients/{public_id}/pkce"),
        Some(admin_jwt(&admin)),
        Some(json!({"require_pkce": false}))
    );
    assert_eq!(status, 400);
    let (status, _) = api!(
        app,
        post,
        format!("/api/oidc/clients/{public_id}/secret"),
        Some(a.clone()),
        Some(json!({}))
    );
    assert_eq!(status, 400, "公开客户端没有 secret");

    // 删除：只有所有者
    let (status, _) = api!(
        app,
        delete,
        format!("/api/oidc/clients/{client_id}"),
        Some(a.clone()),
        None
    );
    assert_eq!(status, 200);
    let (status, _) = api!(
        app,
        get,
        format!("/api/oidc/clients/{client_id}"),
        Some(a.clone()),
        None
    );
    assert_eq!(status, 404);
}

#[actix_web::test]
async fn oidc_disabled_keeps_platform_login_unchanged() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    configure_jwt();
    let app = app!(db, None);
    let user = seed_user(&db, "legacy").await;
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/users/session")
            .set_json(json!({"username": user.username, "password": PASSWORD}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers().get("set-cookie").is_none(),
        "OIDC 关闭时不写任何 Cookie"
    );
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 0);
    assert_eq!(body["message"], "OK");
    let token = body["data"].as_str().unwrap().to_string();
    let (status, me) = api!(app, get, "/api/users/me", Some(token.clone()), None);
    assert_eq!(status, 200);
    assert_eq!(me["data"]["username"], user.username);
    // 错误口令仍是 401 + 平台信封
    let (status, err) = api!(
        app,
        post,
        "/api/users/session",
        None::<String>,
        Some(json!({"username": user.username, "password": "wrong"}))
    );
    assert_eq!(status, 401);
    assert_eq!(err["code"], 401);
    // OIDC 管理接口 404，协议端点 404
    let (status, _) = api!(app, get, "/api/oidc/clients", Some(token), None);
    assert_eq!(status, 404);
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/oauth/authorize?client_id=x")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}
