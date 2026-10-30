//! OpenID Connect Back-Channel Logout 1.0（OP 侧）：Logout Token、投递 outbox、重试与出站安全。
//!
//! 只在用户**明确**选择「退出所有关联应用」（主站按钮或 RP 发起的 end_session 经 OP 确认页确认）时调用：
//! 1. [`enqueue`] 按被撤销的身份会话（sid）找出拿到过它令牌、且登记了 `backchannel_logout_uri` 的 active 应用，
//!    每个 (应用, sid) 写一行 outbox（`oidc_backchannel_deliveries`）；
//! 2. [`deliver_batch`] 立即并发投递（有总超时），把每个应用的真实结果返回给页面；
//! 3. 失败的行由 [`spawn_worker`] 按指数退避重试（多实例经 `FOR UPDATE SKIP LOCKED` 领取，只由同 issuer 实例投递）。
//!
//! 出站安全（防 SSRF / 内网探测）：
//! - 登记期 `clients::validate_backchannel_uri` 只接受 https 绝对 URL；
//! - 投递期自建 DNS 解析器只放行**公网**地址；IP 字面量同样校验；解析结果直接用于连接（防 DNS 重绑定）；
//! - 内网 RP 只能经运维 TOML `backchannel_trusted_hosts`（精确 `host:port`）显式放行，应用自己无法声明；
//! - 禁止重定向、不走代理、连接/总超时、只读取状态码。

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, DbBackend, EntityTrait,
    QueryFilter, Statement, sea_query::Expr,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::AppError;
use crate::core::config::OidcConfig;
use crate::entity::sea_orm_active_enums::OidcClientStatus;
use crate::entity::{oidc_backchannel_deliveries, oidc_clients, oidc_session_clients};

use super::OidcProvider;
use super::domain::secret;
use super::flow::SessionUser;

/// Logout Token 事件类型（§2.4）。
pub const BACKCHANNEL_EVENT: &str = "http://schemas.openid.net/event/backchannel-logout";
/// Logout Token 的 JWS `typ`。
pub const LOGOUT_TOKEN_TYP: &str = "logout+jwt";
/// 最多尝试次数（含首次）。
pub const MAX_ATTEMPTS: i32 = 6;
/// 第 n 次失败后的等待（秒）。
const BACKOFF_SECS: [i64; 5] = [30, 120, 600, 1800, 7200];
/// 领取后的租约：期间其他实例/worker 不会重复投递同一行。
const LEASE_SECS: i64 = 120;

/// Logout Token claims（Back-Channel Logout 1.0 §2.4）。**禁止** `nonce`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogoutTokenClaims {
    pub iss: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
    pub jti: String,
    pub events: serde_json::Value,
    pub sid: String,
    pub sub: String,
}

impl LogoutTokenClaims {
    pub fn new(issuer: &str, client_id: &str, sub: &str, sid: &str, ttl_secs: i64) -> Self {
        let now = Utc::now().timestamp();
        Self {
            iss: issuer.to_string(),
            aud: client_id.to_string(),
            iat: now,
            exp: now + ttl_secs,
            jti: secret::random_token(24),
            events: serde_json::json!({ BACKCHANNEL_EVENT: {} }),
            sid: sid.to_string(),
            sub: sub.to_string(),
        }
    }
}

/// 触发来源（与表约束一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    MainSite,
    EndSession,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Self::MainSite => "main_site",
            Self::EndSession => "end_session",
        }
    }
}

// ── 出站客户端 ──────────────────────────────────────────────────────────────

/// 公网地址判定（std 的 `is_global` 尚未稳定，按 IANA 特殊用途地址表实现）。
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let seg = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00 // fc00::/7 唯一本地
                || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10 链路本地
                || (seg[0] == 0x2001 && seg[1] == 0x0db8) // 文档
                || (seg[0] == 0x0064 && seg[1] == 0xff9b) // NAT64
                || seg[0] == 0x2002 // 6to4
                || (seg[0] == 0x2001 && seg[1] < 0x0200)) // 2001::/23 IETF 协议分配（含 Teredo）
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64/10 运营商级 NAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0/24
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18/15 基准测试
        || o[0] >= 240) // 240/4 保留
}

/// 只把**公网**地址交给连接器的 DNS 解析器（解析结果即连接目标，不存在二次解析的重绑定窗口）。
struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|a| is_public_ip(a.ip()))
                .collect();
            if addrs.is_empty() {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "目标解析到非公网地址（未在 backchannel_trusted_hosts 中放行）",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Back-Channel 出站客户端（每个 OP 进程一个）。
pub struct Sender {
    strict: Option<reqwest::Client>,
    trusted: Option<reqwest::Client>,
    trusted_hosts: HashSet<String>,
    unavailable: Option<String>,
}

impl Sender {
    /// 由配置构造；CA 文件不可读等错误返回 `Err`（bootstrap 据此拒绝启动）。
    pub fn from_config(config: &OidcConfig) -> Result<Self, String> {
        let timeout = StdDuration::from_secs(config.backchannel_timeout_secs);
        let mut roots = Vec::new();
        if let Some(path) = &config.backchannel_extra_ca_file {
            let pem = std::fs::read(path).map_err(|e| {
                format!(
                    "读取 backchannel_extra_ca_file {} 失败：{e}",
                    path.display()
                )
            })?;
            let cert = reqwest::Certificate::from_pem(&pem)
                .map_err(|e| format!("backchannel_extra_ca_file 不是合法 PEM 证书：{e}"))?;
            roots.push(cert);
        }
        let build = |public_only: bool| {
            let mut builder = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .connect_timeout(timeout)
                .timeout(timeout)
                .user_agent("FloatCTF-OIDC-Backchannel/1");
            for cert in &roots {
                builder = builder.add_root_certificate(cert.clone());
            }
            if public_only {
                builder = builder.dns_resolver(Arc::new(PublicOnlyResolver));
            }
            builder
                .build()
                .map_err(|e| format!("构造 Back-Channel HTTP 客户端失败：{e}"))
        };
        Ok(Self {
            strict: Some(build(true)?),
            trusted: Some(build(false)?),
            trusted_hosts: config.backchannel_trusted_hosts.iter().cloned().collect(),
            unavailable: None,
        })
    }

    /// 无法构造客户端时的占位：所有投递都失败并记录原因（不会悄悄跳过）。
    pub fn unavailable(reason: String) -> Self {
        Self {
            strict: None,
            trusted: None,
            trusted_hosts: HashSet::new(),
            unavailable: Some(reason),
        }
    }

    /// 选择客户端并做投递前的地址策略检查；返回 `Err(原因)` 表示不允许投递（不重试）。
    fn client_for(&self, url: &url::Url) -> Result<&reqwest::Client, String> {
        if let Some(reason) = &self.unavailable {
            return Err(reason.clone());
        }
        let host = url.host_str().ok_or("目标缺少主机名")?.to_ascii_lowercase();
        let port = url.port_or_known_default().ok_or("目标缺少端口")?;
        let host_port = format!(
            "{}:{port}",
            host.trim_start_matches('[').trim_end_matches(']')
        );
        let bracketed = format!("{host}:{port}");
        if self.trusted_hosts.contains(&host_port) || self.trusted_hosts.contains(&bracketed) {
            return self.trusted.as_ref().ok_or_else(|| "客户端不可用".into());
        }
        // IP 字面量不经过 DNS 解析器：在这里校验。
        let literal = host.trim_start_matches('[').trim_end_matches(']');
        if let Ok(ip) = literal.parse::<IpAddr>()
            && !is_public_ip(ip)
        {
            return Err("目标是非公网地址（未在 backchannel_trusted_hosts 中放行）".into());
        }
        if url.scheme() != "https" {
            return Err(
                "只向 https 地址投递（http loopback 需在 backchannel_trusted_hosts 中放行）".into(),
            );
        }
        self.strict.as_ref().ok_or_else(|| "客户端不可用".into())
    }

    /// POST `logout_token`（application/x-www-form-urlencoded）。返回 HTTP 状态码。
    pub async fn post(&self, target: &str, logout_token: &str) -> Result<u16, Attempt> {
        let url = url::Url::parse(target).map_err(|_| Attempt::Refused("登记的地址无效".into()))?;
        let client = self.client_for(&url).map_err(Attempt::Refused)?;
        let resp = client
            .post(url)
            .header("Cache-Control", "no-store")
            .form(&[("logout_token", logout_token)])
            .send()
            .await
            .map_err(|e| {
                // 解析器拒绝的地址属于策略拒绝，不重试。
                let text = describe_error(&e);
                if text.contains("非公网地址") {
                    Attempt::Refused(text)
                } else {
                    Attempt::Transient(text)
                }
            })?;
        Ok(resp.status().as_u16())
    }
}

fn describe_error(e: &reqwest::Error) -> String {
    use std::error::Error;
    let mut text = if e.is_timeout() {
        "请求超时".to_string()
    } else if e.is_connect() {
        "无法连接".to_string()
    } else {
        "请求失败".to_string()
    };
    let mut source = e.source();
    while let Some(inner) = source {
        text.push('：');
        text.push_str(&inner.to_string());
        source = inner.source();
    }
    text.chars().take(300).collect()
}

/// 一次投递尝试失败的分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attempt {
    /// 策略拒绝（非公网地址、http 等）：不重试。
    Refused(String),
    /// 网络/超时等暂时性错误：按退避重试。
    Transient(String),
}

// ── outbox ─────────────────────────────────────────────────────────────────

/// 某些身份会话关联的应用（确认页与结果页展示用）。
#[derive(Debug, Clone, Serialize)]
pub struct AffectedClient {
    pub client_id: String,
    pub name: String,
    /// 是否登记了 Back-Channel 接收地址（能否即时通知）。
    pub supports_backchannel: bool,
}

/// 列出这些会话拿到过令牌的应用（去重，名称排序）。
pub async fn affected_clients(
    db: &DatabaseConnection,
    session_ids: &[Uuid],
) -> Result<Vec<AffectedClient>, AppError> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = oidc_session_clients::Entity::find()
        .filter(oidc_session_clients::Column::SessionId.is_in(session_ids.to_vec()))
        .find_also_related(oidc_clients::Entity)
        .all(db)
        .await?;
    let mut seen = HashSet::new();
    let mut out: Vec<AffectedClient> = rows
        .into_iter()
        .filter_map(|(_, c)| c)
        .filter(|c| seen.insert(c.id))
        .map(|c| AffectedClient {
            supports_backchannel: c.backchannel_logout_uri.is_some()
                && c.status == OidcClientStatus::Active,
            client_id: c.client_id,
            name: c.name,
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 为被撤销的会话写入 outbox：每个 (登记了接收地址的 active 应用, sid) 一行。
///
/// 新行带短租约（`next_attempt_at = now + LEASE`），随后由调用方立即投递；worker 只接手失败或进程中断的行。
pub async fn enqueue(
    db: &DatabaseConnection,
    issuer: &str,
    sessions: &[SessionUser],
    reason: Reason,
) -> Result<(Uuid, Vec<oidc_backchannel_deliveries::Model>), AppError> {
    let batch_id = Uuid::new_v4();
    let mut rows = Vec::new();
    for (session, user) in sessions {
        let bindings = oidc_session_clients::Entity::find()
            .filter(oidc_session_clients::Column::SessionId.eq(session.id))
            .find_also_related(oidc_clients::Entity)
            .all(db)
            .await?;
        for (_, client) in bindings {
            let Some(client) = client else { continue };
            if client.backchannel_logout_uri.is_none() || client.status != OidcClientStatus::Active
            {
                continue;
            }
            let now = Utc::now().fixed_offset();
            let row = oidc_backchannel_deliveries::ActiveModel {
                id: Set(Uuid::new_v4()),
                batch_id: Set(batch_id),
                issuer: Set(issuer.to_string()),
                oidc_client_id: Set(client.id),
                user_id: Set(Some(user.id)),
                sub: Set(user.id.to_string()),
                sid: Set(session.sid.clone()),
                reason: Set(reason.as_str().to_string()),
                status: Set("pending".into()),
                attempts: Set(0),
                next_attempt_at: Set(now + Duration::seconds(LEASE_SECS)),
                last_error: Set(None),
                last_http_status: Set(None),
                last_jti: Set(None),
                created_at: Set(now),
                delivered_at: Set(None),
            }
            .insert(db)
            .await?;
            rows.push(row);
        }
    }
    Ok((batch_id, rows))
}

/// 投递结果（展示给用户；不含令牌）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryOutcome {
    pub client_id: String,
    pub client_name: String,
    /// `delivered` | `pending`（将自动重试）| `failed`
    pub status: String,
    pub detail: Option<String>,
}

/// 投递一行并落库结果。
pub async fn deliver_one(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    row: &oidc_backchannel_deliveries::Model,
) -> Result<DeliveryOutcome, AppError> {
    let client = oidc_clients::Entity::find_by_id(row.oidc_client_id)
        .one(db)
        .await?;
    let Some(client) = client else {
        mark(db, row, "failed", None, Some("应用已删除".into()), None).await?;
        return Ok(outcome_of(None, row, "failed", Some("应用已删除".into())));
    };
    let Some(target) = client.backchannel_logout_uri.clone() else {
        mark(
            db,
            row,
            "failed",
            None,
            Some("应用已移除接收地址".into()),
            None,
        )
        .await?;
        return Ok(outcome_of(
            Some(&client),
            row,
            "failed",
            Some("应用已移除接收地址".into()),
        ));
    };
    let claims = LogoutTokenClaims::new(
        &row.issuer,
        &client.client_id,
        &row.sub,
        &row.sid,
        oidc.config.logout_token_ttl_secs,
    );
    let token = match oidc.keys.sign_typed(&claims, LOGOUT_TOKEN_TYP) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "logout token signing failed");
            return retry_or_fail(db, row, &client, None, "签名失败".into(), None).await;
        }
    };
    match oidc.backchannel.post(&target, &token).await {
        Ok(status) if (200..300).contains(&status) => {
            mark(db, row, "delivered", Some(status), None, Some(claims.jti)).await?;
            tracing::info!(client_id = %client.client_id, sid = %row.sid, "back-channel logout delivered");
            Ok(outcome_of(Some(&client), row, "delivered", None))
        }
        Ok(400) => {
            // §2.8：RP 无法完成退出时返回 400——记录并放弃，由用户看到「未能确认」。
            let detail = "应用拒绝了退出通知（HTTP 400）".to_string();
            mark(
                db,
                row,
                "failed",
                Some(400),
                Some(detail.clone()),
                Some(claims.jti),
            )
            .await?;
            tracing::warn!(client_id = %client.client_id, sid = %row.sid, "back-channel logout rejected by RP");
            Ok(outcome_of(Some(&client), row, "failed", Some(detail)))
        }
        Ok(status) => {
            retry_or_fail(
                db,
                row,
                &client,
                Some(status),
                format!("HTTP {status}"),
                Some(claims.jti),
            )
            .await
        }
        Err(Attempt::Refused(reason)) => {
            mark(
                db,
                row,
                "failed",
                None,
                Some(reason.clone()),
                Some(claims.jti),
            )
            .await?;
            tracing::warn!(client_id = %client.client_id, reason = %reason, "back-channel target refused by policy");
            Ok(outcome_of(Some(&client), row, "failed", Some(reason)))
        }
        Err(Attempt::Transient(reason)) => {
            retry_or_fail(db, row, &client, None, reason, Some(claims.jti)).await
        }
    }
}

async fn retry_or_fail(
    db: &DatabaseConnection,
    row: &oidc_backchannel_deliveries::Model,
    client: &oidc_clients::Model,
    http_status: Option<u16>,
    reason: String,
    jti: Option<String>,
) -> Result<DeliveryOutcome, AppError> {
    let attempts = row.attempts + 1;
    if attempts >= MAX_ATTEMPTS {
        mark(db, row, "failed", http_status, Some(reason.clone()), jti).await?;
        tracing::warn!(client_id = %client.client_id, sid = %row.sid, reason = %reason, "back-channel logout gave up");
        return Ok(outcome_of(Some(client), row, "failed", Some(reason)));
    }
    let wait = BACKOFF_SECS[(attempts as usize - 1).min(BACKOFF_SECS.len() - 1)];
    oidc_backchannel_deliveries::Entity::update_many()
        .col_expr(
            oidc_backchannel_deliveries::Column::Attempts,
            Expr::value(attempts),
        )
        .col_expr(
            oidc_backchannel_deliveries::Column::NextAttemptAt,
            Expr::value(Utc::now().fixed_offset() + Duration::seconds(wait)),
        )
        .col_expr(
            oidc_backchannel_deliveries::Column::LastError,
            Expr::value(Some(reason.clone())),
        )
        .col_expr(
            oidc_backchannel_deliveries::Column::LastHttpStatus,
            Expr::value(http_status.map(i32::from)),
        )
        .col_expr(
            oidc_backchannel_deliveries::Column::LastJti,
            Expr::value(jti),
        )
        .filter(oidc_backchannel_deliveries::Column::Id.eq(row.id))
        .exec(db)
        .await?;
    tracing::warn!(client_id = %client.client_id, sid = %row.sid, attempts, retry_in_secs = wait, reason = %reason, "back-channel logout will retry");
    Ok(outcome_of(Some(client), row, "pending", Some(reason)))
}

async fn mark(
    db: &DatabaseConnection,
    row: &oidc_backchannel_deliveries::Model,
    status: &'static str,
    http_status: Option<u16>,
    error: Option<String>,
    jti: Option<String>,
) -> Result<(), AppError> {
    let now = Utc::now().fixed_offset();
    let mut update = oidc_backchannel_deliveries::Entity::update_many()
        .col_expr(
            oidc_backchannel_deliveries::Column::Status,
            Expr::value(status),
        )
        .col_expr(
            oidc_backchannel_deliveries::Column::Attempts,
            Expr::value(row.attempts + 1),
        )
        .col_expr(
            oidc_backchannel_deliveries::Column::LastError,
            Expr::value(error),
        )
        .col_expr(
            oidc_backchannel_deliveries::Column::LastHttpStatus,
            Expr::value(http_status.map(i32::from)),
        )
        .col_expr(
            oidc_backchannel_deliveries::Column::LastJti,
            Expr::value(jti),
        );
    if status == "delivered" {
        update = update.col_expr(
            oidc_backchannel_deliveries::Column::DeliveredAt,
            Expr::value(Some(now)),
        );
    }
    update
        .filter(oidc_backchannel_deliveries::Column::Id.eq(row.id))
        .exec(db)
        .await?;
    Ok(())
}

fn outcome_of(
    client: Option<&oidc_clients::Model>,
    row: &oidc_backchannel_deliveries::Model,
    status: &str,
    detail: Option<String>,
) -> DeliveryOutcome {
    DeliveryOutcome {
        client_id: client.map(|c| c.client_id.clone()).unwrap_or_default(),
        client_name: client
            .map(|c| c.name.clone())
            .unwrap_or_else(|| row.oidc_client_id.to_string()),
        status: status.to_string(),
        detail,
    }
}

/// 立即并发投递一批，总耗时不超过 `backchannel_timeout_secs + 1` 秒；超时未完成的行保持 pending 由 worker 接手。
pub async fn deliver_batch(
    db: &DatabaseConnection,
    oidc: &OidcProvider,
    rows: &[oidc_backchannel_deliveries::Model],
) -> Vec<DeliveryOutcome> {
    let deadline = StdDuration::from_secs(oidc.config.backchannel_timeout_secs + 1);
    let futures = rows.iter().map(|row| async move {
        match tokio::time::timeout(deadline, deliver_one(db, oidc, row)).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(e)) => {
                tracing::error!(error = %e, "back-channel delivery bookkeeping failed");
                DeliveryOutcome {
                    client_id: String::new(),
                    client_name: row.oidc_client_id.to_string(),
                    status: "pending".into(),
                    detail: Some("记录投递结果失败，将重试".into()),
                }
            }
            Err(_) => DeliveryOutcome {
                client_id: String::new(),
                client_name: row.oidc_client_id.to_string(),
                status: "pending".into(),
                detail: Some("请求超时，将自动重试".into()),
            },
        }
    });
    futures_util::future::join_all(futures).await
}

/// 领取到期的待重试行（本 issuer），并加租约。多实例并发安全。
pub async fn claim_due(
    db: &DatabaseConnection,
    issuer: &str,
    limit: u64,
) -> Result<Vec<oidc_backchannel_deliveries::Model>, AppError> {
    let sql = format!(
        "UPDATE oidc_backchannel_deliveries SET next_attempt_at = now() + interval '{LEASE_SECS} seconds' \
         WHERE id IN (SELECT id FROM oidc_backchannel_deliveries \
                      WHERE status = 'pending' AND issuer = $1 AND next_attempt_at <= now() \
                      ORDER BY next_attempt_at LIMIT {limit} FOR UPDATE SKIP LOCKED) \
         RETURNING *"
    );
    Ok(oidc_backchannel_deliveries::Entity::find()
        .from_raw_sql(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [issuer.into()],
        ))
        .all(db)
        .await?)
}

/// 重试一轮：返回处理的行数。
pub async fn retry_due(db: &DatabaseConnection, oidc: &OidcProvider) -> Result<usize, AppError> {
    let rows = claim_due(db, oidc.issuer(), 20).await?;
    let n = rows.len();
    if n > 0 {
        deliver_batch(db, oidc, &rows).await;
    }
    Ok(n)
}

/// 后台重试循环（每 30 秒）；只在 `[oidc].backchannel_logout = true` 时启动。
pub fn spawn_worker(db: DatabaseConnection, oidc: Arc<OidcProvider>) {
    actix_web::rt::spawn(async move {
        let mut ticker = tokio::time::interval(StdDuration::from_secs(30));
        loop {
            ticker.tick().await;
            if let Err(e) = retry_due(&db, &oidc).await {
                tracing::error!(error = %e, "back-channel logout retry round failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logout_token_claims_follow_the_spec() {
        let c = LogoutTokenClaims::new("https://op.example", "client-1", "user-1", "sid-1", 120);
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["iss"], "https://op.example");
        assert_eq!(v["aud"], "client-1");
        assert_eq!(v["sid"], "sid-1");
        assert_eq!(v["sub"], "user-1");
        assert_eq!(v["exp"].as_i64().unwrap() - v["iat"].as_i64().unwrap(), 120);
        assert!(v["events"][BACKCHANNEL_EVENT].is_object());
        assert!(v.get("nonce").is_none(), "Logout Token 禁止 nonce");
        let other =
            LogoutTokenClaims::new("https://op.example", "client-1", "user-1", "sid-1", 120);
        assert_ne!(c.jti, other.jti, "每次签发唯一 jti");
    }

    #[test]
    fn only_public_addresses_are_reachable_by_default() {
        for private in [
            "127.0.0.1",
            "10.66.66.7",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "198.18.0.1",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(private.parse().unwrap()), "{private}");
        }
        for public in ["1.1.1.1", "120.95.81.151", "2606:4700:4700::1111"] {
            assert!(is_public_ip(public.parse().unwrap()), "{public}");
        }
    }

    fn sender(trusted: &[&str]) -> Sender {
        let mut config = OidcConfig::disabled_defaults();
        config.backchannel_trusted_hosts = trusted.iter().map(|s| s.to_string()).collect();
        Sender::from_config(&config).unwrap()
    }

    #[test]
    fn delivery_policy_refuses_internal_targets_unless_explicitly_trusted() {
        let s = sender(&["10.66.66.7:8080"]);
        let refused = |u: &str| s.client_for(&url::Url::parse(u).unwrap()).is_err();
        assert!(refused("https://127.0.0.1/bc"));
        assert!(refused("https://169.254.169.254/latest/meta-data"));
        assert!(refused("https://[::1]:8443/bc"));
        assert!(
            refused("https://10.66.66.7:8443/bc"),
            "白名单是精确 host:port"
        );
        assert!(
            refused("http://learn.example.com/bc"),
            "非白名单只允许 https"
        );
        assert!(!refused(
            "https://10.66.66.7:8080/api/oidc/backchannel-logout"
        ));
        assert!(!refused(
            "https://learn.example.com/api/oidc/backchannel-logout"
        ));
        let dead = Sender::unavailable("CA 文件缺失".into());
        assert!(
            dead.client_for(&url::Url::parse("https://learn.example.com/").unwrap())
                .is_err()
        );
    }
}
