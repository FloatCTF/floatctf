//! OIDC 集成测试支撑：真实 PostgreSQL（`DATABASE_URL`，不可达时 soft-skip，与其他 DB-gated 测试一致）
//! + 进程内 actix 服务（`configure_all_routes`，与生产同一路由聚合点）。
//!
//! 测试 key 是仓库内公开的测试专用 RSA key（`src/modules/identity/oidc/testdata/`）。

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use floatctf::core::config::{OidcConfig, OidcSigningKeyConfig};
use floatctf::modules::identity::oidc::{
    OidcProvider, RateGate,
    keys::{KeyMaterial, KeyRing},
};

pub const ISSUER: &str = "https://op.floatctf.test";

pub const TEST_KEY_A: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/modules/identity/oidc/testdata/test-key-a.pem"
));
pub const TEST_KEY_B: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/modules/identity/oidc/testdata/test-key-b.pem"
));

pub fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

pub async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => {
            // 迁移未应用（例如指向了旧库）时同样 skip，而不是误报失败。
            use sea_orm::ConnectionTrait;
            let ok = db
                .execute_unprepared("SELECT 1 FROM oidc_clients LIMIT 1")
                .await
                .is_ok();
            if ok {
                Some(db)
            } else {
                eprintln!("skip: oidc tables missing in {}", db_url());
                None
            }
        }
        Err(e) => {
            eprintln!("skip: DB unreachable ({e})");
            None
        }
    }
}

pub fn test_config() -> OidcConfig {
    OidcConfig {
        enabled: true,
        issuer: ISSUER.to_string(),
        signing_keys: vec![OidcSigningKeyConfig {
            kid: "test-a".into(),
            private_key_file: "unused".into(),
            active: true,
        }],
        session_ttl_secs: 8 * 3600,
        transaction_ttl_secs: 600,
        authorization_code_ttl_secs: 60,
        access_token_ttl_secs: 3600,
        id_token_ttl_secs: 3600,
        secret_rotation_grace_secs: 3600,
        allow_loopback_redirect_uris: true,
        require_client_approval: true,
        account_chooser: true,
        multi_account: false,
        max_browser_accounts: 5,
        browser_container_ttl_secs: 30 * 24 * 3600,
        unified_web_session: false,
        first_party_origins: Vec::new(),
        backchannel_logout: false,
        backchannel_trusted_hosts: Vec::new(),
        backchannel_extra_ca_file: None,
        backchannel_timeout_secs: 3,
        logout_token_ttl_secs: 120,
    }
}

/// 统一身份配置：浏览器账号容器 + 主站 Cookie 会话全部开启。
pub fn unified_config() -> OidcConfig {
    OidcConfig {
        multi_account: true,
        unified_web_session: true,
        ..test_config()
    }
}

pub fn test_keys(active_b: bool) -> KeyRing {
    KeyRing::from_materials(vec![
        KeyMaterial {
            kid: "test-a".into(),
            pem: TEST_KEY_A.as_bytes().to_vec(),
            active: !active_b,
        },
        KeyMaterial {
            kid: "test-b".into(),
            pem: TEST_KEY_B.as_bytes().to_vec(),
            active: active_b,
        },
    ])
    .expect("test key ring")
}

/// 内存限流（测试专用）：与 Redis 实现同一语义（固定窗口内计数）。
#[derive(Default)]
pub struct MemoryGate {
    hits: Mutex<HashMap<String, u32>>,
}

#[async_trait::async_trait]
impl RateGate for MemoryGate {
    async fn hit(
        &self,
        bucket: &str,
        key: &str,
        limit: u32,
        _window_secs: u64,
    ) -> Result<bool, String> {
        let mut hits = self.hits.lock().unwrap();
        let n = hits.entry(format!("{bucket}:{key}")).or_insert(0);
        if *n >= limit {
            return Ok(false);
        }
        *n += 1;
        Ok(true)
    }
}

pub fn provider(config: OidcConfig, keys: KeyRing) -> OidcProvider {
    OidcProvider::new(config, keys, Arc::new(MemoryGate::default()))
}

// ── 浏览器模拟辅助 ─────────────────────────────────────────────────────────

use actix_web::{body::MessageBody, dev::ServiceResponse};

pub fn enc(v: &str) -> String {
    url::form_urlencoded::byte_serialize(v.as_bytes()).collect()
}

pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn query_of(location: &str) -> HashMap<String, String> {
    let url = url::Url::parse(&format!("https://x{location}"))
        .or_else(|_| url::Url::parse(location))
        .unwrap();
    url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

pub fn location(resp: &ServiceResponse<impl MessageBody>) -> String {
    resp.headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default()
}

pub fn hidden(html: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    let start = html
        .find(&marker)
        .unwrap_or_else(|| panic!("no hidden {name} in page"))
        + marker.len();
    let end = html[start..].find('"').unwrap() + start;
    html[start..end].to_string()
}

/// 最小浏览器：只保存 Cookie。
#[derive(Default, Clone)]
pub struct Browser {
    pub cookies: HashMap<String, String>,
}

impl Browser {
    pub fn absorb(&mut self, resp: &ServiceResponse<impl MessageBody>) {
        for cookie in resp.response().cookies() {
            if cookie.max_age().is_some_and(|a| a.is_zero()) || cookie.value().is_empty() {
                self.cookies.remove(cookie.name());
            } else {
                self.cookies
                    .insert(cookie.name().to_string(), cookie.value().to_string());
            }
        }
    }

    pub fn header(&self) -> String {
        self.cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

// ── 完整平台 app_data（平台登录等使用 ReqCtx 的处理器需要）──────────────────

/// 惰性 Docker 客户端：bollard 只要求路径存在，测试中永远不会真正调用 Docker。
pub fn lazy_docker() -> bollard::Docker {
    let path = std::env::temp_dir().join("floatctf-oidc-tests-docker.sock");
    if !path.exists() {
        std::fs::write(&path, b"").unwrap();
    }
    bollard::Docker::connect_with_unix(path.to_str().unwrap(), 5, bollard::API_DEFAULT_VERSION)
        .unwrap()
}

/// 构造与 bootstrap 相同形状的 `AppState`（Docker/S3/Redis 客户端均为惰性连接，不会被访问）。
pub fn app_state(
    db: sea_orm::DatabaseConnection,
) -> actix_web::web::Data<floatctf::bootstrap::AppState> {
    use floatctf::infrastructure::LogService;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
        [database]
        url = "postgres://unused/db"
        [auth]
        jwt_secret = "oidc-management-test-jwt-secret"
        [redis]
        url = "redis://127.0.0.1:1/"
        [rustfs]
        endpoint_url = "http://127.0.0.1:1"
        access_key_id = "a"
        secret_access_key = "s"
        region = "local"
        "#,
    )
    .unwrap();
    let config = Arc::new(floatctf::core::AppConfig::from_file(&path).unwrap());
    let docker = lazy_docker();
    let s3 = aws_sdk_s3::Client::from_conf(
        aws_sdk_s3::Config::builder()
            .behavior_version(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new("local"))
            .endpoint_url("http://127.0.0.1:1")
            .build(),
    );
    let redis = redis::Client::open("redis://127.0.0.1:1/").unwrap();
    let web_db = actix_web::web::Data::new(db.clone());
    let log = LogService::new(web_db.clone());
    let scheduler = floatctf::scheduler::TaskScheduler::new(
        web_db,
        actix_web::web::Data::new(docker.clone()),
        actix_web::web::Data::new(s3.clone()),
        log.clone(),
    );
    actix_web::web::Data::new(floatctf::bootstrap::AppState::new(
        config,
        db,
        docker,
        s3,
        redis.clone(),
        log.clone(),
        floatctf::infrastructure::audit::AuditService::new(log),
        Arc::new(floatctf::infrastructure::realtime::RecordingEventPublisher::new()),
        Arc::new(scheduler),
        Arc::new(
            floatctf::modules::platform::operations::terminal::TerminalTicketStore::new(
                std::time::Duration::from_secs(60),
                redis,
            ),
        ),
    ))
}
