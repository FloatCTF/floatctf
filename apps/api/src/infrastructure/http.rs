//! 平台**出网**（访问互联网）的统一入口：`ProxyReqwest`。
//!
//! 与内网/容器探测用的裸 `reqwest::Client` 严格区分：
//!   - `gamebox::healthcheck` 里的客户端刻意 `.no_proxy()`，目标是容器私网地址；
//!   - 本类型的目标是**互联网**（例如下载外部制品），必须走代理（若配置了）。
//!
//! ## 代理来源（优先级从高到低）
//!
//! 1. 设置 `settings.OUTBOUND_PROXY`（**平台参数**，管理端可改；TOML `[proxy] url`
//!    只是它的 seed 默认值）。改完不用重启：设置本身有 60s 缓存，代理串变化时重建 client。
//! 2. 标准代理环境变量（`HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY`，大小写均可）——
//!    容器运行时/CI 注入代理时**开箱即用**，与 curl / git / reqwest 的默认行为一致。
//! 3. 都没有 → 直连。
//!
//! 第 2 条是有意为之：本平台就部署在"出网必须走代理"的机器上（不配置就直连会被挂住，
//! 表现为下载超时）。若某处确实需要绕开环境代理，请显式配置 `OUTBOUND_PROXY` 指到
//! 该走的方向，或用 `ProxyReqwest` 之外的裸客户端。

use std::sync::Arc;
use std::time::Duration;

use sea_orm::DbConn;
use tokio::sync::Mutex;

use super::settings::get_setting;

/// 平台出网代理的设置键（管理端可编辑；TOML `[proxy] url` 为其 seed 默认值）。
pub const OUTBOUND_PROXY_SETTING_KEY: &str = "OUTBOUND_PROXY";

/// 未显式配置时按此顺序读取的标准代理环境变量。
const PROXY_ENV_KEYS: [&str; 6] = [
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "ALL_PROXY",
    "all_proxy",
];

/// 绝不走代理的目标（内部服务名 + 回环）：纵深防御，避免误指向内网的请求被送去代理。
const DEFAULT_NO_PROXY: &str = "localhost,127.0.0.1,::1,api,caddy,postgres,redis,rustfs";

/// 连接超时（到代理或直连目标的 TCP/TLS 建连）。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// 单次请求总超时。制品下载可能有几十 MB，给得比一般 API 调用宽。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// 代理来源。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ProxySource {
    /// 显式设置（优先级最高）。
    Setting(String),
    /// 标准环境变量。
    Environment(String),
    /// 直连。
    Direct,
}

impl ProxySource {
    fn url(&self) -> Option<&str> {
        match self {
            Self::Setting(url) | Self::Environment(url) => Some(url),
            Self::Direct => None,
        }
    }

    /// 脱敏展示（不含凭据），用于日志与接口。
    fn describe(&self) -> String {
        match self {
            Self::Setting(url) => {
                format!("setting {}", crate::core::config::redact_proxy(url))
            }
            Self::Environment(url) => {
                format!("env {}", crate::core::config::redact_proxy(url))
            }
            Self::Direct => "direct".to_string(),
        }
    }
}

/// 解析生效的代理来源。抽成纯函数以便单测（不依赖进程环境）。
fn resolve_proxy_source(
    setting_value: &str,
    env_lookup: impl Fn(&str) -> Option<String>,
) -> ProxySource {
    let trimmed = setting_value.trim();
    if !trimmed.is_empty() {
        return ProxySource::Setting(trimmed.to_string());
    }
    for key in PROXY_ENV_KEYS {
        if let Some(value) = env_lookup(key) {
            let value = value.trim().to_string();
            if !value.is_empty() {
                return ProxySource::Environment(value);
            }
        }
    }
    ProxySource::Direct
}

/// 平台出网客户端句柄：按生效的代理串缓存一个 [`reqwest::Client`]。
#[derive(Clone)]
pub struct ProxyReqwest {
    db: DbConn,
    /// `(生效代理串, client)`；代理串变化时重建。空串代表直连。
    cache: Arc<Mutex<Option<(String, reqwest::Client)>>>,
}

impl ProxyReqwest {
    pub fn new(db: DbConn) -> Self {
        Self {
            db,
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// 生效的代理来源（脱敏展示，如 `setting http://proxy:3128` / `env …` / `direct`）。
    pub async fn describe_proxy(&self) -> String {
        self.resolve().await.describe()
    }

    /// 是否经由代理出网（供日志/接口展示）。
    pub async fn is_proxied(&self) -> bool {
        !matches!(self.resolve().await, ProxySource::Direct)
    }

    async fn resolve(&self) -> ProxySource {
        // 设置缺失或读取失败时回落环境变量：出网能力不应因为一次设置读取失败而整体不可用。
        let setting = get_setting(&self.db, OUTBOUND_PROXY_SETTING_KEY)
            .await
            .unwrap_or_default();
        resolve_proxy_source(&setting, |key| std::env::var(key).ok())
    }

    /// 取当前应使用的客户端；生效代理变化时重建，否则复用缓存。
    pub async fn client(&self) -> anyhow::Result<reqwest::Client> {
        let source = self.resolve().await;
        let key = source.url().unwrap_or("").to_string();
        let mut guard = self.cache.lock().await;
        if let Some((cached_key, client)) = guard.as_ref() {
            if cached_key == &key {
                return Ok(client.clone());
            }
        }
        let client = build_client(source.url())?;
        *guard = Some((key, client.clone()));
        Ok(client)
    }

    /// 带**大小上限**的 GET，用于下载外部制品。
    ///
    /// 边读边计数：`Content-Length` 缺失或撒谎都不会把内存吃光。
    pub async fn get_bytes(&self, url: &str, max_bytes: u64) -> anyhow::Result<Vec<u8>> {
        let client = self.client().await?;
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("请求 {url} 失败：{e}"))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("请求 {url} 返回 HTTP {status}");
        }
        if let Some(len) = response.content_length() {
            if len > max_bytes {
                anyhow::bail!("{url} 响应体 {len} 字节，超过上限 {max_bytes}");
            }
        }

        let mut buffer: Vec<u8> = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| anyhow::anyhow!("读取 {url} 响应失败：{e}"))?
        {
            if buffer.len() as u64 + chunk.len() as u64 > max_bytes {
                anyhow::bail!("{url} 响应体超过上限 {max_bytes} 字节，已中止");
            }
            buffer.extend_from_slice(&chunk);
        }
        Ok(buffer)
    }
}

/// 构造客户端：`proxy` 为 `None` → 直连（显式关掉环境代理，避免"设置里没配却偷偷走代理"）；
/// `Some` → 全部请求走该代理。
///
/// 错误信息**不含代理串原文**（可能带凭据），只说明形状问题。
fn build_client(proxy: Option<&str>) -> anyhow::Result<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .user_agent(concat!("FloatCTF/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT);

    let builder = match proxy {
        None => builder.no_proxy(),
        Some(proxy) => {
            // reqwest 会把没有 scheme 的串默默当成 `http://<host>`，所以这里必须自己做形状
            // 校验：管理员在设置里填错值时应当明确失败，而不是"看起来生效了"。
            let parsed = url::Url::parse(proxy).map_err(|e| {
                anyhow::anyhow!("出网代理不是合法的 URL（{e}）：期望形如 http://host:7890")
            })?;
            if parsed.host_str().is_none() {
                anyhow::bail!("出网代理缺少主机名（期望形如 http://host:7890）");
            }
            let parsed_proxy = reqwest::Proxy::all(proxy)
                .map_err(|e| anyhow::anyhow!("出网代理不是可用的地址：{e}"))?;
            builder.proxy(parsed_proxy.no_proxy(reqwest::NoProxy::from_string(DEFAULT_NO_PROXY)))
        }
    };

    builder
        .build()
        .map_err(|e| anyhow::anyhow!("构建出网 HTTP 客户端失败：{e}"))
}

/// Actix `web::Data` 句柄。
pub type WebProxyReqwest = actix_web::web::Data<ProxyReqwest>;

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn setting_wins_over_environment() {
        let source = resolve_proxy_source("http://from-setting:1", |_| {
            Some("http://from-env:2".to_string())
        });
        assert_eq!(source, ProxySource::Setting("http://from-setting:1".into()));
    }

    #[test]
    fn environment_is_used_when_setting_is_empty() {
        let source = resolve_proxy_source("  ", |key| {
            (key == "HTTPS_PROXY").then(|| "http://from-env:2".to_string())
        });
        assert_eq!(source, ProxySource::Environment("http://from-env:2".into()));
    }

    #[test]
    fn direct_when_neither_is_configured() {
        assert_eq!(resolve_proxy_source("", no_env), ProxySource::Direct);
        // 空字符串的环境变量不算配置
        assert_eq!(
            resolve_proxy_source("", |_| Some("   ".to_string())),
            ProxySource::Direct
        );
    }

    #[test]
    fn describe_is_redacted() {
        let source = ProxySource::Setting("http://user:secret@proxy.internal:3128".into());
        let described = source.describe();
        assert_eq!(described, "setting http://proxy.internal:3128");
        assert!(!described.contains("secret"));
    }

    #[test]
    fn direct_client_builds_and_ignores_env_proxy() {
        assert!(build_client(None).is_ok());
    }

    #[test]
    fn http_and_socks_proxies_build() {
        assert!(build_client(Some("http://127.0.0.1:7890")).is_ok());
        assert!(build_client(Some("http://user:pass@127.0.0.1:7890")).is_ok());
        assert!(build_client(Some("socks5h://127.0.0.1:1080")).is_ok());
    }

    #[test]
    fn invalid_proxy_is_rejected_without_leaking_it() {
        let err = build_client(Some("not-a-url")).expect_err("应当拒绝");
        let message = err.to_string();
        assert!(message.contains("出网代理"));
        assert!(!message.contains("not-a-url"));
    }
}
