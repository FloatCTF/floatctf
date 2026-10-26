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
    /// 平台**出网**（访问互联网）的代理默认值。运行时以 `settings.OUTBOUND_PROXY`
    /// 为准（管理员可改），这里只是它的 seed 默认值。
    pub proxy: ProxyConfig,
    /// 主站地址前缀（[application] main_url），作为 MAIN_URL 设置的 seed 默认值
    pub main_url: String,
    /// OpenID Connect Provider（`[oidc]` 段，默认关闭）。
    pub oidc: OidcConfig,
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
    /// JWT 签名主密钥（HS512）。≥16 字符，`Secret` 包装（Debug/日志脱敏）。
    ///
    /// 历史上它同时被用作 AWD flag 派生根与 AWDP 判题令牌根（风险清单 #7）。现已拆分：
    /// 见 [`AuthConfig::awd_root_key`] / [`AuthConfig::internal_token_key`] —— 两个专用键
    /// **未配置时回落本值**（兼容既有部署），配置后即可独立轮换，互不影响 JWT。
    pub jwt_secret: Secret,
    /// AWD/AWDP 派生根（flag、实例密钥等，`AwdCrypto` 的 HKDF 根）。缺省回落 `jwt_secret`。
    pub awd_root_key: Option<Secret>,
    /// AWDP 判题容器 `INTERNAL_TOKEN` 派生根。缺省回落 `jwt_secret`。
    ///
    /// ⚠️ 这个值会下发进判题容器；与 `jwt_secret` 分开正是为了「判题容器被攻陷 ≠ 可伪造 JWT」。
    pub internal_token_key: Option<Secret>,
}

impl AuthConfig {
    /// 生效的 AWD 派生根（专用键优先，缺省回落主密钥）。
    pub fn awd_root_key(&self) -> &Secret {
        self.awd_root_key.as_ref().unwrap_or(&self.jwt_secret)
    }

    /// 生效的判题令牌派生根（专用键优先，缺省回落主密钥）。
    pub fn internal_token_key(&self) -> &Secret {
        self.internal_token_key.as_ref().unwrap_or(&self.jwt_secret)
    }

    /// 两个专用键是否都已显式配置（bootstrap 用它决定是否告警"仍在回落主密钥"）。
    pub fn uses_dedicated_keys(&self) -> bool {
        self.awd_root_key.is_some() && self.internal_token_key.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
}

/// 平台出网代理（`[proxy]` 段）。
///
/// 只作用于平台**主动访问互联网**的请求（目前只有 `ProxyReqwest`）；数据库、Redis、
/// RustFS、容器/内网探测一律直连，绝不受这里影响。运行时以管理端可改的
/// `OUTBOUND_PROXY` 设置为准，本结构只是它的 seed 默认值。
#[derive(Debug, Clone, Default)]
pub struct ProxyConfig {
    /// 代理地址，例如 `http://host:7890` / `http://user:pass@host:7890` / `socks5h://host:1080`。
    /// 空 = 直连。可含凭据，因此用 [`Secret`] 包装、日志只输出主机端口。
    pub url: Option<Secret>,
}

#[derive(Debug, Clone)]
pub struct PathConfig {
    pub work_dir: String,
}

/// AWD 进程静态配置（非每场赛事密钥）。
#[derive(Debug, Clone)]
pub struct AwdStaticConfig {
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

/// OpenID Connect Provider 静态配置（`[oidc]` 段）。
///
/// `enabled = false`（默认）时其余字段不校验、协议端点全部 404、登录不写 SSO Cookie。
/// `enabled = true` 时 issuer 与签名 key ring 在启动期严格校验，任何不合法都拒绝启动
/// （fail closed，绝不临时生成 key）。
#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub enabled: bool,
    /// Issuer：只允许 origin（`https://host[:port]`，无 path/query/fragment/尾斜杠）；
    /// `http` 只允许 loopback 主机（本地开发）。ID Token `iss` 与 Discovery 逐字节等于它。
    pub issuer: String,
    /// 签名 key ring：恰好一个 `active`（用于签发），其余仅发布公钥供验签过渡。
    pub signing_keys: Vec<OidcSigningKeyConfig>,
    /// 第一方 SSO 会话有效期（秒）。
    pub session_ttl_secs: i64,
    /// 授权事务（登录/同意页）有效期（秒）。
    pub transaction_ttl_secs: i64,
    /// 授权码有效期（秒，1..=120）。
    pub authorization_code_ttl_secs: i64,
    /// Access token 有效期（秒）。
    pub access_token_ttl_secs: i64,
    /// ID Token 有效期（秒）。
    pub id_token_ttl_secs: i64,
    /// 轮换 client secret 后旧 secret 的默认宽限期（秒）。
    pub secret_rotation_grace_secs: i64,
    /// 是否允许 `http://127.0.0.1|[::1]|localhost` 回调（本地开发 / 原生应用 loopback）。
    pub allow_loopback_redirect_uris: bool,
    /// 新登记应用是否需要管理员审批后才能使用（默认 true）。
    pub require_client_approval: bool,
    /// `prompt=select_account` 是否进入独立的账号选择页（默认 true）。
    /// `false` 仅为临时兼容/回滚模式：`select_account` 回到旧行为（导向同意页）。
    pub account_chooser: bool,
    /// 浏览器账号容器（`__Host-fctf_accounts`）与多账号（默认 false = 旧版单会话 `__Host-fctf_sso`）。
    pub multi_account: bool,
    /// 一个浏览器容器内最多同时登录的账号数（超出时淘汰最久未用的会话）。
    pub max_browser_accounts: usize,
    /// 浏览器账号容器 Cookie 的滑动有效期（秒）；容器内各账号会话仍按 `session_ttl_secs` 过期。
    pub browser_container_ttl_secs: i64,
    /// FloatCTF 主站改用服务端可撤销的 Cookie 会话（默认 false = 旧版 localStorage JWT）。要求 `multi_account`。
    pub unified_web_session: bool,
    /// 除 issuer 外，同一 API 实例服务的其他主站 origin（必须与 issuer 同主机，Cookie 才能共享）。
    pub first_party_origins: Vec<String>,
    /// 标准退出：`end_session_endpoint`（RP-Initiated Logout 1.0）+ Back-Channel Logout 1.0 + ID Token `sid`（默认 false）。
    pub backchannel_logout: bool,
    /// 允许解析到内网/环回地址的 Back-Channel 目标（`host:port`，小写，逐字匹配）。其余目标只能是公网地址。
    pub backchannel_trusted_hosts: Vec<String>,
    /// 额外信任的 CA（PEM，内网 RP 使用私有 CA 时配置）；相对路径以配置文件所在目录为基准。
    pub backchannel_extra_ca_file: Option<std::path::PathBuf>,
    /// 单次 Back-Channel 投递超时（秒）。
    pub backchannel_timeout_secs: u64,
    /// Logout Token 有效期（秒）。
    pub logout_token_ttl_secs: i64,
}

impl OidcConfig {
    /// `[oidc]` 段全部缺省时的配置（OIDC 关闭；测试与工具构造用）。
    pub fn disabled_defaults() -> Self {
        parse_oidc_config(OidcToml::default(), Path::new("."))
            .expect("default OIDC config is valid")
    }

    /// 主站 Cookie 会话允许的 `Origin`：issuer 本身 + `first_party_origins`。
    pub fn is_first_party_origin(&self, origin: &str) -> bool {
        origin == self.issuer || self.first_party_origins.iter().any(|o| o == origin)
    }
}

/// 一把 OIDC 签名 key（RSA 私钥 PEM 文件引用）。
#[derive(Debug, Clone)]
pub struct OidcSigningKeyConfig {
    /// JWS `kid`（`[A-Za-z0-9._-]{1,64}`，key ring 内唯一）。
    pub kid: String,
    /// RSA 私钥 PEM 文件（PKCS#8 或 PKCS#1）；相对路径以配置文件所在目录为基准。
    pub private_key_file: std::path::PathBuf,
    /// 是否为当前签发 key（恰好一把）；非 active 的 key 只在 JWKS 中发布公钥。
    pub active: bool,
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
    #[serde(default)]
    proxy: ProxyToml,
    #[serde(default)]
    oidc: OidcToml,
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
    /// 可选：AWD/AWDP 派生根（≥16 字符）。缺省回落 `jwt_secret`。
    #[serde(default)]
    awd_root_key: Option<String>,
    /// 可选：AWDP 判题令牌派生根（≥16 字符）。缺省回落 `jwt_secret`。
    #[serde(default)]
    internal_token_key: Option<String>,
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

/// canonical AWD/AWDP 运行时镜像默认值（与 install.sh 模板、build-runtime-images.sh
/// 的 `--registry ghcr.io/floatctf` 输出一致）。`:latest` 只是 TOML 缺键时的兜底；
/// 生产由 install.sh 渲染成 `:${VERSION}` 的钉版引用（`warn_if_latest` 会对浮动 tag 告警）。
fn default_practice_judgeserver_image() -> String {
    "ghcr.io/floatctf/awdp-judgeserver:latest".to_string()
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

/// `[proxy]` 段：平台出网代理的 seed 默认值（运行时以 `OUTBOUND_PROXY` 设置为准）。
#[derive(Debug, Deserialize, Default)]
struct ProxyToml {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OidcToml {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    issuer: String,
    #[serde(default)]
    signing_keys: Vec<OidcSigningKeyToml>,
    #[serde(default = "default_oidc_session_ttl_secs")]
    session_ttl_secs: i64,
    #[serde(default = "default_oidc_transaction_ttl_secs")]
    transaction_ttl_secs: i64,
    #[serde(default = "default_oidc_authorization_code_ttl_secs")]
    authorization_code_ttl_secs: i64,
    #[serde(default = "default_oidc_access_token_ttl_secs")]
    access_token_ttl_secs: i64,
    #[serde(default = "default_oidc_id_token_ttl_secs")]
    id_token_ttl_secs: i64,
    #[serde(default = "default_oidc_secret_rotation_grace_secs")]
    secret_rotation_grace_secs: i64,
    #[serde(default)]
    allow_loopback_redirect_uris: bool,
    #[serde(default = "default_true")]
    require_client_approval: bool,
    #[serde(default = "default_true")]
    account_chooser: bool,
    #[serde(default)]
    multi_account: bool,
    #[serde(default = "default_oidc_max_browser_accounts")]
    max_browser_accounts: usize,
    #[serde(default = "default_oidc_browser_container_ttl_secs")]
    browser_container_ttl_secs: i64,
    #[serde(default)]
    unified_web_session: bool,
    #[serde(default)]
    first_party_origins: Vec<String>,
    #[serde(default)]
    backchannel_logout: bool,
    #[serde(default)]
    backchannel_trusted_hosts: Vec<String>,
    #[serde(default)]
    backchannel_extra_ca_file: Option<String>,
    #[serde(default = "default_oidc_backchannel_timeout_secs")]
    backchannel_timeout_secs: u64,
    #[serde(default = "default_oidc_logout_token_ttl_secs")]
    logout_token_ttl_secs: i64,
}

impl Default for OidcToml {
    fn default() -> Self {
        Self {
            enabled: false,
            issuer: String::new(),
            signing_keys: Vec::new(),
            session_ttl_secs: default_oidc_session_ttl_secs(),
            transaction_ttl_secs: default_oidc_transaction_ttl_secs(),
            authorization_code_ttl_secs: default_oidc_authorization_code_ttl_secs(),
            access_token_ttl_secs: default_oidc_access_token_ttl_secs(),
            id_token_ttl_secs: default_oidc_id_token_ttl_secs(),
            secret_rotation_grace_secs: default_oidc_secret_rotation_grace_secs(),
            allow_loopback_redirect_uris: false,
            require_client_approval: true,
            account_chooser: true,
            multi_account: false,
            max_browser_accounts: default_oidc_max_browser_accounts(),
            browser_container_ttl_secs: default_oidc_browser_container_ttl_secs(),
            unified_web_session: false,
            first_party_origins: Vec::new(),
            backchannel_logout: false,
            backchannel_trusted_hosts: Vec::new(),
            backchannel_extra_ca_file: None,
            backchannel_timeout_secs: default_oidc_backchannel_timeout_secs(),
            logout_token_ttl_secs: default_oidc_logout_token_ttl_secs(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OidcSigningKeyToml {
    kid: String,
    private_key_file: String,
    #[serde(default)]
    active: bool,
}

fn default_true() -> bool {
    true
}
fn default_oidc_session_ttl_secs() -> i64 {
    8 * 3600
}
fn default_oidc_transaction_ttl_secs() -> i64 {
    600
}
fn default_oidc_authorization_code_ttl_secs() -> i64 {
    60
}
fn default_oidc_access_token_ttl_secs() -> i64 {
    3600
}
fn default_oidc_id_token_ttl_secs() -> i64 {
    3600
}
fn default_oidc_secret_rotation_grace_secs() -> i64 {
    24 * 3600
}
fn default_oidc_max_browser_accounts() -> usize {
    5
}
fn default_oidc_browser_container_ttl_secs() -> i64 {
    30 * 24 * 3600
}
fn default_oidc_backchannel_timeout_secs() -> u64 {
    5
}
fn default_oidc_logout_token_ttl_secs() -> i64 {
    120
}

/// 校验 OIDC issuer：只允许 origin，`https` 必需，`http` 仅限 loopback 主机。
///
/// 返回规范化前的原字符串——issuer 必须与 ID Token `iss` 逐字节一致，因此不做任何改写，
/// 只拒绝不合法的形态（含尾斜杠、默认端口显式写出等会让 RP 比对失败的写法）。
pub(crate) fn validate_oidc_issuer(issuer: &str) -> anyhow::Result<()> {
    let parsed = url::Url::parse(issuer).map_err(|e| {
        anyhow::anyhow!("oidc.issuer 不是合法 URL（{e}）：期望形如 https://ctf.example.com")
    })?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("oidc.issuer 缺少主机名"))?;
    match parsed.scheme() {
        "https" => {}
        "http" if is_loopback_host(host) => {}
        "http" => anyhow::bail!(
            "oidc.issuer 必须使用 https（http 只允许 localhost/127.0.0.1/[::1] 本地开发）"
        ),
        other => anyhow::bail!("oidc.issuer 不支持的 scheme：{other}"),
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("oidc.issuer 不得包含 userinfo");
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        anyhow::bail!("oidc.issuer 不得包含 query 或 fragment");
    }
    // url crate 会把空 path 规范化为 "/"；原串必须恰好是 scheme://host[:port]。
    if parsed.path() != "/" || issuer.ends_with('/') {
        anyhow::bail!("oidc.issuer 只能是 origin（scheme://host[:port]），不得带路径或尾斜杠");
    }
    if parsed.origin().ascii_serialization() != issuer {
        anyhow::bail!(
            "oidc.issuer 必须是规范 origin 写法（小写主机、不写默认端口）：期望 {}",
            parsed.origin().ascii_serialization()
        );
    }
    Ok(())
}

/// loopback 主机判定（issuer 与 redirect_uri 的开发放行共用）。
pub(crate) fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

fn parse_oidc_config(file: OidcToml, config_dir: &Path) -> anyhow::Result<OidcConfig> {
    let config = OidcConfig {
        enabled: file.enabled,
        issuer: file.issuer.trim().to_string(),
        signing_keys: file
            .signing_keys
            .into_iter()
            .map(|key| {
                let path = std::path::PathBuf::from(key.private_key_file.trim());
                OidcSigningKeyConfig {
                    kid: key.kid.trim().to_string(),
                    private_key_file: if path.is_absolute() {
                        path
                    } else {
                        config_dir.join(path)
                    },
                    active: key.active,
                }
            })
            .collect(),
        session_ttl_secs: file.session_ttl_secs,
        transaction_ttl_secs: file.transaction_ttl_secs,
        authorization_code_ttl_secs: file.authorization_code_ttl_secs,
        access_token_ttl_secs: file.access_token_ttl_secs,
        id_token_ttl_secs: file.id_token_ttl_secs,
        secret_rotation_grace_secs: file.secret_rotation_grace_secs,
        allow_loopback_redirect_uris: file.allow_loopback_redirect_uris,
        require_client_approval: file.require_client_approval,
        account_chooser: file.account_chooser,
        multi_account: file.multi_account,
        max_browser_accounts: file.max_browser_accounts,
        browser_container_ttl_secs: file.browser_container_ttl_secs,
        unified_web_session: file.unified_web_session,
        first_party_origins: file
            .first_party_origins
            .iter()
            .map(|o| o.trim().to_string())
            .collect(),
        backchannel_logout: file.backchannel_logout,
        backchannel_trusted_hosts: file
            .backchannel_trusted_hosts
            .iter()
            .map(|h| h.trim().to_ascii_lowercase())
            .collect(),
        backchannel_extra_ca_file: file
            .backchannel_extra_ca_file
            .map(|p| std::path::PathBuf::from(p.trim()))
            .map(|p| {
                if p.is_absolute() {
                    p
                } else {
                    config_dir.join(p)
                }
            }),
        backchannel_timeout_secs: file.backchannel_timeout_secs,
        logout_token_ttl_secs: file.logout_token_ttl_secs,
    };
    if !config.enabled {
        return Ok(config);
    }

    validate_oidc_issuer(&config.issuer)?;
    if config.signing_keys.is_empty() {
        anyhow::bail!(
            "oidc.enabled = true 但未配置 [[oidc.signing_keys]]（拒绝启动，不会临时生成 key）"
        );
    }
    let mut seen = std::collections::HashSet::new();
    for key in &config.signing_keys {
        let kid_ok = !key.kid.is_empty()
            && key.kid.len() <= 64
            && key
                .kid
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !kid_ok {
            anyhow::bail!(
                "oidc.signing_keys.kid 非法：{:?}（只允许 [A-Za-z0-9._-]，1-64 字符）",
                key.kid
            );
        }
        if !seen.insert(key.kid.clone()) {
            anyhow::bail!("oidc.signing_keys.kid 重复：{}", key.kid);
        }
    }
    let active = config.signing_keys.iter().filter(|k| k.active).count();
    if active != 1 {
        anyhow::bail!(
            "oidc.signing_keys 必须恰好有一把 active = true 的签发 key（当前 {active} 把）"
        );
    }
    let bounded = |name: &str, value: i64, min: i64, max: i64| -> anyhow::Result<()> {
        if value < min || value > max {
            anyhow::bail!("oidc.{name} 必须在 {min}..={max} 之间（当前 {value}）");
        }
        Ok(())
    };
    bounded(
        "session_ttl_secs",
        config.session_ttl_secs,
        300,
        30 * 24 * 3600,
    )?;
    bounded(
        "transaction_ttl_secs",
        config.transaction_ttl_secs,
        60,
        3600,
    )?;
    bounded(
        "authorization_code_ttl_secs",
        config.authorization_code_ttl_secs,
        1,
        120,
    )?;
    bounded(
        "access_token_ttl_secs",
        config.access_token_ttl_secs,
        60,
        24 * 3600,
    )?;
    bounded("id_token_ttl_secs", config.id_token_ttl_secs, 60, 24 * 3600)?;
    bounded(
        "secret_rotation_grace_secs",
        config.secret_rotation_grace_secs,
        0,
        30 * 24 * 3600,
    )?;
    if config.max_browser_accounts == 0 || config.max_browser_accounts > 10 {
        anyhow::bail!(
            "oidc.max_browser_accounts 必须在 1..=10 之间（当前 {}）",
            config.max_browser_accounts
        );
    }
    bounded(
        "browser_container_ttl_secs",
        config.browser_container_ttl_secs,
        config.session_ttl_secs,
        180 * 24 * 3600,
    )?;
    if config.unified_web_session && !config.multi_account {
        anyhow::bail!(
            "oidc.unified_web_session = true 需要 oidc.multi_account = true（主站会话挂在浏览器账号容器上）"
        );
    }
    if config.backchannel_timeout_secs == 0 || config.backchannel_timeout_secs > 30 {
        anyhow::bail!(
            "oidc.backchannel_timeout_secs 必须在 1..=30 之间（当前 {}）",
            config.backchannel_timeout_secs
        );
    }
    bounded(
        "logout_token_ttl_secs",
        config.logout_token_ttl_secs,
        30,
        600,
    )?;
    for host in &config.backchannel_trusted_hosts {
        // 主机只能是域名 / IPv4 / [IPv6] 字面量：不允许通配符或空主机。
        let host_ok = |h: &str| {
            if let Some(inner) = h.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                return inner.parse::<std::net::Ipv6Addr>().is_ok();
            }
            !h.is_empty()
                && h.len() <= 253
                && !h.starts_with('.')
                && !h.starts_with('-')
                && h.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
        };
        let valid = host
            .rsplit_once(':')
            .is_some_and(|(h, port)| host_ok(h) && port.parse::<u16>().is_ok_and(|p| p > 0));
        if !valid {
            anyhow::bail!(
                "oidc.backchannel_trusted_hosts 项 {host:?} 必须形如 host:port（例如 10.66.66.7:8080）"
            );
        }
    }
    let issuer_host = url::Url::parse(&config.issuer)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string));
    for origin in &config.first_party_origins {
        validate_oidc_issuer(origin)
            .map_err(|e| anyhow::anyhow!("oidc.first_party_origins 项 {origin:?} 非法：{e}"))?;
        let host = url::Url::parse(origin)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string));
        if host != issuer_host {
            anyhow::bail!(
                "oidc.first_party_origins 项 {origin:?} 必须与 issuer 同主机（host-only Cookie 只在同主机间共享）"
            );
        }
    }
    Ok(config)
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
        // 可选专用键：设置时必须够长（同样是密钥），缺省回落 jwt_secret。
        let awd_root_key = optional_secret("auth.awd_root_key", file.auth.awd_root_key)?;
        let internal_token_key =
            optional_secret("auth.internal_token_key", file.auth.internal_token_key)?;
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
        let proxy_url = parse_optional_proxy_url("proxy.url", file.proxy.url)?;
        let config_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let oidc = parse_oidc_config(file.oidc, config_dir)?;

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
                awd_root_key,
                internal_token_key,
            },
            cors: CorsConfig {
                allowed_origins: file.cors.allowed_origins,
            },
            paths: PathConfig {
                work_dir: file.server.work_dir,
            },
            awd: AwdStaticConfig {
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
            proxy: ProxyConfig { url: proxy_url },
            oidc,
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
            outbound_proxy = %self
                .proxy
                .url
                .as_ref()
                .map(|url| redact_proxy(url.expose()))
                .unwrap_or_else(|| "direct".to_string()),
            oidc_enabled = self.oidc.enabled,
            oidc_issuer = %self.oidc.issuer,
            oidc_signing_kids = ?self.oidc.signing_keys.iter().map(|k| k.kid.as_str()).collect::<Vec<_>>(),
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

/// 可选出网代理：空 → `None`（直连）；非空必须是带 scheme 与 host 的合法 URL。
///
/// 只做**形状校验**，不校验可达性——代理不可达由使用方在请求时失败并如实报错，
/// 不在启动期阻断平台（否则代理故障会让整个平台起不来）。
fn parse_optional_proxy_url(name: &str, value: Option<String>) -> anyhow::Result<Option<Secret>> {
    match non_empty(value) {
        None => Ok(None),
        Some(v) => {
            let parsed = url::Url::parse(&v).map_err(|e| {
                anyhow::anyhow!("{name} 不是合法的代理 URL（{e}）：期望形如 http://host:7890")
            })?;
            if parsed.host_str().is_none() {
                anyhow::bail!("{name} 缺少主机名：{v}");
            }
            Ok(Some(Secret::new(v)))
        }
    }
}

/// 代理 URL 的脱敏展示：只保留 `scheme://host:port`，丢弃 userinfo / path / query。
pub(crate) fn redact_proxy(raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(parsed) => match parsed.port() {
            Some(port) => format!(
                "{}://{}:{}",
                parsed.scheme(),
                parsed.host_str().unwrap_or("?"),
                port
            ),
            None => format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or("?")),
        },
        Err(_) => "Secret(***)".to_string(),
    }
}

/// 可选密钥字段：缺省/空串 → `None`（调用方回落主密钥）；一旦设置就必须满足最小长度。
fn optional_secret(name: &str, value: Option<String>) -> anyhow::Result<Option<Secret>> {
    match non_empty(value) {
        None => Ok(None),
        Some(v) => {
            if v.len() < 16 {
                anyhow::bail!("{name} must be at least 16 characters when set");
            }
            Ok(Some(Secret::new(v)))
        }
    }
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
/// canonical AWD flagserver 镜像默认值（`:latest` 兜底；生产用 `${VERSION}` 钉版）。
fn default_flagserver_image() -> String {
    "ghcr.io/floatctf/awd-flagserver:latest".to_string()
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
/// canonical AWD judgeserver 镜像默认值（`:latest` 兜底；生产用 `${VERSION}` 钉版）。
fn default_judgeserver_image() -> String {
    "ghcr.io/floatctf/awd-judgeserver:latest".to_string()
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
    fn proxy_url_parsing_accepts_http_and_socks_and_rejects_garbage() {
        assert!(
            parse_optional_proxy_url("proxy.url", None)
                .unwrap()
                .is_none()
        );
        assert!(
            parse_optional_proxy_url("proxy.url", Some("   ".to_string()))
                .unwrap()
                .is_none()
        );

        for ok in [
            "http://127.0.0.1:7890",
            "http://user:pass@proxy.internal:3128",
            "socks5h://127.0.0.1:1080",
            "https://proxy.example.com",
        ] {
            let parsed = parse_optional_proxy_url("proxy.url", Some(ok.to_string())).unwrap();
            assert_eq!(parsed.expect("应当解析成功").expose(), ok);
        }

        // 缺 scheme / 缺主机必须在校验期拒绝：否则要到运行期请求时才发现，排查困难
        assert!(parse_optional_proxy_url("proxy.url", Some("127.0.0.1:7890".to_string())).is_err());
        assert!(parse_optional_proxy_url("proxy.url", Some("http://".to_string())).is_err());
    }

    #[test]
    fn redact_proxy_drops_credentials_and_path() {
        assert_eq!(
            redact_proxy("http://user:secret@proxy.internal:3128/some/path?x=1"),
            "http://proxy.internal:3128"
        );
        assert_eq!(
            redact_proxy("socks5h://127.0.0.1:1080"),
            "socks5h://127.0.0.1:1080"
        );
        assert_eq!(
            redact_proxy("http://proxy.example.com"),
            "http://proxy.example.com"
        );
    }

    #[test]
    fn runtime_image_defaults_match_canonical_ghcr_refs() {
        // 与 install.sh 模板 / build-runtime-images.sh --registry ghcr.io/floatctf
        // 的 canonical ref 一致；awdp 已扁平化（不再有 floatctf/infra/ 段）。
        assert_eq!(
            default_flagserver_image(),
            "ghcr.io/floatctf/awd-flagserver:latest"
        );
        assert_eq!(
            default_judgeserver_image(),
            "ghcr.io/floatctf/awd-judgeserver:latest"
        );
        assert_eq!(
            default_practice_judgeserver_image(),
            "ghcr.io/floatctf/awdp-judgeserver:latest"
        );
        // 三个默认值都必须走 canonical registry 前缀，且 awdp 不再带 infra/ 段。
        for image in [
            default_flagserver_image(),
            default_judgeserver_image(),
            default_practice_judgeserver_image(),
        ] {
            assert!(
                image.starts_with("ghcr.io/floatctf/"),
                "默认镜像必须指向 canonical registry: {image}"
            );
            assert!(
                !image.contains("/infra/"),
                "awdp 已扁平化，默认镜像不应含 infra/ 段: {image}"
            );
        }
        // 缺键时 AwdToml/AwdpToml 的 Default 必须与上面一致。
        let awd = AwdToml::default();
        assert_eq!(awd.flagserver_image, default_flagserver_image());
        assert_eq!(awd.judgeserver_image, default_judgeserver_image());
        assert_eq!(
            AwdpToml::default().practice_judgeserver_image,
            default_practice_judgeserver_image()
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

    /// 写一份最小可加载配置，`auth_extra` 追加到 `[auth]` 段（风险清单 #7 的拆分测试用）。
    fn write_auth_config(auth_extra: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            format!(
                r#"
                [database]
                url = "postgres://localhost/db"
                [auth]
                jwt_secret = "a-development-secret"
                {auth_extra}
                [redis]
                url = "redis://localhost:6379/"
                [rustfs]
                endpoint_url = "http://localhost:9000"
                access_key_id = "access"
                secret_access_key = "secret"
                region = "local"
                "#
            ),
        )
        .unwrap();
        (dir, path)
    }

    #[test]
    fn auth_keys_fall_back_to_jwt_secret_when_unset() {
        // 未配置专用键 → 三个用途仍共用主密钥（兼容既有部署），且能被上层识别出来告警。
        let (_dir, path) = write_auth_config("");
        let config = AppConfig::from_file(&path).unwrap();
        assert!(!config.auth.uses_dedicated_keys());
        assert_eq!(config.auth.awd_root_key().expose(), "a-development-secret");
        assert_eq!(
            config.auth.internal_token_key().expose(),
            "a-development-secret"
        );
    }

    #[test]
    fn auth_dedicated_keys_take_precedence_over_jwt_secret() {
        // 配置了专用键 → 各用途取各自的值，JWT 签名仍用主密钥（可独立轮换）。
        let (_dir, path) = write_auth_config(
            "awd_root_key = \"awd-root-key-0123456789\"\n                internal_token_key = \"internal-token-0123456789\"",
        );
        let config = AppConfig::from_file(&path).unwrap();
        assert!(config.auth.uses_dedicated_keys());
        assert_eq!(config.auth.jwt_secret.expose(), "a-development-secret");
        assert_eq!(
            config.auth.awd_root_key().expose(),
            "awd-root-key-0123456789"
        );
        assert_eq!(
            config.auth.internal_token_key().expose(),
            "internal-token-0123456789"
        );
    }

    #[test]
    fn auth_dedicated_key_must_meet_min_length() {
        let (_dir, path) = write_auth_config("awd_root_key = \"short\"");
        let error = AppConfig::from_file(&path).unwrap_err().to_string();
        assert!(error.contains("auth.awd_root_key"), "{error}");
    }

    #[test]
    fn auth_dedicated_keys_default_to_none_in_toml_struct() {
        // serde 缺省：老配置文件（只有 jwt_secret）解析后两个字段都是 None。
        let parsed: AuthToml = toml::from_str("jwt_secret = \"a-development-secret\"").unwrap();
        assert!(parsed.awd_root_key.is_none());
        assert!(parsed.internal_token_key.is_none());
    }

    /// 写一份最小可加载配置并追加 `[oidc]` 段（OIDC 配置测试用）。
    fn write_oidc_config(oidc: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            format!(
                r#"
                [database]
                url = "postgres://localhost/db"
                [auth]
                jwt_secret = "a-development-secret"
                [redis]
                url = "redis://localhost:6379/"
                [rustfs]
                endpoint_url = "http://localhost:9000"
                access_key_id = "access"
                secret_access_key = "secret"
                region = "local"
                {oidc}
                "#
            ),
        )
        .unwrap();
        (dir, path)
    }

    #[test]
    fn oidc_is_disabled_by_default_with_safe_defaults() {
        let (_dir, path) = write_oidc_config("");
        let config = AppConfig::from_file(&path).unwrap();
        assert!(!config.oidc.enabled);
        assert!(config.oidc.require_client_approval);
        assert!(config.oidc.account_chooser, "账号选择页默认开启");
        assert!(!config.oidc.allow_loopback_redirect_uris);
        assert_eq!(config.oidc.authorization_code_ttl_secs, 60);
        assert_eq!(config.oidc.session_ttl_secs, 8 * 3600);
    }

    #[test]
    fn oidc_account_chooser_can_be_turned_off_as_rollback_switch() {
        let (_dir, path) = write_oidc_config("[oidc]\naccount_chooser = false\n");
        let config = AppConfig::from_file(&path).unwrap();
        assert!(!config.oidc.account_chooser);
    }

    const ENABLED_OIDC: &str = "[oidc]\nenabled = true\nissuer = \"https://ctf.example.com\"\n\
        [[oidc.signing_keys]]\nkid = \"a\"\nprivate_key_file = \"keys/a.pem\"\nactive = true\n";

    #[test]
    fn unified_identity_switches_default_off_for_staged_rollout() {
        let (_dir, path) = write_oidc_config(ENABLED_OIDC);
        let oidc = AppConfig::from_file(&path).unwrap().oidc;
        assert!(!oidc.multi_account);
        assert!(!oidc.unified_web_session);
        assert_eq!(oidc.max_browser_accounts, 5);
        assert_eq!(oidc.browser_container_ttl_secs, 30 * 24 * 3600);
        assert!(oidc.first_party_origins.is_empty());
        assert!(!oidc.backchannel_logout);
        assert!(oidc.backchannel_trusted_hosts.is_empty());
        assert_eq!(oidc.backchannel_timeout_secs, 5);
        assert_eq!(oidc.logout_token_ttl_secs, 120);
        assert!(oidc.is_first_party_origin("https://ctf.example.com"));
        assert!(!oidc.is_first_party_origin("https://evil.example.com"));
    }

    #[test]
    fn unified_web_session_requires_browser_account_container() {
        // TOML 中 [[oidc.signing_keys]] 之后的键会落进数组表，因此开关必须写在 key 之前。
        let toml = |body: &str| {
            format!(
                "[oidc]\nenabled = true\nissuer = \"https://ctf.example.com\"\n{body}\n\
                 [[oidc.signing_keys]]\nkid = \"a\"\nprivate_key_file = \"keys/a.pem\"\nactive = true\n"
            )
        };
        let (_d, p) = write_oidc_config(&toml("unified_web_session = true"));
        assert!(
            AppConfig::from_file(&p)
                .unwrap_err()
                .to_string()
                .contains("multi_account")
        );
        let (_d, p) = write_oidc_config(&toml("unified_web_session = true\nmulti_account = true"));
        let oidc = AppConfig::from_file(&p).unwrap().oidc;
        assert!(oidc.unified_web_session && oidc.multi_account);

        // 第一方 origin 必须是规范 origin 且与 issuer 同主机（端口可以不同）。
        let (_d, p) = write_oidc_config(&toml(
            "multi_account = true\nfirst_party_origins = [\"https://ctf.example.com:8443\"]",
        ));
        let oidc = AppConfig::from_file(&p).unwrap().oidc;
        assert!(oidc.is_first_party_origin("https://ctf.example.com:8443"));
        for bad in [
            "https://other.example.com",
            "https://ctf.example.com/",
            "http://ctf.example.com",
        ] {
            let (_d, p) = write_oidc_config(&toml(&format!("first_party_origins = [\"{bad}\"]")));
            assert!(AppConfig::from_file(&p).is_err(), "{bad} must be rejected");
        }
        let (_d, p) = write_oidc_config(&toml("multi_account = true\nmax_browser_accounts = 0"));
        assert!(AppConfig::from_file(&p).is_err());

        // Back-Channel 内网白名单必须是精确的 host:port。
        let (_d, p) = write_oidc_config(&toml(
            "backchannel_logout = true\nbackchannel_trusted_hosts = [\"Learn.Example.internal:8080\"]",
        ));
        let oidc = AppConfig::from_file(&p).unwrap().oidc;
        assert_eq!(
            oidc.backchannel_trusted_hosts,
            vec!["learn.example.internal:8080"]
        );
        for bad in ["learn.example.internal", "10.0.0.1:0", ":8080", "*:443"] {
            let (_d, p) =
                write_oidc_config(&toml(&format!("backchannel_trusted_hosts = [\"{bad}\"]")));
            assert!(AppConfig::from_file(&p).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn oidc_enabled_requires_valid_issuer_and_exactly_one_active_key() {
        let key = |active: bool, kid: &str| {
            format!(
                "[[oidc.signing_keys]]\nkid = \"{kid}\"\nprivate_key_file = \"keys/{kid}.pem\"\nactive = {active}\n"
            )
        };
        // 无 key → 拒绝（不临时生成）
        let (_d, p) =
            write_oidc_config("[oidc]\nenabled = true\nissuer = \"https://ctf.example.com\"\n");
        assert!(
            AppConfig::from_file(&p)
                .unwrap_err()
                .to_string()
                .contains("signing_keys")
        );
        // 两把 active → 拒绝
        let (_d, p) = write_oidc_config(&format!(
            "[oidc]\nenabled = true\nissuer = \"https://ctf.example.com\"\n{}{}",
            key(true, "a"),
            key(true, "b")
        ));
        assert!(
            AppConfig::from_file(&p)
                .unwrap_err()
                .to_string()
                .contains("active")
        );
        // kid 重复 → 拒绝
        let (_d, p) = write_oidc_config(&format!(
            "[oidc]\nenabled = true\nissuer = \"https://ctf.example.com\"\n{}{}",
            key(true, "a"),
            key(false, "a")
        ));
        assert!(
            AppConfig::from_file(&p)
                .unwrap_err()
                .to_string()
                .contains("重复")
        );
        // 合法：相对路径以配置文件目录为基准
        let (dir, p) = write_oidc_config(&format!(
            "[oidc]\nenabled = true\nissuer = \"https://ctf.example.com\"\n{}{}",
            key(true, "a"),
            key(false, "b")
        ));
        let config = AppConfig::from_file(&p).unwrap();
        assert_eq!(config.oidc.signing_keys.len(), 2);
        assert_eq!(
            config.oidc.signing_keys[0].private_key_file,
            dir.path().join("keys/a.pem")
        );
        // 授权码 TTL 超过 120s → 拒绝
        let (_d, p) = write_oidc_config(&format!(
            "[oidc]\nenabled = true\nissuer = \"https://ctf.example.com\"\nauthorization_code_ttl_secs = 600\n{}",
            key(true, "a")
        ));
        assert!(
            AppConfig::from_file(&p)
                .unwrap_err()
                .to_string()
                .contains("authorization_code_ttl_secs")
        );
    }

    #[test]
    fn oidc_issuer_rules() {
        for ok in [
            "https://ctf.example.com",
            "https://ctf.example.com:8443",
            "http://localhost:7780",
            "http://127.0.0.1:9090",
        ] {
            assert!(validate_oidc_issuer(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://ctf.example.com",
            "https://ctf.example.com/",
            "https://ctf.example.com/idp",
            "https://ctf.example.com?x=1",
            "https://ctf.example.com#f",
            "https://user@ctf.example.com",
            "https://CTF.example.com",
            "https://ctf.example.com:443",
            "ftp://ctf.example.com",
            "ctf.example.com",
            "",
        ] {
            assert!(validate_oidc_issuer(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn oidc_disabled_does_not_validate_issuer() {
        // 关闭时保留旧部署零配置启动能力：错误的占位 issuer 不阻断启动。
        let (_d, p) = write_oidc_config("[oidc]\nenabled = false\nissuer = \"not a url\"\n");
        assert!(AppConfig::from_file(&p).is_ok());
    }
}
