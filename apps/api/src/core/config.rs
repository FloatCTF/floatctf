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
    pub redis: RedisConfig,
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
    /// Docker-compatible policy proxy exposed by floatctf-helper.
    pub socket_path: String,
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
    /// 网络 runtime 选择：`helper` = 通过固定 Unix socket 调用特权 floatctf-helper；
    /// `noop` 仅供 unit test / mock 使用（Noop 永远不允许 Verified）。
    pub network_runtime: String,
    pub flagserver_image: String,
    pub judgeserver_image: String,
    /// JudgeServer/FlagServer 访问 FloatCTF internal API 的端点（容器视角）。
    /// 当 `platform_internal_network` 为空时，它作为 `scheme://host:port` 模板：
    /// host 会按赛事替换为 infra 网关；配置 control network 时则使用固定 URL。
    pub platform_internal_url: String,
    /// 可选的 Docker control network。生产 API 容器化时，infra 容器额外加入该
    /// internal 网络并直接访问固定 `platform_internal_url`；开发原生 API 保持 None。
    pub platform_internal_network: Option<String>,
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
    /// 比赛赛事子网分配池（CIDR）：每 AWDP Event 从池中分配一个 `event_netmask` 大小的子网。
    pub network_pool: String,
    /// 每赛事子网掩码长度（如 24 → 每赛事 /24；默认 24）。
    pub event_netmask: i32,
    /// JudgeServer data plane 主机名（玩家 contract；data 网络内 DNS alias，默认 judge-server）。
    pub practice_judge_data_host: String,
    /// JudgeServer 访问 FloatCTF internal API 的基址（容器视角，control/data 网络可达；
    /// 宿主部署时用 data 网络网关直连宿主 API，host firewall 限制 GameBox 访问；
    /// 必须显式配置——不再默认 host.docker.internal（§36/§37）。
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
pub struct RedisConfig {
    /// Mandatory Redis endpoint used by realtime fan-out, distributed rate limiting,
    /// terminal tickets, scheduler wakeups and settings cache.
    pub url: Secret,
}

#[derive(Debug, Clone)]
pub struct RealtimeConfig {
    pub channel: String,
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
    #[serde(default)]
    docker: DockerToml,
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
    redis: RedisToml,
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
struct DockerToml {
    #[serde(default = "default_docker_socket_path")]
    socket_path: String,
}

impl Default for DockerToml {
    fn default() -> Self {
        Self {
            socket_path: default_docker_socket_path(),
        }
    }
}

fn default_docker_socket_path() -> String {
    helper_protocol::DEFAULT_DOCKER_SOCKET_PATH.to_string()
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
    /// 默认 `noop` 只用于未显式配置的测试场景；开发/生产配置显式写 `helper`。
    #[serde(default = "default_network_runtime")]
    network_runtime: String,
    #[serde(default = "default_flagserver_image")]
    flagserver_image: String,
    #[serde(default = "default_judgeserver_image")]
    judgeserver_image: String,
    #[serde(default = "default_platform_internal_url")]
    platform_internal_url: String,
    #[serde(default)]
    platform_internal_network: Option<String>,
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
            platform_internal_url: default_platform_internal_url(),
            platform_internal_network: None,
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
    #[serde(default = "default_network_pool")]
    network_pool: String,
    #[serde(default = "default_event_netmask")]
    event_netmask: i32,
    #[serde(default = "default_practice_judge_data_host")]
    practice_judge_data_host: String,
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
            network_pool: default_network_pool(),
            event_netmask: default_event_netmask(),
            practice_judge_data_host: default_practice_judge_data_host(),
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

fn default_practice_judge_data_host() -> String {
    "judge-server".to_string()
}

fn default_practice_judgeserver_image() -> String {
    "floatctf/infra/awdp-judgeserver:latest".to_string()
}

fn default_practice_network_subnet() -> String {
    "10.42.2.0/23".to_string()
}

fn default_practice_judge_ip() -> String {
    "10.42.2.2".to_string()
}

fn default_network_pool() -> String {
    // 10.43.0.0/16：与练习固定网络（10.42.2.0/24）、control（10.42.8.0/24）完全错开，
    // 避免 Docker 报 "Pool overlaps with other one on this address space"。
    "10.43.0.0/16".to_string()
}

fn default_event_netmask() -> i32 {
    24
}

fn default_platform_internal_url() -> String {
    String::new()
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

#[derive(Debug, Deserialize)]
struct RedisToml {
    url: String,
}

#[derive(Debug, Deserialize)]
struct RealtimeToml {
    #[serde(default = "default_realtime_channel")]
    channel: String,
}

impl Default for RealtimeToml {
    fn default() -> Self {
        Self {
            channel: default_realtime_channel(),
        }
    }
}

fn default_realtime_channel() -> String {
    "floatctf:realtime".to_string()
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
        let redis_url = required_value("redis.url", file.redis.url)?;
        if !matches!(file.awd.network_runtime.as_str(), "helper" | "noop") {
            anyhow::bail!(
                "awd.network_runtime must be 'helper' (normal runtime) or 'noop' (tests only)"
            );
        }

        Ok(Self {
            server: ServerConfig {
                listen_ip: file.server.listen_ip,
                listen_port: file.server.listen_port,
                work_dir: file.server.work_dir.clone(),
            },
            database: DatabaseConfig {
                url: Secret::new(database_url),
            },
            docker: DockerConfig {
                socket_path: file.docker.socket_path,
            },
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
                flagserver_image: warn_if_latest("awd.flagserver_image", file.awd.flagserver_image),
                judgeserver_image: warn_if_latest(
                    "awd.judgeserver_image",
                    file.awd.judgeserver_image,
                ),
                platform_internal_url: file.awd.platform_internal_url,
                platform_internal_network: non_empty(file.awd.platform_internal_network),
            },
            awdp: AwdpStaticConfig {
                practice_judgeserver_image: warn_if_latest(
                    "awdp.practice_judgeserver_image",
                    file.awdp.practice_judgeserver_image,
                ),
                practice_network_subnet: file.awdp.practice_network_subnet,
                practice_judge_ip: file.awdp.practice_judge_ip,
                network_pool: file.awdp.network_pool,
                event_netmask: file.awdp.event_netmask,
                practice_judge_data_host: file.awdp.practice_judge_data_host,
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
            redis: RedisConfig {
                url: Secret::new(redis_url),
            },
            realtime: RealtimeConfig {
                channel: required_value("realtime.channel", file.realtime.channel)?,
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
            redis_url = "Secret(***)",
            realtime_channel = %self.realtime.channel,
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

/// 判别镜像引用是否为浮动 tag（`:latest` 或无 tag）。
/// 生产部署必须钉版（install.sh 模板恒写 `${VERSION}`）；TOML 缺键时 serde 会
/// 静默回退到 `:latest` 默认值——这里发出显式警告，避免"手写配置漏键 → 意外
/// 拉到 latest"的无声漂移。仅警告不阻断：开发环境（development.toml）本就用 latest。
fn warn_if_latest(field: &str, image: String) -> String {
    let floating =
        image.ends_with(":latest") || !image.rsplit('/').next().unwrap_or("").contains(':');
    if floating {
        tracing::warn!(
            field,
            image = %image,
            "镜像配置未钉版（:latest/无 tag）：生产环境存在不可复现风险，请在 TOML 中显式指定版本 tag"
        );
    }
    image
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
    fn warn_if_latest_flags_floating_tags() {
        // 浮动 tag 与缺 tag 均视为未钉版（返回原值不变，仅告警副作用）
        assert_eq!(
            warn_if_latest("t", "floatctf/awd-flagserver:latest".to_string()),
            "floatctf/awd-flagserver:latest"
        );
        assert_eq!(
            warn_if_latest("t", "floatctf/awd-flagserver".to_string()),
            "floatctf/awd-flagserver"
        );
        // 钉版 tag 原样返回
        assert_eq!(
            warn_if_latest("t", "floatctf/awd-flagserver:0.3.3".to_string()),
            "floatctf/awd-flagserver:0.3.3"
        );
        // registry 带端口的镜像名不被误判（路径分段含 ':' 也算钉版）
        assert_eq!(
            warn_if_latest("t", "registry.local:5000/floatctf/judge:1.0".to_string()),
            "registry.local:5000/floatctf/judge:1.0"
        );
    }

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
                [redis]
                url = "redis://localhost:6379/"
                [awd]
                network_runtime = "helper"
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
        assert_eq!(config.redis.url.expose(), "redis://localhost:6379/");
        assert_eq!(config.realtime.channel, "floatctf:realtime");
        assert_eq!(
            config.docker.socket_path,
            helper_protocol::DEFAULT_DOCKER_SOCKET_PATH
        );
        assert_eq!(config.awd.network_runtime, "helper");
    }

    #[test]
    fn redis_url_is_required() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
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

        let error = AppConfig::from_file(&path).unwrap_err().to_string();
        assert!(error.contains("redis"));
    }

    #[test]
    fn secret_in_database_config_redacts() {
        let c = DatabaseConfig {
            url: Secret::new("postgres://user:pass@localhost/db"),
        };
        assert!(!format!("{c:?}").contains("pass@"));
    }
}
