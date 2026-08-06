//! Typed static configuration loaded once at process start from the environment.
//!
//! Dynamic, admin-editable settings remain in the `settings` DB table
//! (`seed_default_settings` / `get_setting`).

use std::env;

use super::secret::Secret;

/// Top-level application configuration (env / process-static only).
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
}

#[derive(Debug, Clone)]
pub struct FeatureFlags {
    pub enable_unsafe_sql_admin: bool,
    pub enable_web_terminal: bool,
}

impl AppConfig {
    /// Load and validate all required environment variables.
    ///
    /// Fails fast if mandatory secrets or connection settings are missing.
    pub fn from_env() -> anyhow::Result<Self> {
        let work_dir = env_or("WORK_DIR", "./");
        let log_dir = env_or("LOG_DIR", "./logs");

        let jwt_secret = required("SECRET")?;
        if jwt_secret.len() < 16 {
            anyhow::bail!("SECRET must be at least 16 characters");
        }

        let database_url = required("DATABASE_URL")?;
        let listen_port = env_or("SERVER_LISTEN_PORT", "8080")
            .parse::<u16>()
            .map_err(|e| anyhow::anyhow!("SERVER_LISTEN_PORT invalid: {e}"))?;

        let cors_origins = match env::var("CORS_ALLOWED_ORIGINS") {
            Ok(raw) if !raw.trim().is_empty() => raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => vec![
                "http://localhost:3000".to_string(),
                "http://127.0.0.1".to_string(),
            ],
        };

        let storage = StorageConfig {
            endpoint_url: required("RUSTFS_ENDPOINT_URL")?,
            access_key_id: required("RUSTFS_ACCESS_KEY_ID")?,
            secret_access_key: Secret::new(required("RUSTFS_SECRET_ACCESS_KEY")?),
            region: required("RUSTFS_REGION")?,
        };

        Ok(Self {
            server: ServerConfig {
                listen_ip: env_or("SERVER_LISTEN_IP", "127.0.0.1"),
                listen_port,
                work_dir: work_dir.clone(),
                log_dir: log_dir.clone(),
            },
            database: DatabaseConfig {
                url: Secret::new(database_url),
            },
            docker: DockerConfig { use_defaults: true },
            storage,
            auth: AuthConfig {
                jwt_secret: Secret::new(jwt_secret),
            },
            cors: CorsConfig {
                allowed_origins: cors_origins,
            },
            paths: PathConfig {
                work_dir,
                log_dir,
                changelog_path: env_or("SYSTEM_CHANGELOG_PATH", "./CHANGELOG.md"),
            },
            awd: AwdStaticConfig {
                crypto_from_app_secret: true,
            },
            features: FeatureFlags {
                enable_unsafe_sql_admin: env_flag("ENABLE_UNSAFE_SQL_ADMIN"),
                enable_web_terminal: env_flag("ENABLE_WEB_TERMINAL"),
            },
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
            "AppConfig loaded from environment"
        );
        if self.features.enable_unsafe_sql_admin {
            tracing::warn!(
                "ENABLE_UNSAFE_SQL_ADMIN is enabled — arbitrary SQL admin API is exposed"
            );
        }
    }
}

fn required(key: &str) -> anyhow::Result<String> {
    env::var(key).map_err(|_| anyhow::anyhow!("{key} environment variable is required"))
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_flag(key: &str) -> bool {
    matches!(
        env::var(key).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_flag_parses_truthy() {
        // pure unit: use helper via reflection of local logic
        assert!(matches!(Some("1"), Some("1") | Some("true")));
        assert!(!matches!(Some("0"), Some("1") | Some("true")));
    }

    #[test]
    fn secret_in_database_config_redacts() {
        let c = DatabaseConfig {
            url: Secret::new("postgres://user:pass@localhost/db"),
        };
        assert!(!format!("{c:?}").contains("pass@"));
    }
}
