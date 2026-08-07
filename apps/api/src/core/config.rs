//! Typed static configuration loaded once from a TOML file at process start.
//!
//! The file selected by `FLOATCTF_CONFIG` is the only source for process-static
//! API configuration. Dynamic, admin-editable settings remain in the `settings`
//! DB table (`seed_default_settings` / `get_setting`).

use std::path::Path;

use serde::Deserialize;

use super::secret::Secret;

/// Top-level application configuration.
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
    pub features: FeatureFlags,
    pub realtime: RealtimeConfig,
    pub logging: LoggingConfig,
    pub challenge: ChallengeConfig,
    /// IANA timezone (e.g. "Asia/Shanghai"); empty = keep system local time.
    /// Applied to the process `TZ` env var before the logger initializes,
    /// so `ChronoLocal` log timestamps honor it.
    pub timezone: String,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_ip: String,
    pub listen_port: u16,
    pub work_dir: String,
    pub log_dir: String,
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
    pub log_dir: String,
    pub changelog_path: String,
}

/// Static AWD process config (not per-event secrets).
#[derive(Debug, Clone)]
pub struct AwdStaticConfig {
    /// Whether AWD crypto could be derived from the shared JWT secret material.
    pub crypto_from_app_secret: bool,
    pub host_network: bool,
    pub flagserver_image: String,
    pub judgeserver_image: String,
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
}

#[derive(Debug, Clone)]
pub struct ChallengeConfig {
    pub event_score_decay: String,
    pub event_score_min_percent: String,
    pub instance_max_per_user: String,
    pub instance_destroy_delay: String,
    pub http_prefix: String,
    pub node_ip: String,
    pub challenges_dir: String,
    pub main_url: String,
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
    paths: PathsToml,
    #[serde(default)]
    features: FeaturesToml,
    #[serde(default)]
    awd: AwdToml,
    #[serde(default)]
    realtime: RealtimeToml,
    #[serde(default)]
    logging: LoggingToml,
    #[serde(default)]
    challenge: ChallengeToml,
}

#[derive(Debug, Deserialize)]
struct ApplicationToml {
    #[serde(default = "default_changelog_path")]
    changelog_path: String,
    #[serde(default = "default_main_url")]
    main_url: String,
    #[serde(default = "default_timezone")]
    timezone: String,
}

impl Default for ApplicationToml {
    fn default() -> Self {
        Self {
            changelog_path: default_changelog_path(),
            main_url: default_main_url(),
            timezone: default_timezone(),
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
    #[serde(default = "default_log_dir")]
    log_dir: String,
}

impl Default for ServerToml {
    fn default() -> Self {
        Self {
            listen_ip: default_listen_ip(),
            listen_port: default_listen_port(),
            work_dir: default_work_dir(),
            log_dir: default_log_dir(),
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
struct PathsToml {
    #[serde(default)]
    changelog_path: Option<String>,
    #[serde(default)]
    challenges_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChallengeToml {
    #[serde(default = "default_score_decay")]
    event_score_decay: i64,
    #[serde(default = "default_min_percent")]
    event_score_min_percent: f64,
    #[serde(default = "default_instance_max")]
    instance_max_per_user: i64,
    #[serde(default = "default_destroy_delay")]
    instance_destroy_delay: i64,
    #[serde(default = "default_http_prefix")]
    http_prefix: String,
    #[serde(default = "default_node_ip")]
    node_ip: String,
}

impl Default for ChallengeToml {
    fn default() -> Self {
        Self {
            event_score_decay: default_score_decay(),
            event_score_min_percent: default_min_percent(),
            instance_max_per_user: default_instance_max(),
            instance_destroy_delay: default_destroy_delay(),
            http_prefix: default_http_prefix(),
            node_ip: default_node_ip(),
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
    #[serde(default)]
    host_network: bool,
    #[serde(default = "default_flagserver_image")]
    flagserver_image: String,
    #[serde(default = "default_judgeserver_image")]
    judgeserver_image: String,
}

impl Default for AwdToml {
    fn default() -> Self {
        Self {
            crypto_from_app_secret: true,
            host_network: false,
            flagserver_image: default_flagserver_image(),
            judgeserver_image: default_judgeserver_image(),
        }
    }
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
}

impl Default for LoggingToml {
    fn default() -> Self {
        Self {
            filter: default_log_filter(),
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

        let changelog_path = file
            .paths
            .changelog_path
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(file.application.changelog_path);

        Ok(Self {
            server: ServerConfig {
                listen_ip: file.server.listen_ip,
                listen_port: file.server.listen_port,
                work_dir: file.server.work_dir.clone(),
                log_dir: file.server.log_dir.clone(),
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
                log_dir: file.server.log_dir,
                changelog_path,
            },
            awd: AwdStaticConfig {
                crypto_from_app_secret: file.awd.crypto_from_app_secret,
                host_network: file.awd.host_network,
                flagserver_image: file.awd.flagserver_image,
                judgeserver_image: file.awd.judgeserver_image,
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
            },
            challenge: ChallengeConfig {
                event_score_decay: file.challenge.event_score_decay.to_string(),
                event_score_min_percent: file.challenge.event_score_min_percent.to_string(),
                instance_max_per_user: file.challenge.instance_max_per_user.to_string(),
                instance_destroy_delay: file.challenge.instance_destroy_delay.to_string(),
                http_prefix: file.challenge.http_prefix,
                node_ip: file.challenge.node_ip,
                challenges_dir: path_or_default(file.paths.challenges_dir, "./challenges"),
                main_url: file.application.main_url,
            },
            timezone: file.application.timezone,
        })
    }

    /// Log non-secret configuration sources for operators (never logs secrets).
    pub fn log_source_summary(&self) {
        tracing::info!(
            listen = %format!("{}:{}", self.server.listen_ip, self.server.listen_port),
            work_dir = %self.server.work_dir,
            log_dir = %self.server.log_dir,
            storage_endpoint = %self.storage.endpoint_url,
            storage_region = %self.storage.region,
            cors_origins = ?self.cors.allowed_origins,
            enable_unsafe_sql_admin = self.features.enable_unsafe_sql_admin,
            enable_web_terminal = self.features.enable_web_terminal,
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

fn path_or_default(value: Option<String>, default: &str) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
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
fn default_log_dir() -> String {
    "./logs".to_string()
}
/// Empty timezone = leave the process local timezone untouched.
fn default_timezone() -> String {
    String::new()
}
fn default_changelog_path() -> String {
    "./CHANGELOG.md".to_string()
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
fn default_score_decay() -> i64 {
    15
}
fn default_min_percent() -> f64 {
    0.45
}
fn default_instance_max() -> i64 {
    2
}
fn default_destroy_delay() -> i64 {
    60
}
fn default_http_prefix() -> String {
    "http://".to_string()
}
fn default_node_ip() -> String {
    "127.0.0.1".to_string()
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
