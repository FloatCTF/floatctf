//! 标准退出（P4）：RP-Initiated Logout 1.0 + Back-Channel Logout 1.0，进程内 HTTP + 真实 PostgreSQL +
//! 本机真实 TCP 接收端（模拟 LearnOS 的后端退出接口）。
//!
//! 覆盖 A04（普通退出不通知 RP）、A12（OP 侧 end_session 参数校验、Logout Token 形态与签名）、
//! A13（只撤销被选中的 sid；同浏览器其他账号、其他设备不受影响）、A14（接收端不可达 → pending + 重试 + 如实报告）、
//! A20（未放行的内网地址被拒绝，接收端从未被访问）。

mod oidc_support;

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

use actix_web::{App, test, web};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use oidc_support::*;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    sea_query::Expr,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use floatctf::core::config::OidcConfig;
use floatctf::entity::sea_orm_active_enums::{OidcClientStatus, OidcClientType};
use floatctf::entity::{
    oidc_backchannel_deliveries, oidc_browser_sessions, oidc_clients, oidc_session_clients, users,
};
use floatctf::infrastructure::LogService;
use floatctf::modules::identity::oidc::clients::{self, ClientInput};

const PASSWORD: &str = "Logout@2026";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const EVENT: &str = "http://schemas.openid.net/event/backchannel-logout";

// ── 本机 RP 接收端 ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct Receiver {
    port: u16,
    bodies: Arc<Mutex<Vec<String>>>,
    status: Arc<AtomicU16>,
}

impl Receiver {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let status = Arc::new(AtomicU16::new(200));
        let (b, st) = (bodies.clone(), status.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let (b, st) = (b.clone(), st.clone());
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    loop {
                        let n = sock.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                        let text = String::from_utf8_lossy(&buf).to_string();
                        if let Some(idx) = text.find("\r\n\r\n") {
                            let len = text[..idx]
                                .lines()
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                                })
                                .unwrap_or(0);
                            if buf.len() >= idx + 4 + len {
                                b.lock()
                                    .unwrap()
                                    .push(text[idx + 4..idx + 4 + len].to_string());
                                break;
                            }
                        }
                    }
                    let code = st.load(Ordering::SeqCst);
                    let reply = format!(
                        "HTTP/1.1 {code} X\r\nContent-Length: 0\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
                    );
                    let _ = sock.write_all(reply.as_bytes()).await;
                });
            }
        });
        Self {
            port,
            bodies,
            status,
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/api/oidc/backchannel-logout", self.port)
    }

    fn tokens(&self) -> Vec<String> {
        self.bodies
            .lock()
            .unwrap()
            .iter()
            .filter_map(|b| {
                url::form_urlencoded::parse(b.as_bytes())
                    .find(|(k, _)| k == "logout_token")
                    .map(|(_, v)| v.into_owned())
            })
            .collect()
    }
}

fn logout_config(trusted: &[u16]) -> OidcConfig {
    OidcConfig {
        backchannel_logout: true,
        backchannel_trusted_hosts: trusted.iter().map(|p| format!("127.0.0.1:{p}")).collect(),
        ..unified_config()
    }
}

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
        username: Set(format!("lo-{tag}-{short}")),
        nickname: Set(format!("Lo {tag} {short}")),
        password: Set(hash),
        email: Set(format!("lo-{tag}-{short}@example.com")),
        created_at: Set(now),
        updated_at: Set(now),
        avatar: Set(None),
    }
    .insert(db)
    .await
    .unwrap()
}

struct TestClient {
    model: oidc_clients::Model,
    secret: String,
    redirect: String,
    post_logout: String,
}

async fn seed_client(
    db: &DatabaseConnection,
    config: &OidcConfig,
    owner: Uuid,
    backchannel: Option<String>,
) -> TestClient {
    let tag = &Uuid::new_v4().simple().to_string()[..8];
    let redirect = format!("https://rp-{tag}.example.com/callback");
    let post_logout = format!("https://rp-{tag}.example.com/auth/signed-out");
    let (model, secret) = clients::create(
        db,
        config,
        owner,
        OidcClientType::Confidential,
        None,
        &ClientInput {
            name: format!("Logout RP {tag}"),
            homepage_url: Some(format!("https://rp-{tag}.example.com")),
            redirect_uris: vec![redirect.clone()],
            scopes: vec!["openid".into(), "profile".into()],
            post_logout_redirect_uris: Some(vec![post_logout.clone()]),
            backchannel_logout_uri: backchannel,
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
        post_logout,
    }
}

macro_rules! app {
    ($db:expr, $config:expr) => {{
        let provider = web::Data::new(provider($config, test_keys(false)));
        let db = web::Data::new($db.clone());
        test::init_service(
            App::new()
                .app_data(app_state($db.clone()))
                .app_data(provider.clone())
                .app_data(db.clone())
                .app_data(web::Data::new(LogService::new(db)))
                .configure(floatctf::bootstrap::configure_all_routes),
        )
        .await
    }};
}

macro_rules! send {
    ($app:expr, $browser:expr, $req:expr) => {{
        let mut req = $req;
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

/// 当前浏览器的主站状态。
macro_rules! web_status {
    ($app:expr, $browser:expr) => {{
        let resp = send!(
            $app,
            $browser,
            test::TestRequest::get().uri("/api/users/web-session")
        );
        let body: Value = test::read_body_json(resp).await;
        body["data"].clone()
    }};
}

/// 主站登录，返回登录后的 CSRF。
macro_rules! web_login {
    ($app:expr, $browser:expr, $user:expr) => {{
        let st = web_status!($app, $browser);
        let csrf = st["csrf_token"].as_str().unwrap_or("anonymous").to_string();
        let resp = send!(
            $app,
            $browser,
            test::TestRequest::post()
                .uri("/api/users/web-session/login")
                .insert_header(("Origin", ISSUER))
                .insert_header(("X-CSRF-Token", csrf))
                .set_json(json!({"username": $user.username, "password": PASSWORD}))
        );
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["csrf_token"].as_str().unwrap().to_string()
    }};
}

fn origin_of(url: &str) -> String {
    url::Url::parse(url).unwrap().origin().ascii_serialization()
}

/// 已登录浏览器走一遍 RP 授权（不带 prompt）：必要时同意，返回 ID Token。
macro_rules! rp_id_token {
    ($app:expr, $browser:expr, $rp:expr) => {{
        let uri = format!(
            "/oauth/authorize?{}",
            form(&[
                ("response_type", "code"),
                ("client_id", $rp.model.client_id.as_str()),
                ("redirect_uri", $rp.redirect.as_str()),
                ("scope", "openid profile"),
                ("state", "st"),
                ("nonce", "n"),
                ("code_challenge", CHALLENGE),
                ("code_challenge_method", "S256"),
            ])
        );
        let resp = send!($app, $browser, test::TestRequest::get().uri(&uri));
        let mut loc = location(&resp);
        if loc.starts_with("/oauth/consent") {
            let resp = send!($app, $browser, test::TestRequest::get().uri(&loc));
            let html = body_string!(resp);
            let resp = send!(
                $app,
                $browser,
                test::TestRequest::post()
                    .uri("/oauth/consent")
                    .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
                    .insert_header(("Origin", ISSUER))
                    .set_payload(form(&[
                        ("txn", hidden(&html, "txn").as_str()),
                        ("csrf", hidden(&html, "csrf").as_str()),
                        ("action", "allow"),
                        ("scope", "profile")
                    ]))
            );
            loc = location(&resp);
        }
        assert!(loc.starts_with(&$rp.redirect), "{loc}");
        let code = query_of(&loc)["code"].clone();
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
                            .encode(format!("{}:{}", $rp.model.client_id, $rp.secret))
                    ),
                ))
                .set_payload(form(&[
                    ("grant_type", "authorization_code"),
                    ("code", code.as_str()),
                    ("redirect_uri", $rp.redirect.as_str()),
                    ("code_verifier", VERIFIER),
                ]))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        body["id_token"].as_str().unwrap().to_string()
    }};
}

async fn jwks(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
) -> JwkSet {
    test::read_body_json(
        test::call_service(
            app,
            test::TestRequest::get()
                .uri("/.well-known/jwks.json")
                .to_request(),
        )
        .await,
    )
    .await
}

fn verify(token: &str, jwks: &JwkSet, aud: &str) -> (jsonwebtoken::Header, Value) {
    let header = jsonwebtoken::decode_header(token).unwrap();
    let key = DecodingKey::from_jwk(jwks.find(header.kid.as_deref().unwrap()).unwrap()).unwrap();
    let mut v = Validation::new(Algorithm::RS256);
    v.set_audience(&[aud]);
    v.set_issuer(&[ISSUER]);
    v.set_required_spec_claims(&["iss", "aud", "exp", "iat"]);
    (
        header,
        jsonwebtoken::decode::<Value>(token, &key, &v)
            .unwrap()
            .claims,
    )
}

async fn session_of(
    db: &DatabaseConnection,
    user: &users::Model,
) -> Vec<oidc_browser_sessions::Model> {
    oidc_browser_sessions::Entity::find()
        .filter(oidc_browser_sessions::Column::UserId.eq(user.id))
        .all(db)
        .await
        .unwrap()
}

async fn deliveries(
    db: &DatabaseConnection,
    rp: &TestClient,
) -> Vec<oidc_backchannel_deliveries::Model> {
    oidc_backchannel_deliveries::Entity::find()
        .filter(oidc_backchannel_deliveries::Column::OidcClientId.eq(rp.model.id))
        .all(db)
        .await
        .unwrap()
}

macro_rules! web_logout {
    ($app:expr, $browser:expr, $csrf:expr, $scope:expr) => {{
        let resp = send!(
            $app,
            $browser,
            test::TestRequest::post()
                .uri("/api/users/web-session/logout")
                .insert_header(("Origin", ISSUER))
                .insert_header(("X-CSRF-Token", $csrf.clone()))
                .set_json(json!({"scope": $scope}))
        );
        let status = resp.status().as_u16();
        let body: Value = test::read_body_json(resp).await;
        (status, body)
    }};
}

// ── Discovery / sid / 绑定记录 ──────────────────────────────────────────────

#[actix_web::test]
async fn id_tokens_carry_sid_and_bindings_are_recorded() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let rx = Receiver::start().await;
    let config = logout_config(&[rx.port]);
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id, Some(rx.url())).await;

    let doc: Value = test::read_body_json(
        test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/.well-known/openid-configuration")
                .to_request(),
        )
        .await,
    )
    .await;
    assert_eq!(
        doc["end_session_endpoint"],
        format!("{ISSUER}/oauth/end-session")
    );
    assert_eq!(doc["backchannel_logout_supported"], true);

    let mut b = Browser::default();
    web_login!(app, b, alice);
    let id_token = rp_id_token!(app, b, rp);
    let (_, claims) = verify(&id_token, &jwks(&app).await, &rp.model.client_id);
    let session = session_of(&db, &alice).await.pop().unwrap();
    assert_eq!(
        claims["sid"], session.sid,
        "ID Token sid = 身份会话的对外标识"
    );
    assert_ne!(claims["sid"], session.id.to_string(), "sid 不暴露会话主键");
    let binding = oidc_session_clients::Entity::find()
        .filter(oidc_session_clients::Column::SessionId.eq(session.id))
        .filter(oidc_session_clients::Column::OidcClientId.eq(rp.model.id))
        .one(&db)
        .await
        .unwrap();
    assert!(binding.is_some());
}

// ── A04：普通退出不通知 RP ──────────────────────────────────────────────────

#[actix_web::test]
async fn ordinary_logout_never_notifies_relying_parties() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let rx = Receiver::start().await;
    let config = logout_config(&[rx.port]);
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id, Some(rx.url())).await;
    let mut b = Browser::default();
    let csrf = web_login!(app, b, alice);
    rp_id_token!(app, b, rp);

    let (code, body) = web_logout!(app, b, csrf, "account");
    assert_eq!(code, 200, "{body}");
    assert!(body["data"].get("logout_report").is_none());
    assert!(
        rx.tokens().is_empty(),
        "普通退出不得向 RP 发送 Logout Token"
    );
    assert!(deliveries(&db, &rp).await.is_empty());
    assert!(
        session_of(&db, &alice)
            .await
            .iter()
            .all(|s| s.revoked_reason.as_deref() == Some("logout"))
    );
}

// ── A12 / A13：全局退出只撤销被选中的 sid，Logout Token 合规 ─────────────────

#[actix_web::test]
async fn global_logout_revokes_only_the_selected_session_and_sends_a_valid_logout_token() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let rx = Receiver::start().await;
    let config = logout_config(&[rx.port]);
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let bob = seed_user(&db, "bob").await;
    let rp = seed_client(&db, &config, owner.id, Some(rx.url())).await;
    let no_bc = seed_client(&db, &config, owner.id, None).await;

    // 浏览器 1：alice 登录并授权两个 RP（单账号时静默），随后再登录 bob，再切回 alice
    let mut laptop = Browser::default();
    web_login!(app, laptop, alice);
    rp_id_token!(app, laptop, rp);
    rp_id_token!(app, laptop, no_bc);
    web_login!(app, laptop, bob);
    let st = web_status!(app, laptop);
    let alice_account = st["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["user_id"] == alice.id.to_string())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let resp = send!(
        app,
        laptop,
        test::TestRequest::post()
            .uri("/api/users/web-session/select")
            .insert_header(("Origin", ISSUER))
            .insert_header(("X-CSRF-Token", st["csrf_token"].as_str().unwrap()))
            .set_json(json!({"account_id": alice_account}))
    );
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let csrf = body["data"]["csrf_token"].as_str().unwrap().to_string();
    // 设备 2：alice 也登录着
    let mut device2 = Browser::default();
    web_login!(app, device2, alice);
    assert_eq!(session_of(&db, &alice).await.len(), 2);

    let (code, body) = web_logout!(app, laptop, csrf, "global");
    assert_eq!(code, 200, "{body}");
    let report = &body["data"]["logout_report"];
    assert_eq!(report["deliveries"][0]["status"], "delivered", "{report}");
    assert_eq!(
        report["without_backchannel"],
        json!([no_bc.model.name]),
        "未接入的应用如实列出"
    );
    assert_eq!(body["data"]["authenticated"], false);

    // 接收端收到合规 Logout Token
    let tokens = rx.tokens();
    assert_eq!(tokens.len(), 1);
    let (header, claims) = verify(&tokens[0], &jwks(&app).await, &rp.model.client_id);
    assert_eq!(header.typ.as_deref(), Some("logout+jwt"));
    let revoked: Vec<_> = session_of(&db, &alice)
        .await
        .into_iter()
        .filter(|s| s.revoked_reason.as_deref() == Some("global_logout"))
        .collect();
    assert_eq!(revoked.len(), 1, "只撤销本浏览器这一个 alice 会话");
    assert_eq!(claims["sid"], revoked[0].sid);
    assert_eq!(claims["sub"], alice.id.to_string());
    assert!(claims["events"][EVENT].is_object());
    assert!(claims.get("nonce").is_none());
    assert!(claims["jti"].as_str().unwrap().len() >= 16);
    assert!(claims["exp"].as_i64().unwrap() > claims["iat"].as_i64().unwrap());

    // 另一设备的 alice、同一浏览器里的 bob 都不受影响；本浏览器仍保留 bob 的登录
    let alive = session_of(&db, &alice)
        .await
        .into_iter()
        .filter(|s| s.revoked_at.is_none())
        .count();
    assert_eq!(alive, 1);
    assert!(
        session_of(&db, &bob)
            .await
            .iter()
            .all(|s| s.revoked_at.is_none())
    );
    let st = web_status!(app, laptop);
    assert_eq!(st["accounts"].as_array().unwrap().len(), 1);
    assert_eq!(st["accounts"][0]["user_id"], bob.id.to_string());
    let rows = deliveries(&db, &rp).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "delivered");
    assert_eq!(rows[0].reason, "main_site");
    assert!(deliveries(&db, &no_bc).await.is_empty());
}

// ── A14：接收端不可达 → pending、如实报告、之后重试成功 ─────────────────────

#[actix_web::test]
async fn unreachable_receiver_is_reported_and_retried() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    // 先占一个端口再释放：投递时连接被拒。
    let dead_port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let config = logout_config(&[dead_port]);
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let target = format!("http://127.0.0.1:{dead_port}/api/oidc/backchannel-logout");
    let rp = seed_client(&db, &config, owner.id, Some(target)).await;
    let mut b = Browser::default();
    let csrf = web_login!(app, b, alice);
    rp_id_token!(app, b, rp);

    let (code, body) = web_logout!(app, b, csrf, "global");
    assert_eq!(code, 200, "{body}");
    let d = &body["data"]["logout_report"]["deliveries"][0];
    assert_eq!(
        d["status"], "pending",
        "未送达必须如实报告，不能显示成功：{d}"
    );
    let row = deliveries(&db, &rp).await.pop().unwrap();
    assert_eq!(row.status, "pending");
    assert_eq!(row.attempts, 1);
    assert!(row.last_error.is_some());
    assert!(
        row.next_attempt_at > chrono::Utc::now().fixed_offset(),
        "退避后再试"
    );

    // 接收端恢复：同一端口起服务，令该行到期，worker 一轮即投递成功
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", dead_port))
        .await
        .unwrap();
    let got = Arc::new(Mutex::new(0usize));
    let g = got.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 8192];
            let _ = sock.read(&mut buf).await;
            *g.lock().unwrap() += 1;
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
        }
    });
    oidc_backchannel_deliveries::Entity::update_many()
        .col_expr(
            oidc_backchannel_deliveries::Column::NextAttemptAt,
            Expr::value(chrono::Utc::now().fixed_offset() - chrono::Duration::seconds(1)),
        )
        .filter(oidc_backchannel_deliveries::Column::Id.eq(row.id))
        .exec(&db)
        .await
        .unwrap();
    let oidc = provider(config, test_keys(false));
    let n = floatctf::modules::identity::oidc::backchannel::retry_due(&db, &oidc)
        .await
        .unwrap();
    assert!(n >= 1);
    let row = oidc_backchannel_deliveries::Entity::find_by_id(row.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "delivered", "{:?}", row.last_error);
    assert_eq!(row.attempts, 2);
    assert_eq!(*got.lock().unwrap(), 1);
}

// ── RP 返回 400 → failed；A20：未放行的内网地址拒绝投递 ─────────────────────

#[actix_web::test]
async fn rejected_and_internal_targets_fail_honestly() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let rejecting = Receiver::start().await;
    rejecting.status.store(400, Ordering::SeqCst);
    let internal = Receiver::start().await; // 不在白名单
    let config = logout_config(&[rejecting.port]);
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp_400 = seed_client(&db, &config, owner.id, Some(rejecting.url())).await;
    let rp_int = seed_client(&db, &config, owner.id, Some(internal.url())).await;
    let mut b = Browser::default();
    let csrf = web_login!(app, b, alice);
    rp_id_token!(app, b, rp_400);
    rp_id_token!(app, b, rp_int);

    let (code, body) = web_logout!(app, b, csrf, "global");
    assert_eq!(code, 200, "{body}");
    let report = body["data"]["logout_report"]["deliveries"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(report.len(), 2);
    assert!(report.iter().all(|d| d["status"] == "failed"), "{report:?}");
    assert_eq!(rejecting.tokens().len(), 1, "被拒绝的 RP 确实收到了请求");
    assert!(internal.tokens().is_empty(), "未放行的内网地址从未被访问");
    let row = deliveries(&db, &rp_int).await.pop().unwrap();
    assert_eq!(row.status, "failed");
    assert!(row.last_error.unwrap().contains("非公网"));
    assert_eq!(
        deliveries(&db, &rp_400)
            .await
            .pop()
            .unwrap()
            .last_http_status,
        Some(400)
    );
}

// ── A12：RP-Initiated Logout 参数校验、确认页、一次性事务 ─────────────────────

#[actix_web::test]
async fn end_session_validates_hint_redirect_and_requires_confirmation() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let rx = Receiver::start().await;
    let config = logout_config(&[rx.port]);
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let mallory = seed_user(&db, "mallory").await;
    let rp = seed_client(&db, &config, owner.id, Some(rx.url())).await;
    let other_rp = seed_client(&db, &config, owner.id, None).await;

    let mut b = Browser::default();
    web_login!(app, b, alice);
    let hint = rp_id_token!(app, b, rp);
    let mut mb = Browser::default();
    web_login!(app, mb, mallory);
    let mallory_hint = rp_id_token!(app, mb, rp);

    let end = |pairs: &[(&str, &str)]| format!("/oauth/end-session?{}", form(pairs));

    // 未登记的回跳地址：错误页，绝不重定向
    let resp = send!(
        app,
        b,
        test::TestRequest::get().uri(&end(&[
            ("id_token_hint", hint.as_str()),
            ("post_logout_redirect_uri", "https://evil.example/steal"),
        ]))
    );
    assert_eq!(resp.status(), 400);
    assert!(resp.headers().get("location").is_none());
    // client_id 与 hint.aud 不一致
    let resp = send!(
        app,
        b,
        test::TestRequest::get().uri(&end(&[
            ("id_token_hint", hint.as_str()),
            ("client_id", other_rp.model.client_id.as_str()),
        ]))
    );
    assert_eq!(resp.status(), 400);
    // 伪造 hint
    let resp = send!(
        app,
        b,
        test::TestRequest::get().uri(&end(&[("id_token_hint", "eyJ.fake.token")]))
    );
    assert_eq!(resp.status(), 400);
    // 别人的 hint：本浏览器没有对应会话 → 无事可做，不撤销任何会话
    let resp = send!(
        app,
        b,
        test::TestRequest::get().uri(&end(&[("id_token_hint", mallory_hint.as_str())]))
    );
    assert_eq!(resp.status(), 200);
    assert!(body_string!(resp).contains("无需退出"));
    assert!(
        session_of(&db, &mallory)
            .await
            .iter()
            .all(|s| s.revoked_at.is_none())
    );

    // 合法请求：GET 只渲染确认页，不撤销
    let resp = send!(
        app,
        b,
        test::TestRequest::get().uri(&end(&[
            ("id_token_hint", hint.as_str()),
            ("post_logout_redirect_uri", rp.post_logout.as_str()),
            ("state", "lo-state"),
        ]))
    );
    assert_eq!(resp.status(), 200);
    let csp = resp
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(csp.contains(&origin_of(&rp.post_logout)), "{csp}");
    let html = body_string!(resp);
    assert!(html.contains("退出 FloatCTF 及关联应用"));
    assert!(html.contains(&alice.username));
    assert!(html.contains(&rp.model.name), "列出将通知的应用");
    assert!(
        session_of(&db, &alice)
            .await
            .iter()
            .all(|s| s.revoked_at.is_none()),
        "GET 不改变状态"
    );
    let request = hidden(&html, "request");
    let csrf = hidden(&html, "csrf");

    let confirm = |request: &str, csrf: &str, action: &str| {
        test::TestRequest::post()
            .uri("/oauth/end-session/confirm")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header(("Origin", ISSUER))
            .set_payload(form(&[
                ("request", request),
                ("csrf", csrf),
                ("action", action),
            ]))
    };
    // 错误 CSRF / 换浏览器
    let resp = send!(app, b, confirm(&request, "forged", "logout"));
    assert_eq!(resp.status(), 400);
    let mut thief = Browser::default();
    let resp = send!(app, thief, confirm(&request, &csrf, "logout"));
    assert_eq!(resp.status(), 400);
    assert!(
        session_of(&db, &alice)
            .await
            .iter()
            .all(|s| s.revoked_at.is_none())
    );
    // 跨站提交
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/oauth/end-session/confirm")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header(("Origin", "https://evil.example"))
            .insert_header(("Cookie", b.header()))
            .set_payload(form(&[
                ("request", request.as_str()),
                ("csrf", csrf.as_str()),
                ("action", "logout"),
            ]))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // 确认：撤销 alice 在本浏览器的会话、通知 RP、回跳并带回 state
    let resp = send!(app, b, confirm(&request, &csrf, "logout"));
    assert_eq!(resp.status(), 302);
    let loc = location(&resp);
    assert!(loc.starts_with(&rp.post_logout));
    assert_eq!(query_of(&loc)["state"], "lo-state");
    assert!(
        session_of(&db, &alice)
            .await
            .iter()
            .all(|s| s.revoked_reason.as_deref() == Some("global_logout"))
    );
    assert_eq!(rx.tokens().len(), 1);
    assert_eq!(
        deliveries(&db, &rp).await.pop().unwrap().reason,
        "end_session"
    );
    assert!(
        session_of(&db, &mallory)
            .await
            .iter()
            .all(|s| s.revoked_at.is_none()),
        "其他用户不受影响"
    );

    // 重放确认：事务只能完成一次
    let resp = send!(app, b, confirm(&request, &csrf, "logout"));
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn end_session_cancel_keeps_the_login_and_endpoint_is_absent_when_disabled() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let rx = Receiver::start().await;
    let config = logout_config(&[rx.port]);
    let app = app!(db, config.clone());
    let owner = seed_user(&db, "owner").await;
    let alice = seed_user(&db, "alice").await;
    let rp = seed_client(&db, &config, owner.id, Some(rx.url())).await;
    let mut b = Browser::default();
    web_login!(app, b, alice);
    let hint = rp_id_token!(app, b, rp);
    let resp = send!(
        app,
        b,
        test::TestRequest::get().uri(&format!(
            "/oauth/end-session?{}",
            form(&[
                ("id_token_hint", hint.as_str()),
                ("post_logout_redirect_uri", rp.post_logout.as_str()),
                ("state", "cancel-state"),
            ])
        ))
    );
    let html = body_string!(resp);
    let resp = send!(
        app,
        b,
        test::TestRequest::post()
            .uri("/oauth/end-session/confirm")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .insert_header(("Origin", ISSUER))
            .set_payload(form(&[
                ("request", hidden(&html, "request").as_str()),
                ("csrf", hidden(&html, "csrf").as_str()),
                ("action", "cancel"),
            ]))
    );
    assert_eq!(resp.status(), 200);
    let html = body_string!(resp);
    assert!(html.contains("已取消退出"));
    assert!(html.contains("返回"), "仍提供回到应用的链接");
    assert!(
        !html.contains("state="),
        "取消后回到应用不带 state，RP 不会误报已退出 FloatCTF"
    );
    assert!(
        session_of(&db, &alice)
            .await
            .iter()
            .all(|s| s.revoked_at.is_none())
    );
    assert!(rx.tokens().is_empty());

    // 开关关闭：端点不存在、Discovery 不宣告、主站 global 不可用
    let off = app!(db, unified_config());
    let resp = test::call_service(
        &off,
        test::TestRequest::get()
            .uri("/oauth/end-session")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}
