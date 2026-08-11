//! Application readiness probes for GameBox (HTTP GET / TCP connect).
//!
//! These are **not** Docker HEALTHCHECK instructions. They come from
//! `gamebox_revisions.healthchecks_json` (or EventGameBox override) and are
//! used by precheck / reset wait-ready paths.

use std::net::SocketAddr;
use std::time::Duration;

use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::modules::event::awd::{AwdError, AwdResult};

/// One readiness probe entry (mirrors fcmc::NormalizedHealthcheck JSON shape).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AppHealthcheck {
    Http {
        port: u16,
        path: String,
        #[serde(default = "default_status")]
        expected_status: u16,
    },
    Tcp {
        port: u16,
    },
}

fn default_status() -> u16 {
    200
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub ok: bool,
    pub detail: String,
}

/// Parse healthchecks_json (array) into typed probes.
pub fn parse_healthchecks(json: &serde_json::Value) -> AwdResult<Vec<AppHealthcheck>> {
    if json.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(json.clone())
        .map_err(|e| AwdError::Validation(format!("healthchecks_json invalid: {e}")))
}

/// Probe all checks against `ip`. Returns Ok only if every check passes.
pub async fn probe_all(
    ip: &str,
    checks: &[AppHealthcheck],
    per_check_timeout: Duration,
) -> AwdResult<Vec<ProbeResult>> {
    let mut out = Vec::with_capacity(checks.len());
    for c in checks {
        out.push(probe_one(ip, c, per_check_timeout).await);
    }
    Ok(out)
}

pub async fn probe_one(ip: &str, check: &AppHealthcheck, t: Duration) -> ProbeResult {
    match check {
        AppHealthcheck::Tcp { port } => match timeout(t, TcpStream::connect((ip, *port))).await {
            Ok(Ok(_)) => ProbeResult {
                ok: true,
                detail: format!("tcp://{ip}:{port} open"),
            },
            Ok(Err(e)) => ProbeResult {
                ok: false,
                detail: format!("tcp://{ip}:{port} connect failed: {e}"),
            },
            Err(_) => ProbeResult {
                ok: false,
                detail: format!("tcp://{ip}:{port} timed out"),
            },
        },
        AppHealthcheck::Http {
            port,
            path,
            expected_status,
        } => {
            let url = format!("http://{ip}:{port}{path}");
            let client = match reqwest::Client::builder().timeout(t).build() {
                Ok(c) => c,
                Err(e) => {
                    return ProbeResult {
                        ok: false,
                        detail: format!("http client error: {e}"),
                    };
                }
            };
            match client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if status == *expected_status {
                        ProbeResult {
                            ok: true,
                            detail: format!("{url} → {status}"),
                        }
                    } else {
                        ProbeResult {
                            ok: false,
                            detail: format!("{url} → {status}, expected {expected_status}"),
                        }
                    }
                }
                Err(e) => ProbeResult {
                    ok: false,
                    detail: format!("{url} request failed: {e}"),
                },
            }
        }
    }
}

/// Convenience: all checks must pass.
pub async fn all_healthy(
    ip: &str,
    healthchecks_json: &serde_json::Value,
    per_check_timeout: Duration,
) -> AwdResult<bool> {
    let checks = parse_healthchecks(healthchecks_json)?;
    if checks.is_empty() {
        return Ok(true);
    }
    let results = probe_all(ip, &checks, per_check_timeout).await?;
    Ok(results.iter().all(|r| r.ok))
}

/// Resolve IP:port to SocketAddr (best-effort validation helper).
#[allow(dead_code)]
pub fn parse_socket(ip: &str, port: u16) -> AwdResult<SocketAddr> {
    format!("{ip}:{port}")
        .parse()
        .map_err(|e| AwdError::Validation(format!("invalid address {ip}:{port}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_and_tcp() {
        let v = serde_json::json!([
            {"type": "http", "port": 80, "path": "/", "expected_status": 200},
            {"type": "tcp", "port": 3306}
        ]);
        let checks = parse_healthchecks(&v).unwrap();
        assert_eq!(checks.len(), 2);
        assert!(matches!(checks[0], AppHealthcheck::Http { port: 80, .. }));
        assert!(matches!(checks[1], AppHealthcheck::Tcp { port: 3306 }));
    }

    #[test]
    fn empty_json_ok() {
        assert!(
            parse_healthchecks(&serde_json::Value::Null)
                .unwrap()
                .is_empty()
        );
        assert!(
            parse_healthchecks(&serde_json::json!([]))
                .unwrap()
                .is_empty()
        );
    }
}
