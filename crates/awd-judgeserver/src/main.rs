//! FloatCTF AWD JudgeServer — 独立 Pull Worker，通过轮询执行健康检查/裁判脚本。
//!
//! # 架构（Wave 3.1 Pull）
//!
//! 1. JudgeServer 启动时生成稳定 worker_id
//! 2. 后台轮询循环 POST /internal/awd/events/{event_id}/judge/claim
//! 3. 对每个认领的任务：立即启动子进程执行
//! 4. 执行期间持续心跳续租（POST heartbeat）
//! 5. 执行完成后 POST result 提交结果
//! 6. 重复步骤 2–5 直到 shutdown
//!
//! # 配置（环境变量）
//!
//! - `PLATFORM_INTERNAL_URL` — FloatCTF 平台基址（容器视角可达）
//! - `EVENT_ID` — AWD 赛事 UUID
//! - `INTERNAL_TOKEN` — 平台鉴权 Bearer 令牌
//! - `LISTEN_ADDR` — 健康检查监听地址（默认 `"0.0.0.0:8080"`）
//! - `MAX_CONCURRENT` — 最大并发脚本数（默认 5）
//! - `WORK_DIR` — 临时脚本目录（默认 `"/tmp/judge"`）
//! - `POLL_INTERVAL_SECS` — 无任务时轮询间隔秒数（默认 5）
//! - `HEARTBEAT_INTERVAL_SECS` — 心跳间隔秒数（默认 30，必须 < LEASE_TTL/3）
//! - `LEASE_TTL_SECS` — 租约 TTL 秒数（默认 120，与平台一致）

use actix_web::{App, HttpResponse, HttpServer, web};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

// ── Platform API types (matching Wave 3 backend DTOs) ──

#[derive(Debug, Deserialize)]
struct ClaimedTask {
    task_id: Uuid,
    #[allow(dead_code)]
    batch_id: Uuid,
    #[allow(dead_code)]
    event_id: Uuid,
    #[allow(dead_code)]
    round_id: Uuid,
    #[allow(dead_code)]
    gamebox_instance_id: Uuid,
    #[allow(dead_code)]
    event_gamebox_id: Option<Uuid>,
    #[allow(dead_code)]
    team_id: Uuid,
    attempt: i32,
    lease_token: String,
    #[allow(dead_code)]
    lease_expires_at: String,
    #[allow(dead_code)]
    deadline_at: String,
    // Execution payload
    script_content: String,
    script_args_json: Option<String>,
    target_ip: String,
    timeout_secs: i32,
}

#[derive(Debug, Serialize)]
struct JudgeClaimRequest {
    worker_id: String,
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct JudgeClaimResponse {
    tasks: Vec<ClaimedTask>,
}

#[derive(Debug, Serialize)]
struct JudgeHeartbeatRequest {
    worker_id: String,
    attempt: i32,
    lease_token: String,
}

#[derive(Debug, Serialize)]
struct JudgeResultRequest {
    worker_id: String,
    attempt: i32,
    lease_token: String,
    result_id: String,
    outcome: String,
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    duration_ms: Option<i32>,
}

// ── App State ──

#[derive(Clone)]
struct AppState {
    client: Client,
    platform_url: String,
    event_id: String,
    internal_token: String,
    worker_id: String,
    concurrency: Arc<Semaphore>,
    work_dir: String,
    poll_interval: Duration,
    heartbeat_interval: Duration,
    /// Shutdown signal — set to true when shutdown begins.
    shutdown: Arc<AtomicBool>,
}

// ── Health endpoint ──

/// GET /health — 存活检查。
async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

/// GET /ready — 就绪检查（进程存活 + 可接受流量）。
async fn ready() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ready"}))
}

// ── Auth helpers ──

fn auth_header(token: &str) -> String {
    format!("Bearer {token}")
}

// ── Poll loop ──

/// 后台轮询主循环：claim → spawn → heartbeat → submit → repeat。
async fn poll_loop(state: AppState) {
    tracing::info!("Poll loop started (worker_id={})", state.worker_id);

    loop {
        if state.shutdown.load(Ordering::Relaxed) {
            tracing::info!("Shutdown signal received, poll loop exiting");
            break;
        }

        let available = state.concurrency.available_permits();
        if available == 0 {
            // All slots busy — sleep briefly and retry
            sleep(Duration::from_secs(1)).await;
            continue;
        }

        match claim_tasks(&state, available).await {
            Ok(tasks) => {
                if tasks.is_empty() {
                    sleep(state.poll_interval).await;
                } else {
                    tracing::info!("Claimed {} tasks", tasks.len());
                    for task in tasks {
                        let state_clone = state.clone();
                        let permit = state_clone
                            .concurrency
                            .clone()
                            .acquire_owned()
                            .await
                            .expect("Semaphore closed");
                        actix_web::rt::spawn(async move {
                            let _permit = permit;
                            execute_single_task(&state_clone, task).await;
                        });
                    }
                    // Brief yield to let spawned tasks acquire permits
                    sleep(Duration::from_millis(100)).await;
                }
            }
            Err(e) => {
                tracing::warn!("Claim request failed: {} — retrying in {}s", e, state.poll_interval.as_secs());
                sleep(state.poll_interval).await;
            }
        }
    }
}

/// POST /internal/awd/events/{event_id}/judge/claim
async fn claim_tasks(state: &AppState, limit: usize) -> Result<Vec<ClaimedTask>, String> {
    let url = format!(
        "{}/internal/awd/events/{}/judge/claim",
        state.platform_url, state.event_id
    );

    let body = JudgeClaimRequest {
        worker_id: state.worker_id.clone(),
        limit,
    };

    let resp = state
        .client
        .post(&url)
        .header("Authorization", auth_header(&state.internal_token))
        .json(&body)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    let claim: JudgeClaimResponse = resp
        .json()
        .await
        .map_err(|e| format!("deserialize: {e}"))?;

    Ok(claim.tasks)
}

// ── Task execution ──

/// 执行单个裁判任务：写脚本文件、子进程运行、心跳、回调结果。
async fn execute_single_task(state: &AppState, task: ClaimedTask) {
    tracing::info!("Executing task {} for {}", task.task_id, task.target_ip);

    let script_path = format!("{}/judge_{}.sh", state.work_dir, task.task_id);

    // Generate stable result_id for this execution attempt (reused across retries)
    let result_id = Uuid::new_v4().to_string();

    // Write script to temp file
    if let Err(e) = tokio::fs::write(&script_path, &task.script_content).await {
        tracing::error!("Failed to write script {}: {}", script_path, e);
        let err_msg = format!("Script write error: {e}");
        let _ = submit_result(
            state,
            &task,
            "worker_error",
            None,
            None,
            Some(&err_msg),
            None,
            &result_id,
        )
        .await;
        return;
    }

    // Make executable
    let _ = Command::new("chmod")
        .args(["+x", &script_path])
        .output()
        .await;

    // Start heartbeat loop for this task
    let heartbeat_handle = {
        let state_clone = state.clone();
        let task_id = task.task_id;
        let worker_id = state.worker_id.clone();
        let attempt = task.attempt;
        let lease_token = task.lease_token.clone();
        let interval = state.heartbeat_interval;

        actix_web::rt::spawn(async move {
            heartbeat_loop(&state_clone, task_id, &worker_id, attempt, &lease_token, interval).await
        })
    };

    // Build args from template, replacing {target_ip}
    let args: Vec<String> = task
        .script_args_json
        .as_ref()
        .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|arg| arg.replace("{target_ip}", &task.target_ip))
        .collect();

    let start = std::time::Instant::now();

    let exec_result = tokio::time::timeout(
        Duration::from_secs(task.timeout_secs as u64),
        // env 白名单：只透传 PATH/HOME/LANG 与 JUDGE_* 前缀变量
        Command::new(&script_path)
            .args(&args)
            .env_clear()
            .envs(build_script_env(env::vars()))
            .output(),
    )
    .await;

    let elapsed = start.elapsed();

    // Stop heartbeat
    heartbeat_handle.abort();

    let (outcome, exit_code, stdout, stderr) = match exec_result {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let stdout = truncate_str(&stdout, 4096);
            let stderr = truncate_str(&stderr, 4096);

            let outcome = match exit_code {
                0 => "up",
                1 => "down",
                _ => "worker_error",
            };

            (outcome, Some(exit_code), Some(stdout), Some(stderr))
        }
        Ok(Err(e)) => {
            tracing::error!("Task {} spawn failed: {}", task.task_id, e);
            (
                "worker_error",
                None,
                None,
                Some(format!("Execution error: {e}")),
            )
        }
        Err(_timeout) => {
            tracing::warn!("Task {} timed out", task.task_id);
            (
                "target_timeout",
                None,
                None,
                Some("Execution timed out".to_string()),
            )
        }
    };

    let duration_ms = Some(elapsed.as_millis() as i32);

    let _ = submit_result(
        state,
        &task,
        outcome,
        exit_code,
        stdout.as_deref(),
        stderr.as_deref(),
        duration_ms,
        &result_id,
    )
    .await;

    // Clean up script
    let _ = tokio::fs::remove_file(&script_path).await;
}

// ── Heartbeat ──

/// Heartbeat loop for a single task. Runs until aborted (task execution completes).
async fn heartbeat_loop(
    state: &AppState,
    task_id: Uuid,
    worker_id: &str,
    attempt: i32,
    lease_token: &str,
    interval: Duration,
) {
    // Wait for first interval before starting
    sleep(interval).await;

    loop {
        let url = format!(
            "{}/internal/awd/events/{}/judge/tasks/{}/heartbeat",
            state.platform_url, state.event_id, task_id
        );

        let body = JudgeHeartbeatRequest {
            worker_id: worker_id.to_string(),
            attempt,
            lease_token: lease_token.to_string(),
        };

        let result = state
            .client
            .post(&url)
            .header("Authorization", auth_header(&state.internal_token))
            .json(&body)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                tracing::debug!("Heartbeat OK for task {}", task_id);
            }
            Ok(resp) if resp.status().as_u16() == 409 => {
                tracing::warn!("Heartbeat 409 stale for task {} — ownership lost", task_id);
                // Ownership lost; the parent task executor should discard the result.
                // We signal this by aborting the heartbeat (the parent will see the error
                // when it tries to submit the result).
                return;
            }
            Ok(resp) => {
                tracing::warn!(
                    "Heartbeat failed for task {}: HTTP {} — will retry",
                    task_id,
                    resp.status()
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Heartbeat network error for task {}: {} — will retry",
                    task_id,
                    e
                );
            }
        }

        sleep(interval).await;
    }
}

// ── Result delivery ──

/// Result POST 最大尝试次数：1 次主尝试 + 3 次重试。
const RESULT_MAX_ATTEMPTS: usize = 4;
/// 指数退避间隔（秒）：第 2 / 3 / 4 次尝试前分别等待 1s / 2s / 4s。
const RESULT_RETRY_DELAYS_SECS: [u64; 3] = [1, 2, 4];

/// 将任务结果 POST 回平台。
///
/// 失败时按指数退避自动重试（最多 RESULT_MAX_ATTEMPTS 次）。
/// **每次重试复用同一个 result_id**，平台侧按 result_id 幂等。
/// 全部尝试仍失败 → 记录 error 并静默丢弃。
async fn submit_result(
    state: &AppState,
    task: &ClaimedTask,
    outcome: &str,
    exit_code: Option<i32>,
    stdout: Option<&str>,
    stderr: Option<&str>,
    duration_ms: Option<i32>,
    result_id: &str,
) -> Result<(), String> {
    let url = format!(
        "{}/internal/awd/events/{}/judge/tasks/{}/result",
        state.platform_url, state.event_id, task.task_id
    );

    let mut last_error = String::new();

    for attempt in 1..=RESULT_MAX_ATTEMPTS {
        if attempt > 1 {
            let delay_secs = RESULT_RETRY_DELAYS_SECS[attempt - 2];
            tracing::warn!(
                "Result for task {} failed, retrying in {}s (attempt {}/{})",
                task.task_id,
                delay_secs,
                attempt,
                RESULT_MAX_ATTEMPTS
            );
            sleep(Duration::from_secs(delay_secs)).await;
        }

        let body = JudgeResultRequest {
            worker_id: state.worker_id.clone(),
            attempt: task.attempt,
            lease_token: task.lease_token.clone(),
            result_id: result_id.to_string(),
            outcome: outcome.to_string(),
            exit_code,
            stdout: stdout.map(|s| s.to_string()),
            stderr: stderr.map(|s| s.to_string()),
            duration_ms,
        };

        let res = state
            .client
            .post(&url)
            .header("Authorization", auth_header(&state.internal_token))
            .json(&body)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match res {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!("Result OK for task {} (outcome={})", task.task_id, outcome);
                return Ok(());
            }
            Ok(resp) if resp.status().as_u16() == 409 => {
                tracing::warn!(
                    "Result 409 stale for task {} — ownership lost, discarding",
                    task.task_id
                );
                return Err("stale".to_string());
            }
            Ok(resp) => {
                last_error = format!("HTTP {}", resp.status());
                tracing::warn!(
                    "Result failed for task {}: HTTP {} (attempt {}/{})",
                    task.task_id,
                    resp.status(),
                    attempt,
                    RESULT_MAX_ATTEMPTS
                );
            }
            Err(e) => {
                last_error = format!("network error: {}", e);
                tracing::warn!(
                    "Result network error for task {}: {} (attempt {}/{})",
                    task.task_id,
                    e,
                    attempt,
                    RESULT_MAX_ATTEMPTS
                );
            }
        }
    }

    tracing::error!(
        "Result permanently failed for task {} after {} attempts: {}",
        task.task_id,
        RESULT_MAX_ATTEMPTS,
        last_error
    );
    Err(last_error)
}

// ── Utilities ──

/// Judge 脚本执行的最小 env 白名单。
///
/// 规则：禁止透传宿主全部环境变量；只保留
/// - `PATH` / `HOME` / `LANG`：脚本正常运行所需的基础变量
/// - 宿主中以 `JUDGE_` 开头的变量：运维可按需给脚本注入自定义配置
fn build_script_env(host_env: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    host_env
        .filter(|(key, _)| {
            key == "PATH" || key == "HOME" || key == "LANG" || key.starts_with("JUDGE_")
        })
        .collect()
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}... (truncated, {} bytes total)", &s[..max_len], s.len())
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_env_allowlist_only_keeps_whitelisted_vars() {
        let host_env = vec![
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            ("HOME".to_string(), "/root".to_string()),
            ("LANG".to_string(), "C.UTF-8".to_string()),
            ("JUDGE_TIMEOUT_FACTOR".to_string(), "3".to_string()),
            // 敏感项与无关变量必须被过滤掉。
            ("INTERNAL_TOKEN".to_string(), "should-not-leak".to_string()),
            (
                "PLATFORM_INTERNAL_URL".to_string(),
                "http://127.0.0.1".to_string(),
            ),
            ("RUST_LOG".to_string(), "debug".to_string()),
        ];

        let allowlist = build_script_env(host_env.into_iter());
        let keys: Vec<&str> = allowlist.iter().map(|(k, _)| k.as_str()).collect();

        assert!(keys.contains(&"PATH"));
        assert!(keys.contains(&"HOME"));
        assert!(keys.contains(&"LANG"));
        assert!(keys.contains(&"JUDGE_TIMEOUT_FACTOR"));
        assert!(!keys.contains(&"INTERNAL_TOKEN"));
        assert!(!keys.contains(&"PLATFORM_INTERNAL_URL"));
        assert!(!keys.contains(&"RUST_LOG"));
        assert_eq!(allowlist.len(), 4);
    }

    #[test]
    fn truncate_short_string_unchanged() {
        let s = "hello";
        assert_eq!(truncate_str(s, 10), "hello");
    }

    #[test]
    fn truncate_long_string_adds_truncation_note() {
        let s = "a".repeat(5000);
        let result = truncate_str(&s, 4096);
        assert!(result.starts_with(&"a".repeat(4096)));
        assert!(result.contains("truncated"));
        assert!(result.contains("5000 bytes total"));
    }

    #[test]
    fn auth_header_has_bearer_prefix() {
        assert_eq!(auth_header("my-token"), "Bearer my-token");
    }
}

// ── Main ──

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("default=info".parse().unwrap()),
        )
        .init();

    let platform_url = env::var("PLATFORM_INTERNAL_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9090".to_string());
    let event_id = env::var("EVENT_ID").expect("EVENT_ID must be set");
    let internal_token = env::var("INTERNAL_TOKEN").expect("INTERNAL_TOKEN must be set");
    let listen_addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let max_concurrent: usize = env::var("MAX_CONCURRENT")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .unwrap_or(5);
    let work_dir = env::var("WORK_DIR").unwrap_or_else(|_| "/tmp/judge".to_string());
    let poll_interval_secs: u64 = env::var("POLL_INTERVAL_SECS")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .unwrap_or(5);
    let heartbeat_interval_secs: u64 = env::var("HEARTBEAT_INTERVAL_SECS")
        .unwrap_or_else(|_| "30".to_string())
        .parse()
        .unwrap_or(30);
    let lease_ttl_secs: u64 = env::var("LEASE_TTL_SECS")
        .unwrap_or_else(|_| "120".to_string())
        .parse()
        .unwrap_or(120);

    // 校验：heartbeat 间隔必须 < lease_ttl / 3
    let _ = lease_ttl_secs; // Used only for the check below

    // 校验：heartbeat 间隔必须 < lease_ttl / 3
    if heartbeat_interval_secs >= lease_ttl_secs / 3 {
        tracing::warn!(
            "HEARTBEAT_INTERVAL_SECS ({}) >= LEASE_TTL_SECS/3 ({}); heartbeats may be too infrequent",
            heartbeat_interval_secs,
            lease_ttl_secs / 3
        );
    }

    // Generate stable worker_id for this process lifetime
    let worker_id = Uuid::new_v4().to_string();

    // Ensure work directory exists
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work directory");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to create HTTP client");

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    let state = AppState {
        client,
        platform_url,
        event_id,
        internal_token,
        worker_id,
        concurrency: Arc::new(Semaphore::new(max_concurrent)),
        work_dir,
        poll_interval: Duration::from_secs(poll_interval_secs),
        heartbeat_interval: Duration::from_secs(heartbeat_interval_secs),
        shutdown: shutdown_clone,
    };

    // Start background poll loop
    let poll_state = state.clone();
    let poll_handle = actix_web::rt::spawn(async move {
        poll_loop(poll_state).await;
    });

    tracing::info!(
        "FloatCTF AWD JudgeServer starting on {} (max_concurrent={}, worker_id={})",
        listen_addr,
        max_concurrent,
        state.worker_id
    );

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/health", web::get().to(health))
            .route("/ready", web::get().to(ready))
    })
    .bind(&listen_addr)?
    .run();

    // Graceful shutdown: set shutdown flag, give tasks time to finish
    let server_handle = server.handle();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutdown signal received, draining...");
        shutdown.store(true, Ordering::Relaxed);
        // Give running tasks a grace period to finish
        sleep(Duration::from_secs(10)).await;
        server_handle.stop(true).await;
    });

    server.await?;

    // Ensure poll loop exits
    poll_handle.abort();

    Ok(())
}