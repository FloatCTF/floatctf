//! OP 页面资源（P2 / A15）：官方 Logo 与样式脚本同源可用、CSP 不含内联权限；
//! 选择页头像只经 OP 自己的头像端点、只对本浏览器自己的账号可见，缺失/非图片/越权一律 404（页面回退首字母）。
//!
//! 头像用例需要 RustFS（S3 兼容）：`FLOATCTF_TEST_RUSTFS`（默认 `http://127.0.0.1:59000`，凭据 rustfsadmin/rustfsadmin）。
//! 数据库或 RustFS 不可达时 soft-skip。

mod oidc_support;

use actix_web::{App, test, web};
use aws_sdk_s3::primitives::ByteStream;
use oidc_support::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use uuid::Uuid;

use floatctf::entity::sea_orm_active_enums::{OidcClientStatus, OidcClientType};
use floatctf::entity::users;
use floatctf::infrastructure::LogService;
use floatctf::modules::identity::oidc::clients::{self, ClientInput};

const PASSWORD: &str = "Assets@2026";
const LOGO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../web/public/float.png"
));

fn rustfs_url() -> String {
    std::env::var("FLOATCTF_TEST_RUSTFS").unwrap_or_else(|_| "http://127.0.0.1:59000".into())
}

fn s3() -> aws_sdk_s3::Client {
    aws_sdk_s3::Client::from_conf(
        aws_sdk_s3::Config::builder()
            .behavior_version(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new("cn-east-1"))
            .endpoint_url(rustfs_url())
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                "rustfsadmin",
                "rustfsadmin",
                None,
                None,
                "oidc-pages-test",
            ))
            .force_path_style(true)
            .build(),
    )
}

async fn rustfs_or_skip() -> Option<aws_sdk_s3::Client> {
    let client = s3();
    match client.list_buckets().send().await {
        Ok(_) => {
            let _ = client
                .create_bucket()
                .bucket("floatctf-public")
                .send()
                .await;
            Some(client)
        }
        Err(e) => {
            eprintln!(
                "skip avatar cases: RustFS unreachable at {} ({e})",
                rustfs_url()
            );
            None
        }
    }
}

async fn seed_user(db: &DatabaseConnection, tag: &str, avatar: Option<String>) -> users::Model {
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
        username: Set(format!("asset-{tag}-{short}")),
        nickname: Set(format!("Asset {tag} {short}")),
        password: Set(hash),
        email: Set(format!("asset-{tag}-{short}@example.com")),
        created_at: Set(now),
        updated_at: Set(now),
        avatar: Set(avatar),
    }
    .insert(db)
    .await
    .unwrap()
}

macro_rules! app {
    ($db:expr, $s3:expr) => {{
        let provider = web::Data::new(provider(unified_config(), test_keys(false)));
        let db = web::Data::new($db.clone());
        test::init_service(
            App::new()
                .app_data(provider)
                .app_data(db.clone())
                .app_data(web::Data::new($s3))
                .app_data(web::Data::new(LogService::new(db)))
                .configure(floatctf::bootstrap::configure_all_routes),
        )
        .await
    }};
}

#[actix_web::test]
async fn official_logo_and_page_assets_are_served_from_the_op_origin() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let app = app!(db, s3());
    for (path, ty) in [
        ("/oauth/assets/float.png", "image/png"),
        ("/oauth/assets/ui.css", "text/css; charset=utf-8"),
        ("/oauth/assets/ui.js", "text/javascript; charset=utf-8"),
    ] {
        let resp = test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
        assert_eq!(resp.status(), 200, "{path}");
        assert_eq!(resp.headers().get("content-type").unwrap(), ty);
        assert_eq!(
            resp.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        if path.ends_with(".png") {
            let body = test::read_body(resp).await;
            assert_eq!(&body[..], LOGO, "与 apps/web/public/float.png 逐字节一致");
        }
    }
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/oauth/assets/../config.toml")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/oauth/assets/secret.txt")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

/// 主站登录并返回浏览器。
macro_rules! signed_in {
    ($app:expr, $user:expr) => {{
        let mut b = Browser::default();
        let req = test::TestRequest::post()
            .uri("/api/users/web-session/login")
            .insert_header(("Origin", ISSUER))
            .insert_header(("X-CSRF-Token", "anonymous"))
            .set_json(serde_json::json!({"username": $user.username, "password": PASSWORD}))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200);
        b.absorb(&resp);
        let body: serde_json::Value = test::read_body_json(resp).await;
        let account = body["data"]["accounts"][0]["id"].as_str().unwrap().to_string();
        (b, account)
    }};
}

macro_rules! avatar {
    ($app:expr, $browser:expr, $account:expr) => {{
        let mut req = test::TestRequest::get().uri(&format!("/oauth/accounts/{}/avatar", $account));
        if !$browser.cookies.is_empty() {
            req = req.insert_header(("Cookie", $browser.header()));
        }
        test::call_service(&$app, req.to_request()).await
    }};
}

#[actix_web::test]
async fn chooser_avatars_are_scoped_validated_and_fall_back_to_initials() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(store) = rustfs_or_skip().await else {
        return;
    };
    let tag = &Uuid::new_v4().simple().to_string()[..8];
    let good_key = format!("images/test_{tag}.png");
    let html_key = format!("images/test_{tag}_fake.png");
    store
        .put_object()
        .bucket("floatctf-public")
        .key(&good_key)
        .body(ByteStream::from_static(LOGO))
        .send()
        .await
        .unwrap();
    store
        .put_object()
        .bucket("floatctf-public")
        .key(&html_key)
        .body(ByteStream::from_static(
            b"<!doctype html><script>alert(1)</script>",
        ))
        .send()
        .await
        .unwrap();

    let app = app!(db, s3());
    let good = seed_user(&db, "good", Some(format!("/public/{good_key}"))).await;
    let missing = seed_user(
        &db,
        "missing",
        Some(format!("/public/images/missing_{tag}.png")),
    )
    .await;
    let fake = seed_user(&db, "fake", Some(format!("/public/{html_key}"))).await;
    let external = seed_user(&db, "external", Some("https://evil.example/a.png".into())).await;

    let (b_good, acc_good) = signed_in!(app, good);
    let resp = avatar!(app, b_good, acc_good);
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
    assert_eq!(
        resp.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert!(
        resp.headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    assert_eq!(&test::read_body(resp).await[..], LOGO);

    // 不在本浏览器 / 没有 Cookie：看不到（不能借此枚举全站头像）
    let (b_missing, acc_missing) = signed_in!(app, missing);
    assert_eq!(avatar!(app, b_missing, acc_good).status(), 404);
    assert_eq!(avatar!(app, Browser::default(), acc_good).status(), 404);
    // 对象不存在 → 404（页面显示首字母）
    assert_eq!(avatar!(app, b_missing, acc_missing).status(), 404);
    // 扩展名是 .png、内容是 HTML → 魔数校验拒绝
    let (b_fake, acc_fake) = signed_in!(app, fake);
    assert_eq!(avatar!(app, b_fake, acc_fake).status(), 404);
    // 外站 URL 头像：从不代理（防 SSRF）
    let (b_ext, acc_ext) = signed_in!(app, external);
    assert_eq!(avatar!(app, b_ext, acc_ext).status(), 404);
    assert_eq!(avatar!(app, b_ext, "not-a-uuid").status(), 404);
}

#[actix_web::test]
async fn chooser_page_uses_logo_avatar_endpoint_and_strict_csp() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let app = app!(db, s3());
    let owner = seed_user(&db, "owner", None).await;
    let with_avatar = seed_user(&db, "pic", Some("/public/images/a_b.png".into())).await;
    let config = unified_config();
    let (model, _) = clients::create(
        &db,
        &config,
        owner.id,
        OidcClientType::Confidential,
        None,
        &ClientInput {
            name: "Assets RP".into(),
            description: None,
            homepage_url: Some("https://learn.example.com".into()),
            logo_url: None,
            redirect_uris: vec!["https://rp-assets.example.com/cb".into()],
            scopes: vec!["openid".into(), "profile".into()],
            ..ClientInput::default()
        },
    )
    .await
    .unwrap();
    let model = clients::set_status(&db, model, OidcClientStatus::Active, None, None)
        .await
        .unwrap();
    let (mut b, account) = signed_in!(app, with_avatar);
    let uri = format!(
        "/oauth/authorize?{}",
        form(&[
            ("response_type", "code"),
            ("client_id", model.client_id.as_str()),
            ("redirect_uri", "https://rp-assets.example.com/cb"),
            ("scope", "openid profile"),
            ("prompt", "select_account"),
            (
                "code_challenge",
                "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
            ),
            ("code_challenge_method", "S256"),
        ])
    );
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Cookie", b.header()))
            .to_request(),
    )
    .await;
    b.absorb(&resp);
    let chooser = location(&resp);
    assert!(chooser.starts_with("/oauth/select-account"), "{chooser}");
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&chooser)
            .insert_header(("Cookie", b.header()))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let csp = resp
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(csp.contains("script-src 'self'") && csp.contains("style-src 'self'"));
    assert!(!csp.contains("unsafe-inline"));
    let html = String::from_utf8(test::read_body(resp).await.to_vec()).unwrap();
    assert!(html.contains("/oauth/assets/float.png"));
    assert!(html.contains(&format!(
        "&#x2f;oauth&#x2f;accounts&#x2f;{account}&#x2f;avatar"
    )));
    assert!(html.contains("data-fallback"));
    assert!(!html.contains("&#x2f;public&#x2f;"), "不直接引用公开桶路径");
}
