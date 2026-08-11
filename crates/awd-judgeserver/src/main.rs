//! AWD JudgeServer — 独立 worker，执行健康检查/裁判脚本。
//!
//! # 架构
//!
//! 1. 平台向 JudgeServer 下发裁判批次（任务列表）
//! 2. JudgeServer 将各任务脚本作为本地子进程执行
//! 3. 脚本经命令行参数接收目标 IP
//! 4. 脚本经网络检查目标 GameBox 服务
//! 5. 每个任务结果立即 POST 回平台
//!
//! # 配置（环境变量）
//!
//! - `PLATFORM_INTERNAL_URL` — FloatCTF 平台基址
//! - `EVENT_ID` — AWD 赛事 UUID
//! - `INTERNAL_TOKEN` — 平台鉴权 Bearer 令牌
//! - `LISTEN_ADDR` — 监听地址（默认 `"0.0.0.0:8082"`）
//! - `MAX_CONCURRENT` — 最大并发脚本数（默认 5）
//! - `WORK_DIR` — 临时脚本目录（默认 `"/tmp/judge"`）

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::Duration;

// ── Platform API types ──

#[derive(Debug, Deserialize)]
struct JudgeTask {
    id: uuid::Uuid,
    script_content: String,
    script_args_json: Option<String>,
    target_ip: String,
    timeout_secs: u64,
    callback_id: String,
}

#[derive(Debug, Deserialize)]
struct JudgeBatch {
    tasks: Vec<JudgeTask>,
}

#[derive(Debug, Serialize)]
struct TaskResult {
    task_id: uuid::Uuid,
    callback_id: String,
    status: String,
    attempt_count: i32,
    exit_code: Option<i32>,
    duration_ms: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
}

// ── App State ──

#[derive(Clone)]
struct AppState {
    client: Client,
    platform_url: String,
    event_id: String,
    internal_token: String,
    concurrency: Arc<Semaphore>,
    work_dir: String,
}

/// POST /batch — 接收并执行一批裁判任务。
async fn handle_batch(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<JudgeBatch>,
) -> HttpResponse {
    if !has_valid_bearer_token(&request, &state.internal_token) {
        return HttpResponse::Unauthorized().finish();
    }

    let tasks = body.into_inner().tasks;
    tracing::info!("Received batch with {} tasks", tasks.len());

    let state_arc = state.into_inner();
    let sem = state_arc.concurrency.clone();

    // Spawn each task concurrently, bounded by semaphore
    for task in tasks {
        let state_clone = state_arc.clone();
        let permit = sem.clone().acquire_owned().await;
        actix_web::rt::spawn(async move {
            let _permit = permit;
            execute_single_task(&state_clone, task).await;
        });
    }

    HttpResponse::Ok().json(serde_json::json!({"accepted": true}))
}

fn has_valid_bearer_token(request: &HttpRequest, expected_token: &str) -> bool {
    let Some(provided_token) = request
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };

    constant_time_eq(provided_token.as_bytes(), expected_token.as_bytes())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

/// 执行单个裁判任务：写脚本文件、子进程运行、回调结果。
async fn execute_single_task(state: &AppState, task: JudgeTask) {
    tracing::info!("Executing task {} for {}", task.id, task.target_ip);

    let script_path = format!("{}/judge_{}.sh", state.work_dir, task.id);

    // Write script to temp file
    if let Err(e) = tokio::fs::write(&script_path, &task.script_content).await {
        tracing::error!("Failed to write script {}: {}", script_path, e);
        let _ = send_result(
            state,
            &task,
            TaskResult {
                task_id: task.id,
                callback_id: task.callback_id.clone(),
                status: "judge_error".to_string(),
                attempt_count: 1,
                exit_code: None,
                duration_ms: None,
                stdout: None,
                stderr: Some(format!("Script write error: {}", e)),
            },
        )
        .await;
        return;
    }

    // Make executable
    let _ = Command::new("chmod")
        .args(["+x", &script_path])
        .output()
        .await;

    let mut attempts = 0;
    let max_attempts = 2;

    loop {
        attempts += 1;
        let start = std::time::Instant::now();

        // Build args from template, replacing {target_ip}
        let args: Vec<String> = task
            .script_args_json
            .as_ref()
            .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|arg| arg.replace("{target_ip}", &task.target_ip))
            .collect();

        let result = tokio::time::timeout(
            Duration::from_secs(task.timeout_secs),
            // env 白名单（P3-12）：只透传 PATH/HOME/LANG 与 JUDGE_* 前缀变量，
            // 其余宿主环境（含 INTERNAL_TOKEN 等敏感项）对脚本一律不可见。
            Command::new(&script_path)
                .args(&args)
                .env_clear()
                .envs(build_script_env(env::vars()))
                .output(),
        )
        .await;

        let elapsed = start.elapsed();

        match result {
            Ok(Ok(output)) => {
                let exit_code = output.status.code().unwrap_or(-1);
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                // Truncate output to reasonable limits
                let stdout = truncate_str(&stdout, 4096);
                let stderr = truncate_str(&stderr, 4096);

                let status = match exit_code {
                    0 => "up",
                    1 => "down",
                    _ => "judge_error",
                };

                let _ = send_result(
                    state,
                    &task,
                    TaskResult {
                        task_id: task.id,
                        callback_id: task.callback_id.clone(),
                        status: status.to_string(),
                        attempt_count: attempts,
                        exit_code: Some(exit_code),
                        duration_ms: Some(elapsed.as_millis() as i32),
                        stdout: Some(stdout),
                        stderr: Some(stderr),
                    },
                )
                .await;

                // Clean up script
                let _ = tokio::fs::remove_file(&script_path).await;
                return;
            }
            Ok(Err(e)) => {
                tracing::error!("Task {} failed: {}", task.id, e);
                if attempts >= max_attempts {
                    let _ = send_result(
                        state,
                        &task,
                        TaskResult {
                            task_id: task.id,
                            callback_id: task.callback_id.clone(),
                            status: "judge_error".to_string(),
                            attempt_count: attempts,
                            exit_code: None,
                            duration_ms: None,
                            stdout: None,
                            stderr: Some(format!("Execution error: {}", e)),
                        },
                    )
                    .await;
                    let _ = tokio::fs::remove_file(&script_path).await;
                    return;
                }
            }
            Err(_timeout) => {
                tracing::warn!("Task {} timed out", task.id);
                if attempts >= max_attempts {
                    let _ = send_result(
                        state,
                        &task,
                        TaskResult {
                            task_id: task.id,
                            callback_id: task.callback_id.clone(),
                            status: "judge_timeout".to_string(),
                            attempt_count: attempts,
                            exit_code: None,
                            duration_ms: Some(task.timeout_secs as i32 * 1000),
                            stdout: None,
                            stderr: Some("Execution timed out".to_string()),
                        },
                    )
                    .await;
                    let _ = tokio::fs::remove_file(&script_path).await;
                    return;
                }
            }
        }
    }
}

// ── Callback 重试（P3-9）──

/// 回调最大尝试次数：初始 1 次 + 3 次重试。
const CALLBACK_MAX_ATTEMPTS: usize = 4;
/// 指数退避间隔（秒）：第 2 / 3 / 4 次尝试前分别等待 1s / 2s / 4s。
const CALLBACK_RETRY_DELAYS_SECS: [u64; 3] = [1, 2, 4];

/// 第 `attempt` 次尝试前的退避等待时长（attempt 从 1 开始；首次尝试不等待）。
/// 调用方需保证 `attempt >= 2`。
fn callback_retry_delay(attempt: usize) -> Duration {
    Duration::from_secs(CALLBACK_RETRY_DELAYS_SECS[attempt - 2])
}

/// Judge 脚本执行的最小 env 白名单（P3-12，安全边界）。
///
/// 规则：禁止透传宿主全部环境变量；只保留
/// - `PATH` / `HOME` / `LANG`：脚本正常运行所需的基础变量
/// - 宿主中以 `JUDGE_` 开头的变量：运维可按需给脚本注入自定义配置
///
/// 其余环境变量一律清除——特别是 `INTERNAL_TOKEN` 等敏感项
/// 绝不能对任意脚本内容可见。
fn build_script_env(host_env: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    host_env
        .filter(|(key, _)| {
            key == "PATH" || key == "HOME" || key == "LANG" || key.starts_with("JUDGE_")
        })
        .collect()
}

/// 将任务结果 POST 回平台。
///
/// 失败时按指数退避自动重试（最多 `CALLBACK_MAX_ATTEMPTS` 次）。
/// **每次重试复用同一个 `result`（同一 callback_id / 同一 callback identity）**：
/// 平台侧 judge_callback 已按 callback_id/task_id 幂等（P3-8），
/// 重复投递不会重复计分；绝不能为每次重试生成新 ID（计划 §5.7）。
/// 全部尝试仍失败 → 记录 error 并返回 Err（保持原有"记录后继续"语义）。
async fn send_result(state: &AppState, task: &JudgeTask, result: TaskResult) -> Result<(), String> {
    let url = format!(
        "{}/internal/awd/events/{}/judge/callback",
        state.platform_url, state.event_id
    );

    let mut last_error = String::new();

    for attempt in 1..=CALLBACK_MAX_ATTEMPTS {
        if attempt > 1 {
            let delay = callback_retry_delay(attempt);
            tracing::warn!(
                "Callback for task {} failed, retrying in {:?} (attempt {}/{})",
                task.id,
                delay,
                attempt,
                CALLBACK_MAX_ATTEMPTS
            );
            tokio::time::sleep(delay).await;
        }

        // 每次尝试复用同一个 result（含同一 callback_id），保持同 identity。
        let res = state
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", state.internal_token))
            .json(&result)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match res {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!("Callback OK for task {}", task.id);
                return Ok(());
            }
            Ok(resp) => {
                last_error = format!("HTTP {}", resp.status());
                tracing::warn!(
                    "Callback failed for task {}: HTTP {} (attempt {}/{})",
                    task.id,
                    resp.status(),
                    attempt,
                    CALLBACK_MAX_ATTEMPTS
                );
            }
            Err(e) => {
                last_error = format!("network error: {}", e);
                tracing::warn!(
                    "Callback network error for task {}: {} (attempt {}/{})",
                    task.id,
                    e,
                    attempt,
                    CALLBACK_MAX_ATTEMPTS
                );
            }
        }
    }

    tracing::error!(
        "Callback permanently failed for task {} after {} attempts: {}",
        task.id,
        CALLBACK_MAX_ATTEMPTS,
        last_error
    );
    Err(last_error)
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}... (truncated, {} bytes total)", &s[..max_len], s.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn batch_auth_requires_matching_bearer_token() {
        let matching = TestRequest::default()
            .insert_header(("Authorization", "Bearer event-token"))
            .to_http_request();
        let missing = TestRequest::default().to_http_request();
        let wrong = TestRequest::default()
            .insert_header(("Authorization", "Bearer wrong-token"))
            .to_http_request();

        assert!(has_valid_bearer_token(&matching, "event-token"));
        assert!(!has_valid_bearer_token(&missing, "event-token"));
        assert!(!has_valid_bearer_token(&wrong, "event-token"));
    }

    #[test]
    fn callback_retry_delay_follows_exponential_backoff() {
        // 最多 4 次尝试（初始 1 次 + 3 次重试），退避间隔 1s / 2s / 4s。
        assert_eq!(CALLBACK_MAX_ATTEMPTS, 4);
        assert_eq!(CALLBACK_RETRY_DELAYS_SECS, [1, 2, 4]);
        assert_eq!(callback_retry_delay(2), Duration::from_secs(1));
        assert_eq!(callback_retry_delay(3), Duration::from_secs(2));
        assert_eq!(callback_retry_delay(4), Duration::from_secs(4));
    }

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
}

/// GET /health — 存活检查。
async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("default=info".parse().unwrap()),
        )
        .init();

    let platform_url =
        env::var("PLATFORM_INTERNAL_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
    let event_id = env::var("EVENT_ID").expect("EVENT_ID must be set");
    let internal_token = env::var("INTERNAL_TOKEN").expect("INTERNAL_TOKEN must be set");
    let listen_addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8082".to_string());
    let max_concurrent: usize = env::var("MAX_CONCURRENT")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .unwrap_or(5);
    let work_dir = env::var("WORK_DIR").unwrap_or_else(|_| "/tmp/judge".to_string());

    // Ensure work directory exists
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work directory");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to create HTTP client");

    let state = AppState {
        client,
        platform_url,
        event_id,
        internal_token,
        concurrency: Arc::new(Semaphore::new(max_concurrent)),
        work_dir,
    };

    tracing::info!(
        "FloatCTF AWD JudgeServer starting on {} (max_concurrent={})",
        listen_addr,
        max_concurrent
    );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/batch", web::post().to(handle_batch))
            .route("/health", web::get().to(health))
    })
    .bind(&listen_addr)?
    .run()
    .await
}
