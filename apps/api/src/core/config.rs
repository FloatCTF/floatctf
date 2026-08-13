//! 进程启动时从 TOML 一次性加载的类型化静态配置。
//!
//! 由 `FLOATCTF_CONFIG` 选定的文件是进程静态 API 配置的**唯一**来源。
//! 可管理端动态编辑的项仍在数据库 `settings` 表（`seed_default_settings` / `get_setting`）。

use std::path::Path;

use serde::Deserialize;

use super::secret::Secret;

/// 应用顶层配置。
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub docker: DockerConfig,
    pub storage: StorageConfig,
    pub auth: AuthConfig,
    pub cors: CorsConfig,
    pub paths: PathConfig,
    pub awd: AwdStaticConfig,
    pub awdp: AwdpStaticConfig,
    /// Container image registry settings for GameBox package builds.
    pub registry: RegistryConfig,
    pub features: FeatureFlags,
    pub realtime: RealtimeConfig,
    pub logging: LoggingConfig,
    /// 主站地址前缀（[application] main_url），作为 MAIN_URL 设置的 seed 默认值
    pub main_url: String,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_ip: String,
    pub listen_port: u16,
    pub work_dir: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Connection URL — not Debug-printed in full (see `source_summary`).
    pub url: Secret,
}

#[derive(Debug, Clone)]
pub struct DockerConfig {
    /// Reserved for future host/socket override; currently uses bollard defaults.
    pub use_defaults: bool,
}

/// GameBox 包导入管线的镜像仓库 / 推送设置。
///
/// `push = false` 为显式 **LocalOnly** 模式：本地 build+inspect 后标记
/// 以 `image_id` 就绪；`image_repo_digest` 可为空；运行时钉扎 `image_id`。
/// 当 `push = true` 时必须推送，且仅在拿到 RepoDigest 后标记就绪。
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Image name prefix → `{image_prefix}/gameboxes/{safe}:{ver}`.
    pub image_prefix: String,
    /// When false: LocalOnly (no registry push). When true: must push + resolve digest.
    pub push: bool,
    pub username: Option<String>,
    pub password: Option<Secret>,
    pub server_address: Option<String>,
    /// Reserved/document; bollard may not honor yet.
    pub insecure: bool,
    pub build_timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub endpoint_url: String,
    pub access_key_id: String,
    pub secret_access_key: Secret,
    pub region: String,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub jwt_secret: Secret,
}

#[derive(Debug, Clone)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PathConfig {
    pub work_dir: String,
}

/// AWD 进程静态配置（非每场赛事密钥）。
#[derive(Debug, Clone)]
pub struct AwdStaticConfig {
    /// Whether AWD crypto could be derived from the shared JWT secret material.
    pub crypto_from_app_secret: bool,
    /// 网络 runtime 选择（P1-17）：`host` = HostNetworkRuntime + NftablesFirewallRuntime；
    /// `noop` = 仅 unit test / dev mock（Noop 永远不允许 Verified）。
    /// 取代旧 `host_network = true/false`；不新增 `firewall_backend` 开关。
    pub network_runtime: String,
    pub flagserver_image: String,
    pub judgeserver_image: String,
}

/// AWDP（含练习）进程静态配置。
#[derive(Debug, Clone)]
pub struct AwdpStaticConfig {
    /// 练习 JudgeServer 镜像（部署到练习 docker 子网）。
    pub practice_judgeserver_image: String,
    /// 练习专用 docker 子网 CIDR（全部练习实例 + JudgeServer 所在）。
    pub practice_network_subnet: String,
    /// 练习子网内 JudgeServer 固定 IP。
    pub practice_judge_ip: String,
    /// JudgeServer 回调平台使用的基址（容器视角；默认 host.docker.internal）。
    pub platform_internal_url: String,
    /// 评估 lease 时长（秒）：worker claim 后持有；到期未心跳可被回收重领。
    pub eval_lease_duration_secs: i64,
    /// 评估最大领取次数：超过则终态 PLATFORM_ERROR，不再重领。
    pub eval_max_attempts: i32,
}

#[derive(Debug, Clone)]
pub struct FeatureFlags {
    pub enable_unsafe_sql_admin: bool,
    pub enable_web_terminal: bool,
}

#[derive(Debug, Clone)]
pub struct RealtimeConfig {
    pub redis_url: Option<String>,
    pub redis_channel: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub filter: String,
    /// IANA timezone (e.g. "Asia/Shanghai"); empty = keep system local time.
    /// Applied to the process `TZ` env var before the logger initializes,
    /// so `ChronoLocal` log timestamps honor it.
    pub timezone: String,
}

#[derive(Debug, Deserialize)]
struct TomlConfig {
    #[serde(default)]
    application: ApplicationToml,
    #[serde(default)]
    server: ServerToml,
    database: DatabaseToml,
    rustfs: RustfsToml,
    #[serde(default)]
    auth: AuthToml,
    #[serde(default)]
    cors: CorsToml,
    #[serde(default)]
    features: FeaturesToml,
    #[serde(default)]
    awd: AwdToml,
    #[serde(default)]
    awdp: AwdpToml,
    #[serde(default)]
    registry: RegistryToml,
    #[serde(default)]
    realtime: RealtimeToml,
    #[serde(default)]
    logging: LoggingToml,
}

#[derive(Debug, Deserialize)]
struct ApplicationToml {
    #[serde(default = "default_main_url")]
    main_url: String,
}

impl Default for ApplicationToml {
    fn default() -> Self {
        Self {
            main_url: default_main_url(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ServerToml {
    #[serde(default = "default_listen_ip")]
    listen_ip: String,
    #[serde(default = "default_listen_port")]
    listen_port: u16,
    #[serde(default = "default_work_dir")]
    work_dir: String,
}

impl Default for ServerToml {
    fn default() -> Self {
        Self {
            listen_ip: default_listen_ip(),
            listen_port: default_listen_port(),
            work_dir: default_work_dir(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DatabaseToml {
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize)]
struct RustfsToml {
    #[serde(default)]
    endpoint_url: String,
    #[serde(default)]
    access_key_id: String,
    #[serde(default)]
    secret_access_key: String,
    #[serde(default)]
    region: String,
}

#[derive(Debug, Deserialize, Default)]
struct AuthToml {
    #[serde(default)]
    jwt_secret: String,
}

#[derive(Debug, Deserialize)]
struct CorsToml {
    #[serde(default = "default_cors_origins")]
    allowed_origins: Vec<String>,
}

impl Default for CorsToml {
    fn default() -> Self {
        Self {
            allowed_origins: default_cors_origins(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct FeaturesToml {
    #[serde(default)]
    unsafe_sql_admin: bool,
    #[serde(default)]
    web_terminal: bool,
}

#[derive(Debug, Deserialize)]
struct AwdToml {
    #[serde(default = "default_true")]
    crypto_from_app_secret: bool,
    /// 默认 `noop`（dev/CI 无特权环境）；生产配置显式写 `host`。
    #[serde(default = "default_network_runtime")]
    network_runtime: String,
    #[serde(default = "default_flagserver_image")]
    flagserver_image: String,
    #[serde(default = "default_judgeserver_image")]
    judgeserver_image: String,
}

fn default_network_runtime() -> String {
    "noop".to_string()
}

impl Default for AwdToml {
    fn default() -> Self {
        Self {
            crypto_from_app_secret: true,
            network_runtime: default_network_runtime(),
            flagserver_image: default_flagserver_image(),
            judgeserver_image: default_judgeserver_image(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AwdpToml {
    #[serde(default = "default_practice_judgeserver_image")]
    practice_judgeserver_image: String,
    #[serde(default = "default_practice_network_subnet")]
    practice_network_subnet: String,
    #[serde(default = "default_practice_judge_ip")]
    practice_judge_ip: String,
    #[serde(default = "default_platform_internal_url")]
    platform_internal_url: String,
    #[serde(default = "default_eval_lease_duration_secs")]
    eval_lease_duration_secs: i64,
    #[serde(default = "default_eval_max_attempts")]
    eval_max_attempts: i32,
}

impl Default for AwdpToml {
    fn default() -> Self {
        Self {
            practice_judgeserver_image: default_practice_judgeserver_image(),
            practice_network_subnet: default_practice_network_subnet(),
            practice_judge_ip: default_practice_judge_ip(),
            platform_internal_url: default_platform_internal_url(),
            eval_lease_duration_secs: default_eval_lease_duration_secs(),
            eval_max_attempts: default_eval_max_attempts(),
        }
    }
}

fn default_eval_lease_duration_secs() -> i64 {
    120
}

fn default_eval_max_attempts() -> i32 {
    3
}

fn default_practice_judgeserver_image() -> String {
    "floatctf/awdp-judgeserver:latest".to_string()
}

fn default_practice_network_subnet() -> String {
    "10.42.2.0/24".to_string()
}

fn default_practice_judge_ip() -> String {
    "10.42.2.2".to_string()
}

fn default_platform_internal_url() -> String {
    "http://host.docker.internal:9090".to_string()
}

#[derive(Debug, Deserialize)]
struct RegistryToml {
    #[serde(default = "default_image_prefix")]
    image_prefix: String,
    /// Default false = LocalOnly (dev-friendly).
    #[serde(default)]
    push: bool,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    server_address: Option<String>,
    #[serde(default)]
    insecure: bool,
    #[serde(default = "default_build_timeout_secs")]
    build_timeout_secs: u64,
}

impl Default for RegistryToml {
    fn default() -> Self {
        Self {
            image_prefix: default_image_prefix(),
            push: false,
            username: None,
            password: None,
            server_address: None,
            insecure: false,
            build_timeout_secs: default_build_timeout_secs(),
        }
    }
}

fn default_image_prefix() -> String {
    "floatctf".to_string()
}

fn default_build_timeout_secs() -> u64 {
    600
}

#[derive(Debug, Deserialize, Default)]
struct RealtimeToml {
    #[serde(default)]
    redis_url: Option<String>,
    #[serde(default)]
    redis_channel: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoggingToml {
    #[serde(default = "default_log_filter")]
    filter: String,
    #[serde(default = "default_timezone")]
    timezone: String,
}

impl Default for LoggingToml {
    fn default() -> Self {
        Self {
            filter: default_log_filter(),
            timezone: default_timezone(),
        }
    }
}

impl AppConfig {
    /// Load and validate configuration from a TOML file.
    pub fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read config {}: {e}", path.display()))?;
        let file: TomlConfig = toml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("failed to parse config {}: {e}", path.display()))?;

        let jwt_secret = required_value("auth.jwt_secret", file.auth.jwt_secret)?;
        if jwt_secret.len() < 16 {
            anyhow::bail!("auth.jwt_secret must be at least 16 characters");
        }
        let database_url = required_value("database.url", file.database.url)?;
        let endpoint_url = required_value("rustfs.endpoint_url", file.rustfs.endpoint_url)?;
        let access_key_id = required_value("rustfs.access_key_id", file.rustfs.access_key_id)?;
        let secret_access_key =
            required_value("rustfs.secret_access_key", file.rustfs.secret_access_key)?;
        let region = required_value("rustfs.region", file.rustfs.region)?;

        Ok(Self {
            server: ServerConfig {
                listen_ip: file.server.listen_ip,
                listen_port: file.server.listen_port,
                work_dir: file.server.work_dir.clone(),
            },
            database: DatabaseConfig {
                url: Secret::new(database_url),
            },
            docker: DockerConfig { use_defaults: true },
            storage: StorageConfig {
                endpoint_url,
                access_key_id,
                secret_access_key: Secret::new(secret_access_key),
                region,
            },
            auth: AuthConfig {
                jwt_secret: Secret::new(jwt_secret),
            },
            cors: CorsConfig {
                allowed_origins: file.cors.allowed_origins,
            },
            paths: PathConfig {
                work_dir: file.server.work_dir,
            },
            awd: AwdStaticConfig {
                crypto_from_app_secret: file.awd.crypto_from_app_secret,
                network_runtime: file.awd.network_runtime,
                flagserver_image: file.awd.flagserver_image,
                judgeserver_image: file.awd.judgeserver_image,
            },
            awdp: AwdpStaticConfig {
                practice_judgeserver_image: file.awdp.practice_judgeserver_image,
                practice_network_subnet: file.awdp.practice_network_subnet,
                practice_judge_ip: file.awdp.practice_judge_ip,
                platform_internal_url: file.awdp.platform_internal_url,
                eval_lease_duration_secs: file.awdp.eval_lease_duration_secs,
                eval_max_attempts: file.awdp.eval_max_attempts,
            },
            registry: RegistryConfig {
                image_prefix: file.registry.image_prefix,
                push: file.registry.push,
                username: non_empty(file.registry.username),
                password: non_empty(file.registry.password).map(Secret::new),
                server_address: non_empty(file.registry.server_address),
                insecure: file.registry.insecure,
                build_timeout_secs: file.registry.build_timeout_secs,
            },
            features: FeatureFlags {
                enable_unsafe_sql_admin: file.features.unsafe_sql_admin,
                enable_web_terminal: file.features.web_terminal,
            },
            realtime: RealtimeConfig {
                redis_url: non_empty(file.realtime.redis_url),
                redis_channel: non_empty(file.realtime.redis_channel),
            },
            logging: LoggingConfig {
                filter: file.logging.filter,
                timezone: file.logging.timezone,
            },
            main_url: file.application.main_url,
        })
    }

    /// Log non-secret configuration sources for operators (never logs secrets).
    pub fn log_source_summary(&self) {
        tracing::info!(
            listen = %format!("{}:{}", self.server.listen_ip, self.server.listen_port),
            work_dir = %self.server.work_dir,
            storage_endpoint = %self.storage.endpoint_url,
            storage_region = %self.storage.region,
            cors_origins = ?self.cors.allowed_origins,
            enable_unsafe_sql_admin = self.features.enable_unsafe_sql_admin,
            enable_web_terminal = self.features.enable_web_terminal,
            registry_image_prefix = %self.registry.image_prefix,
            registry_push = self.registry.push,
            registry_build_timeout_secs = self.registry.build_timeout_secs,
            database_url = "Secret(***)",
            jwt_secret = "Secret(***)",
            "AppConfig loaded from TOML"
        );
        if self.features.enable_unsafe_sql_admin {
            tracing::warn!("unsafe SQL admin is enabled — arbitrary SQL admin API is exposed");
        }
    }
}

fn required_value(name: &str, value: String) -> anyhow::Result<String> {
    if value.trim().is_empty() {
        anyhow::bail!("{name} is required in TOML config");
    }
    Ok(value)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn default_true() -> bool {
    true
}
fn default_listen_ip() -> String {
    "127.0.0.1".to_string()
}
fn default_listen_port() -> u16 {
    8080
}
fn default_work_dir() -> String {
    "./".to_string()
}
/// 时区为空 = 不修改进程本地时区。
fn default_timezone() -> String {
    String::new()
}
fn default_main_url() -> String {
    "http://localhost:8080".to_string()
}
fn default_log_filter() -> String {
    "actix_server=info,floatctf=info,fcmc=info".to_string()
}
fn default_flagserver_image() -> String {
    "floatctf/awd-flagserver:latest".to_string()
}
fn default_judgeserver_image() -> String {
    "floatctf/awd-judgeserver:latest".to_string()
}

fn default_cors_origins() -> Vec<String> {
    vec![
        "http://localhost:3000".to_string(),
        "http://127.0.0.1".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_config_loads_without_environment_variables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
                [server]
                listen_port = 9000
                [database]
                url = "postgres://localhost/db"
                [auth]
                jwt_secret = "a-development-secret"
                [rustfs]
                endpoint_url = "http://localhost:9000"
                access_key_id = "access"
                secret_access_key = "secret"
                region = "local"
            "#,
        )
        .unwrap();

        let config = AppConfig::from_file(&path).unwrap();
        assert_eq!(config.server.listen_port, 9000);
        assert_eq!(config.database.url.expose(), "postgres://localhost/db");
        assert_eq!(config.auth.jwt_secret.expose(), "a-development-secret");
    }

    #[test]
    fn secret_in_database_config_redacts() {
        let c = DatabaseConfig {
            url: Secret::new("postgres://user:pass@localhost/db"),
        };
        assert!(!format!("{c:?}").contains("pass@"));
    }
}
