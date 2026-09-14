//! GameBox 健康检查探针（共享基础设施）。

use std::net::SocketAddr;
use std::time::Duration;

use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::modules::gamebox::{GameboxError, GameboxResult};

/// 单条就绪探针（与 fcmc::NormalizedHealthcheck JSON 形状一致）。
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

/// 将 healthchecks_json（数组）解析为类型化探针。
pub fn parse_healthchecks(json: &serde_json::Value) -> GameboxResult<Vec<AppHealthcheck>> {
    if json.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(json.clone())
        .map_err(|e| GameboxError::Validation(format!("healthchecks_json invalid: {e}")))
}

/// 对 `ip` 执行全部探针，返回逐条结果（不短路；全部条目都会执行）。
pub async fn probe_all(
    ip: &str,
    checks: &[AppHealthcheck],
    per_check_timeout: Duration,
) -> Vec<ProbeResult> {
    let mut out = Vec::with_capacity(checks.len());
    for c in checks {
        out.push(probe_one(ip, c, per_check_timeout).await);
    }
    out
}

/// 单条探针；带 `attempts` 次重试（每次间隔 `retry_delay`）。
/// 返回最后一次尝试的结果。
pub async fn probe_one_with_retries(
    ip: &str,
    check: &AppHealthcheck,
    per_check_timeout: Duration,
    attempts: u32,
    retry_delay: Duration,
) -> ProbeResult {
    let mut result = probe_one(ip, check, per_check_timeout).await;
    let mut attempt = 1u32;
    while !result.ok && attempt < attempts {
        tokio::time::sleep(retry_delay).await;
        result = probe_one(ip, check, per_check_timeout).await;
        attempt += 1;
    }
    result
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

/// 便捷方法：全部检查必须通过（无重试）。
pub async fn all_healthy(
    ip: &str,
    healthchecks_json: &serde_json::Value,
    per_check_timeout: Duration,
) -> GameboxResult<bool> {
    let checks = parse_healthchecks(healthchecks_json)?;
    if checks.is_empty() {
        return Ok(true);
    }
    let results = probe_all(ip, &checks, per_check_timeout).await;
    Ok(results.iter().all(|r| r.ok))
}

/// 将 IP:port 解析为 SocketAddr（尽力校验辅助）。
pub fn parse_socket(ip: &str, port: u16) -> GameboxResult<SocketAddr> {
    format!("{ip}:{port}")
        .parse()
        .map_err(|e| GameboxError::Validation(format!("invalid address {ip}:{port}: {e}")))
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
