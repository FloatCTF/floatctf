//! Shared helpers for HTTP API contract / flow tests.
//!
//! Tests talk to a **running** floatctf API (full stack: DB + docker + rustfs).
//! Set `FLOATCTF_API_BASE` (default `http://127.0.0.1:8080`).
//! If the server is unreachable, tests are **skipped** (not failed) unless
//! `FLOATCTF_API_REQUIRE=1` is set.

pub mod routes;

use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

pub const NIL: &str = "00000000-0000-0000-0000-000000000000";

pub fn base_url() -> String {
    std::env::var("FLOATCTF_API_BASE").unwrap_or_else(|_| "http://127.0.0.1:8080".into())
}

pub fn require_live() -> bool {
    std::env::var("FLOATCTF_API_REQUIRE").ok().as_deref() == Some("1")
}

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client")
    })
}

/// Returns false if API is down / not floatctf and tests should soft-skip.
pub async fn api_reachable() -> bool {
    let url = format!("{}/api/events", base_url());
    match client().get(&url).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            // Gateway/proxy 502/503 means nothing useful behind the port.
            if status == 502 || status == 503 || status == 504 {
                if require_live() {
                    panic!(
                        "FLOATCTF_API_REQUIRE=1 but {} returned {} (proxy/upstream down)",
                        base_url(),
                        status
                    );
                }
                eprintln!(
                    "skip: {} returned HTTP {} (not a live floatctf API). Start the server.",
                    base_url(),
                    status
                );
                return false;
            }
            // floatctf returns JSON UniResponse even on 401.
            let ct = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if status == 401 || ct.contains("json") {
                return true;
            }
            if require_live() {
                panic!(
                    "FLOATCTF_API_REQUIRE=1 but {} status={} content-type={ct} looks wrong",
                    base_url(),
                    status
                );
            }
            eprintln!(
                "skip: {} status={} content-type={ct} (unexpected for floatctf)",
                base_url(),
                status
            );
            false
        }
        Err(e) => {
            if require_live() {
                panic!(
                    "FLOATCTF_API_REQUIRE=1 but API not reachable at {}: {}",
                    base_url(),
                    e
                );
            }
            eprintln!(
                "skip: API not reachable at {} ({e}). Start the server or set FLOATCTF_API_BASE.",
                base_url()
            );
            false
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Auth {
    None,
    /// Expect 401 without a token (or with garbage token).
    UserRequired,
    SuperAdminRequired,
}

#[derive(Debug, Clone, Copy)]
pub enum Method {
    Get,
    Post,
    Patch,
    Delete,
}

pub struct Route {
    pub method: Method,
    pub path: &'static str,
    pub auth: Auth,
    /// If true, send minimal JSON `{}` body for POST/PATCH/DELETE.
    pub json_body: bool,
}

pub async fn call(route: &Route, bearer: Option<&str>) -> reqwest::Response {
    let url = format!("{}{}", base_url(), route.path);
    let mut req = match route.method {
        Method::Get => client().get(&url),
        Method::Post => client().post(&url),
        Method::Patch => client().patch(&url),
        Method::Delete => client().delete(&url),
    };
    if let Some(t) = bearer {
        req = req.bearer_auth(t);
    }
    if route.json_body {
        req = req.json(&serde_json::json!({}));
    }
    req.send().await.expect("request failed")
}

pub async fn json_code(resp: reqwest::Response) -> (u16, Option<i64>, Value) {
    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    let code = body.get("code").and_then(|c| c.as_i64());
    (status, code, body)
}

/// Protected routes must not succeed without credentials.
pub fn assert_auth_denied(status: u16, code: Option<i64>, route: &Route) {
    // Actix Jwt guards typically yield 401; some paths may 403.
    // code 401 in UniResponse body is also accepted (some middleware paths).
    assert!(
        status == 401
            || status == 403
            || code == Some(401)
            || code == Some(403)
            || code == Some(401),
        "route {:?} {} expected auth denial, got status={} code={:?}",
        route.method,
        route.path,
        status,
        code
    );
}

pub async fn login_user(username: &str, password: &str) -> Option<String> {
    let url = format!("{}/api/users/session", base_url());
    let resp = client()
        .post(&url)
        .json(&serde_json::json!({ "username": username, "password": password }))
        .send()
        .await
        .ok()?;
    let body: Value = resp.json().await.ok()?;
    if body.get("code").and_then(|c| c.as_i64()) != Some(0) {
        return None;
    }
    body.get("data")
        .and_then(|d| d.as_str())
        .map(|s| s.to_string())
}

pub async fn login_admin(username: &str, password: &str) -> Option<String> {
    let url = format!("{}/api/admin/session", base_url());
    let resp = client()
        .post(&url)
        .json(&serde_json::json!({ "username": username, "password": password }))
        .send()
        .await
        .ok()?;
    let body: Value = resp.json().await.ok()?;
    if body.get("code").and_then(|c| c.as_i64()) != Some(0) {
        return None;
    }
    body.get("data")
        .and_then(|d| d.as_str())
        .map(|s| s.to_string())
}
