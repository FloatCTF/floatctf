use std::{
    collections::HashMap,
    process::Stdio,
    sync::Mutex,
    time::{Duration, Instant},
};

use actix_web::{
    HttpRequest, HttpResponse,
    cookie::{Cookie, SameSite, time::Duration as CookieDuration},
    web,
};
use actix_ws::Message;
use base64::Engine;
use futures_util::StreamExt;
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::api::prelude::*;
use crate::entity::super_admin;

const TERMINAL_TICKET_COOKIE: &str = "floatctf_terminal_ticket";
/// 生产 TTL：60 秒内必须完成 session → WS 握手。
const TERMINAL_TICKET_TTL: Duration = Duration::from_secs(60);

fn terminal_ticket_hash(ticket: &str) -> [u8; 32] {
    Sha256::digest(ticket.as_bytes()).into()
}

fn terminal_ticket_random() -> String {
    // 32 字节 CSPRNG（≥256bit 熵），base64url 编码后作为 Cookie 值。
    let mut random = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut random);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random)
}

/// Terminal 一次性 ticket 存储。
///
/// 正常 API 始终使用 Redis（`SET NX EX` 签发 + `GETDEL` 原子消费），多个 API
/// 节点共享同一份 ticket；Redis 故障时 fail-closed，避免 session 与 WS 落到不同
/// 节点时产生不一致。进程内 HashMap 仅通过显式测试构造器使用。
///
/// 两种测试/生产后端都只存 `SHA-256(ticket)`，明文 ticket 只出现在 HttpOnly Cookie 中。
pub struct TerminalTicketStore {
    ttl: Duration,
    backend: TerminalTicketBackend,
}

enum TerminalTicketBackend {
    Redis(redis::Client),
    Memory(Mutex<HashMap<[u8; 32], TerminalTicketEntry>>),
}

#[derive(Debug)]
struct TerminalTicketEntry {
    admin_id: Uuid,
    expires_at: Instant,
}

#[derive(Debug, thiserror::Error)]
#[error("terminal ticket store unavailable: {0}")]
pub struct TerminalTicketStoreError(String);

impl TerminalTicketStore {
    /// 生产构造器：Redis 是必需后端。
    pub fn new(ttl: Duration, redis: redis::Client) -> Self {
        Self {
            ttl,
            backend: TerminalTicketBackend::Redis(redis),
        }
    }

    /// 仅供隔离测试使用；平台 bootstrap 从不调用这个构造器。
    #[doc(hidden)]
    pub fn in_memory_for_tests(ttl: Duration) -> Self {
        Self {
            ttl,
            backend: TerminalTicketBackend::Memory(Mutex::new(HashMap::new())),
        }
    }

    fn redis_key(digest: &[u8; 32]) -> String {
        format!(
            "floatctf:terminal-ticket:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
        )
    }

    /// 签发一次性 ticket；返回值即 Cookie 明文，存储层只见其 SHA-256。
    pub async fn issue(&self, admin_id: Uuid) -> Result<String, TerminalTicketStoreError> {
        let ticket = terminal_ticket_random();
        let digest = terminal_ticket_hash(&ticket);

        match &self.backend {
            TerminalTicketBackend::Redis(client) => {
                use redis::AsyncCommands;
                let mut conn = client
                    .get_multiplexed_async_connection()
                    .await
                    .map_err(|e| TerminalTicketStoreError(e.to_string()))?;
                let options = redis::SetOptions::default()
                    .conditional_set(redis::ExistenceCheck::NX)
                    .with_expiration(redis::SetExpiry::EX(self.ttl.as_secs().max(1)));
                let key = Self::redis_key(&digest);
                conn.set_options::<_, _, ()>(key, admin_id.to_string(), options)
                    .await
                    .map_err(|e| TerminalTicketStoreError(e.to_string()))?;
            }
            TerminalTicketBackend::Memory(local) => {
                let now = Instant::now();
                let mut tickets = local.lock().expect("terminal ticket lock poisoned");
                tickets.retain(|_, entry| entry.expires_at > now);
                tickets.insert(
                    digest,
                    TerminalTicketEntry {
                        admin_id,
                        expires_at: now + self.ttl,
                    },
                );
            }
        }
        Ok(ticket)
    }

    /// 原子消费一次性 ticket：有效返回 admin_id，无效/过期/存储故障返回 `None`。
    pub async fn consume(&self, ticket: &str) -> Option<Uuid> {
        if ticket.is_empty() {
            return None;
        }
        let digest = terminal_ticket_hash(ticket);

        match &self.backend {
            TerminalTicketBackend::Redis(client) => {
                use redis::AsyncCommands;
                let mut conn = client.get_multiplexed_async_connection().await.ok()?;
                let key = Self::redis_key(&digest);
                // GETDEL 原子取出并删除：同一 ticket 的并发第二次握手必然 miss。
                let raw: Option<String> = conn.get_del(key).await.ok()?;
                raw.as_deref().and_then(|s| Uuid::parse_str(s).ok())
            }
            TerminalTicketBackend::Memory(local) => {
                let now = Instant::now();
                let mut tickets = local.lock().expect("terminal ticket lock poisoned");
                tickets.retain(|_, entry| entry.expires_at > now);
                tickets.remove(&digest).map(|entry| entry.admin_id)
            }
        }
    }

    #[cfg(test)]
    fn in_memory_digest_keys(&self) -> Vec<[u8; 32]> {
        match &self.backend {
            TerminalTicketBackend::Memory(local) => local
                .lock()
                .expect("terminal ticket lock poisoned")
                .keys()
                .copied()
                .collect(),
            TerminalTicketBackend::Redis(_) => Vec::new(),
        }
    }
}

/// POST /api/admin/terminal/session
/// 使用常规 SuperAdmin Authorization 签发 60 秒、单次使用的 HttpOnly ticket cookie。
#[post("/session")]
pub async fn create_terminal_session(
    admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
) -> Result<HttpResponse, actix_web::Error> {
    if !ctx.config.features.enable_web_terminal {
        return Err(actix_web::error::ErrorNotFound("Web terminal is disabled"));
    }

    let admin = admin.into_inner();
    let ticket = ctx
        .req
        .app_data::<web::Data<crate::bootstrap::AppState>>()
        .expect("AppState not found")
        .terminal_tickets
        .issue(admin.id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let secure = ctx.config.main_url.starts_with("https://")
        || ctx.req.connection_info().scheme() == "https";
    let cookie = Cookie::build(TERMINAL_TICKET_COOKIE, ticket)
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Strict)
        .path("/api/admin/terminal/ws")
        .max_age(CookieDuration::seconds(TERMINAL_TICKET_TTL.as_secs() as i64))
        .finish();

    ctx.log
        .add_log(
            "INFO",
            "TERMINAL",
            "ISSUE_TICKET",
            "Issued one-time web terminal ticket",
            json!({"ttl_secs": TERMINAL_TICKET_TTL.as_secs()}),
            None,
            Some(admin.id),
            Some(&ctx.req),
        )
        .await;

    Ok(HttpResponse::NoContent().cookie(cookie).finish())
}

/// GET /api/admin/terminal/ws
#[get("/ws")]
pub async fn terminal_ws(
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let state = req
        .app_data::<actix_web::web::Data<crate::bootstrap::AppState>>()
        .expect("AppState not found");
    if !state.config.features.enable_web_terminal {
        return Err(actix_web::error::ErrorNotFound("Web terminal is disabled"));
    }

    let ticket = req
        .cookie(TERMINAL_TICKET_COOKIE)
        .map(|cookie| cookie.value().to_string())
        .unwrap_or_default();
    let admin_id = state
        .terminal_tickets
        .consume(&ticket)
        .await
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Invalid or expired terminal ticket"))?;

    let db = req.app_data::<WebDb>().cloned().expect("WebDb not found");
    let _admin = super_admin::Entity::find_by_id(admin_id)
        .one(db.get_ref())
        .await
        .map_err(|_| actix_web::error::ErrorInternalServerError("DB error"))?
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Not a super admin"))?;

    // Upgrade to WebSocket only after the one-time ticket has been atomically consumed.
    let (res, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    // Find best available shell: fish > zsh > bash > sh
    let (shell, login_arg) = find_shell();

    // Spawn shell inside script for PTY support
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("script");
        c.arg("-q").arg("/dev/null").arg(shell);
        if let Some(flag) = login_arg {
            c.arg(flag);
        }
        c
    } else {
        let mut c = Command::new("script");
        let shell_cmd = if let Some(flag) = login_arg {
            format!("{shell} {flag}")
        } else {
            shell.to_string()
        };
        c.arg("-q").arg("-c").arg(shell_cmd).arg("/dev/null");
        c
    };

    // Set TERM so programs know how to render
    cmd.env("TERM", "xterm-256color");
    // Disable pagination
    cmd.env("PAGER", "cat");
    cmd.env("MANPAGER", "cat");
    // Merge stderr into stdout for simplicity
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = session
                .text(format!("Failed to spawn shell: {e}\r\n"))
                .await;
            let _ = session.close(None).await;
            return Ok(res);
        }
    };

    let mut child_stdin = child.stdin.take().unwrap();
    let child_stdout = child.stdout.take().unwrap();
    let child_stderr = child.stderr.take().unwrap();

    // Read stdout and forward to WebSocket
    let mut session_out = session.clone();
    let stdout_handle = actix_web::rt::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut reader = tokio::io::BufReader::new(child_stdout);
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let data = Vec::from(&buf[..n]);
                    if session_out.binary(data).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Read stderr and forward to WebSocket
    let mut session_err = session.clone();
    let stderr_handle = actix_web::rt::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut reader = tokio::io::BufReader::new(child_stderr);
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let data = Vec::from(&buf[..n]);
                    if session_err.binary(data).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Clone session for the msg processing task (needed for pong)
    let mut session_msg = session.clone();

    // Process incoming WebSocket messages
    actix_web::rt::spawn(async move {
        while let Some(Ok(msg)) = msg_stream.next().await {
            match msg {
                Message::Text(text) => {
                    // Check if this is a resize message
                    if let Ok(ctrl) = serde_json::from_str::<TerminalControl>(&text) {
                        if ctrl.r#type == "resize" {
                            let rows = ctrl.rows.unwrap_or(24);
                            let cols = ctrl.cols.unwrap_or(80);
                            // Clear screen before stty so the echoed command is wiped
                            let cmd = format!(
                                "printf '\\033[2J\\033[H'; stty rows {} cols {} 2>/dev/null\r",
                                rows, cols
                            );
                            let _ = child_stdin.write_all(cmd.as_bytes()).await;
                            continue;
                        }
                    }
                    // Regular text input - forward to bash stdin
                    let _ = child_stdin.write_all(text.as_bytes()).await;
                }
                Message::Binary(bin) => {
                    let _ = child_stdin.write_all(&bin).await;
                }
                Message::Ping(bytes) => {
                    let _ = session_msg.pong(&bytes).await;
                }
                Message::Close(_) => {
                    break;
                }
                _ => {}
            }
        }

        // WebSocket closed, terminate the child process
        child.kill().await.ok();
    });

    // Cleanup: when stdout or stderr readers end, close the session
    actix_web::rt::spawn(async move {
        let _ = tokio::join!(stdout_handle, stderr_handle);
        let _ = session.close(None).await;
    });

    Ok(res)
}

/// 按顺序尝试 shell：fish > zsh > bash > sh。
/// 返回 (shell_name, optional_login_flag)。
fn find_shell() -> (&'static str, Option<&'static str>) {
    let candidates: &[(&str, Option<&str>)] = &[
        ("fish", Some("--login")),
        ("zsh", Some("--login")),
        ("bash", Some("--login")),
        ("sh", None),
    ];
    for &(shell, login) in candidates {
        if std::process::Command::new(shell)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return (shell, login);
        }
    }
    ("sh", None)
}

#[derive(Debug, Deserialize)]
struct TerminalControl {
    #[serde(rename = "type")]
    r#type: String,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}

#[cfg(test)]
mod ticket_tests {
    use super::*;
    use std::collections::HashSet;

    fn store(ttl: Duration) -> TerminalTicketStore {
        TerminalTicketStore::in_memory_for_tests(ttl)
    }

    #[tokio::test]
    async fn local_ticket_single_use() {
        let s = store(Duration::from_secs(60));
        let admin = Uuid::new_v4();
        let ticket = s.issue(admin).await.expect("issue");
        assert_eq!(s.consume(&ticket).await, Some(admin), "首次消费应成功");
        assert_eq!(s.consume(&ticket).await, None, "二次消费必须拒绝");
    }

    #[tokio::test]
    async fn local_ticket_expired() {
        // 注入短 TTL，无需等待生产 60 秒。
        let s = store(Duration::from_millis(30));
        let admin = Uuid::new_v4();
        let ticket = s.issue(admin).await.expect("issue");
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(s.consume(&ticket).await, None, "过期 ticket 必须拒绝");
    }

    #[tokio::test]
    async fn local_ticket_empty_rejected() {
        let s = store(Duration::from_secs(60));
        assert_eq!(s.consume("").await, None);
    }

    #[tokio::test]
    async fn tickets_are_unique_and_256bit() {
        // 数千 ticket 不得重复；base64url 解码后必须恰好 32 字节（256bit CSPRNG）。
        let s = store(Duration::from_secs(60));
        let mut seen = HashSet::new();
        for _ in 0..5000 {
            let ticket = s.issue(Uuid::new_v4()).await.expect("issue");
            assert_eq!(ticket.len(), 43, "32 字节 base64url-no-pad 长度应为 43");
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(ticket.as_bytes())
                .expect("base64url decode");
            assert_eq!(bytes.len(), 32, "必须为 256bit 随机值");
            assert!(seen.insert(ticket), "ticket 不得重复");
        }
        assert_eq!(seen.len(), 5000);
    }

    #[tokio::test]
    async fn local_store_keeps_only_hash_digest() {
        // 进程内存储的 key 是 SHA-256 摘要：明文 ticket 不落存储。
        let s = store(Duration::from_secs(60));
        let ticket = s.issue(Uuid::new_v4()).await.expect("issue");
        let keys = s.in_memory_digest_keys();
        assert_eq!(keys.len(), 1);
        let key = keys[0];
        assert_ne!(
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key),
            ticket,
            "存储键不得是明文 ticket"
        );
        assert_eq!(key, terminal_ticket_hash(&ticket));
    }

    mod redis_tests {
        use super::*;

        /// FLUSHDB 是全库操作：Redis 测试必须串行。
        static REDIS_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

        fn redis_url() -> Option<String> {
            match std::env::var("TEST_REDIS_URL") {
                Ok(url) if !url.trim().is_empty() => Some(url),
                _ => {
                    eprintln!("skip: TEST_REDIS_URL not set (terminal ticket redis tests)");
                    None
                }
            }
        }

        async fn flush(client: &redis::Client) {
            let mut conn = client
                .get_multiplexed_async_connection()
                .await
                .expect("redis conn");
            let _: () = redis::cmd("FLUSHDB")
                .query_async(&mut conn)
                .await
                .expect("flush");
        }

        /// 多 API 节点共享：节点 A 签发、节点 B 消费。
        #[tokio::test]
        async fn redis_ticket_shared_across_nodes() {
            let _serial = REDIS_SERIAL.lock().unwrap();
            let Some(url) = redis_url() else { return };
            let client = redis::Client::open(url.as_str()).expect("client");
            let node_a = TerminalTicketStore::new(Duration::from_secs(60), client.clone());
            let node_b = TerminalTicketStore::new(Duration::from_secs(60), client.clone());
            flush(&client).await;

            let admin = Uuid::new_v4();
            let ticket = node_a.issue(admin).await.expect("issue on node A");
            assert_eq!(
                node_b.consume(&ticket).await,
                Some(admin),
                "节点 B 应能消费节点 A 签发的 ticket"
            );
            assert_eq!(node_a.consume(&ticket).await, None, "跨节点二次消费拒绝");
        }

        /// TTL 由 Redis EX 保证。
        #[tokio::test]
        async fn redis_ticket_ttl_expiry() {
            let _serial = REDIS_SERIAL.lock().unwrap();
            let Some(url) = redis_url() else { return };
            let client = redis::Client::open(url.as_str()).expect("client");
            let s = TerminalTicketStore::new(Duration::from_secs(1), client.clone());
            flush(&client).await;

            let ticket = s.issue(Uuid::new_v4()).await.expect("issue");
            tokio::time::sleep(Duration::from_millis(1300)).await;
            assert_eq!(s.consume(&ticket).await, None, "过期 ticket 必须拒绝");
        }

        /// Redis 里只存 SHA-256 摘要 key + admin_id 值，不含明文 ticket。
        #[tokio::test]
        async fn redis_stores_hash_not_plaintext() {
            let _serial = REDIS_SERIAL.lock().unwrap();
            let Some(url) = redis_url() else { return };
            let client = redis::Client::open(url.as_str()).expect("client");
            let s = TerminalTicketStore::new(Duration::from_secs(60), client.clone());
            flush(&client).await;

            let ticket = s.issue(Uuid::new_v4()).await.expect("issue");
            let mut conn = client
                .get_multiplexed_async_connection()
                .await
                .expect("conn");
            let keys: Vec<String> = redis::cmd("KEYS")
                .arg("floatctf:terminal-ticket:*")
                .query_async(&mut conn)
                .await
                .expect("keys");
            assert_eq!(keys.len(), 1, "应恰好一个 ticket key");
            assert!(!keys[0].contains(&ticket), "key 不得包含明文 ticket");
            let digest_b64 = keys[0].trim_start_matches("floatctf:terminal-ticket:");
            assert_eq!(
                digest_b64,
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(terminal_ticket_hash(&ticket)),
                "key 必须是 ticket 的 SHA-256 摘要"
            );
        }

        /// Redis 不可达 → fail closed：签发报错、消费拒绝，不降级进程内。
        #[tokio::test]
        async fn redis_outage_fails_closed() {
            let _serial = REDIS_SERIAL.lock().unwrap();
            // 6399 之外的端口基本保证无 Redis。
            let dead = "redis://127.0.0.1:6390/";
            let s = TerminalTicketStore::new(
                Duration::from_secs(60),
                redis::Client::open(dead).expect("valid dead Redis URL"),
            );
            assert!(
                s.issue(Uuid::new_v4()).await.is_err(),
                "Redis 故障时签发必须报错（fail closed）"
            );
            assert_eq!(
                s.consume("anything").await,
                None,
                "Redis 故障时消费必须拒绝"
            );
        }
    }
}
