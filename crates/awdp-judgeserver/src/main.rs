//! AWDP Practice JudgeServer — 对练习 GameBox 执行 exploit 检查与 flag curl 验证。
//!
//! # 架构
//!
//! 1. 平台向 JudgeServer 下发裁判批次（任务列表）
//! 2. `kind=exploit` 任务：将 GameBox 的 awdp exploit 脚本作为本地子进程执行
//!    （命令行参数含目标 IP），验证练习靶机可被攻破
//! 3. `kind=flag` 任务：直接 HTTP GET 目标的 flag 端点（如 `/flag.php`），
//!    验证返回体与平台预期的确定性 flag 一致（练习靶机 flag 已暴露）
//! 4. 每个任务结果立即 POST 回平台
//!
//! JudgeServer 部署在练习专用 docker 子网（`fctf-awdp-practice`）内，
//! 因此可以按容器内网 IP 直达全部练习 GameBox，无需宿主端口转发。
//!
//! # 配置（环境变量）
//!
//! - `PLATFORM_INTERNAL_URL` — FloatCTF 平台基址（回调目标）
//! - `EVENT_ID` — AWDPlusPractice 虚拟赛事 UUID
//! - `INTERNAL_TOKEN` — 平台鉴权 Bearer 令牌（回调认证 + 本服务 /batch 校验）
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

/// 单条裁判任务。
///
/// `kind`：
/// - `exploit` — 运行 `script_content`（awdp exploit 脚本），参数含目标 IP；
///   结果 success = 目标可被攻破（vulnerable）。
/// - `flag` — HTTP GET `flag_url`，返回体与 `expected_flag` 一致
///   结果 success = 目标 flag 已暴露。
#[derive(Debug, Deserialize)]
struct JudgeTask {
    id: uuid::Uuid,
    kind: String,
    #[serde(default)]
    script_content: Option<String>,
    #[serde(default)]
    script_args_json: Option<String>,
    target_ip: String,
    #[serde(default)]
    flag_url: Option<String>,
    #[serde(default)]
    expected_flag: Option<String>,
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
    kind: String,
    status: String,
    exit_code: Option<i32>,
    duration_ms: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    detail: Option<String>,
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

/// 执行单个裁判任务（exploit 子进程 / flag HTTP GET）。
async fn execute_single_task(state: &AppState, task: JudgeTask) {
    tracing::info!(
        "Executing {} task {} for {}",
        task.kind,
        task.id,
        task.target_ip
    );

    let start = std::time::Instant::now();
    let outcome = match task.kind.as_str() {
        "exploit" => execute_exploit_task(state, &task).await,
        "flag" => execute_flag_task(state, &task).await,
        other => {
            let _ = send_result(
                state,
                &task,
                TaskResult {
                    task_id: task.id,
                    callback_id: task.callback_id.clone(),
                    kind: task.kind.clone(),
                    status: "judge_error".to_string(),
                    exit_code: None,
                    duration_ms: None,
                    stdout: None,
                    stderr: None,
                    detail: Some(format!("unknown task kind: {other}")),
                },
            )
            .await;
            return;
        }
    };
    let elapsed = start.elapsed();

    let (status, exit_code, stdout, stderr, detail) = match outcome {
        Ok(TaskOutcome {
            success,
            exit_code,
            stdout,
            stderr,
            detail,
        }) => (
            if success { "success" } else { "failure" }.to_string(),
            exit_code,
            stdout.map(|s| truncate_str(&s, 4096)),
            stderr.map(|s| truncate_str(&s, 4096)),
            detail.map(|d| truncate_str(&d, 4096)),
        ),
        Err(e) => (
            "judge_error".to_string(),
            None,
            None,
            None,
            Some(truncate_str(&e.to_string(), 4096)),
        ),
    };

    let _ = send_result(
        state,
        &task,
        TaskResult {
            task_id: task.id,
            callback_id: task.callback_id.clone(),
            kind: task.kind.clone(),
            status,
            exit_code,
            duration_ms: Some(elapsed.as_millis() as i32),
            stdout,
            stderr,
            detail,
        },
    )
    .await;
}

/// exploit 任务执行结果。
struct TaskOutcome {
    success: bool,
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    detail: Option<String>,
}

/// 运行 GameBox 的 awdp exploit 脚本，判定目标是否可被攻破。
///
/// 契约（与平台侧 script_runner 一致）：
/// - 脚本以 `python3 <script> <target_ip>` 执行；
/// - stdout 输出批量 JSON：`[{"success": true, "gamebox_ip": "ip1", ...}, ...]`；
/// - 命中 `gamebox_ip == target_ip`（或空 IP 兜底）的行作为目标结果。
async fn execute_exploit_task(state: &AppState, task: &JudgeTask) -> Result<TaskOutcome, String> {
    let script_content = task
        .script_content
        .as_deref()
        .ok_or_else(|| "exploit task missing script_content".to_string())?;

    let script_path = format!("{}/exploit_{}.py", state.work_dir, task.id);
    tokio::fs::write(&script_path, script_content)
        .await
        .map_err(|e| format!("script write error: {e}"))?;
    let _ = Command::new("chmod")
        .args(["+x", &script_path])
        .output()
        .await;

    // 参数模板中的 {target_ip} 替换为实际目标。
    let args: Vec<String> = task
        .script_args_json
        .as_ref()
        .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|arg| arg.replace("{target_ip}", &task.target_ip))
        .collect();
    let args = if args.is_empty() {
        vec![task.target_ip.clone()]
    } else {
        args
    };

    let output = tokio::time::timeout(
        Duration::from_secs(task.timeout_secs),
        // 统一用 python3 解释执行（与平台侧 script_runner 契约一致：
        // 脚本内容不要求 shebang；exploit 脚本均为 python）。
        // env 白名单：只透传 PATH/HOME/LANG 与 JUDGE_* 前缀变量，
        // 其余宿主环境（含 INTERNAL_TOKEN 等敏感项）对脚本一律不可见。
        Command::new("python3")
            .arg(&script_path)
            .args(&args)
            .env_clear()
            .envs(build_script_env(env::vars()))
            .output(),
    )
    .await
    .map_err(|_| "exploit timed out".to_string())
    .and_then(|res| res.map_err(|e| format!("exploit process error: {e}")))?;

    let _ = tokio::fs::remove_file(&script_path).await;

    let exit_code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // 解析批量 JSON 结果，命中目标行。
    let (success, detail) = match parse_batch_results(&stdout) {
        Ok(rows) => {
            let hit = rows
                .iter()
                .find(|r| r.ip == task.target_ip || r.ip.is_empty())
                .or_else(|| rows.first());
            match hit {
                Some(r) => (
                    r.success,
                    Some(r.error.clone().unwrap_or_else(|| {
                        format!(
                            "{}: {}",
                            r.ip,
                            if r.success { "VULNERABLE" } else { "FAIL" }
                        )
                    })),
                ),
                None => (false, Some("exploit 未返回该目标结果".to_string())),
            }
        }
        Err(e) => {
            // 解析失败回退退出码：0 = 全部成功。
            let ok = exit_code == Some(0) && stderr.is_empty();
            let detail = format!("exploit 输出解析失败: {e}; exit={:?}", exit_code);
            (ok, Some(detail))
        }
    };

    Ok(TaskOutcome {
        success,
        exit_code,
        stdout: Some(stdout),
        stderr: Some(stderr),
        detail,
    })
}

/// HTTP GET 目标 flag 端点，验证返回体包含预期 flag。
async fn execute_flag_task(state: &AppState, task: &JudgeTask) -> Result<TaskOutcome, String> {
    let url = task
        .flag_url
        .as_deref()
        .ok_or_else(|| "flag task missing flag_url".to_string())?;
    let expected = task
        .expected_flag
        .as_deref()
        .ok_or_else(|| "flag task missing expected_flag".to_string())?;

    let response = tokio::time::timeout(
        Duration::from_secs(task.timeout_secs),
        state.client.get(url).send(),
    )
    .await
    .map_err(|_| "flag curl timed out".to_string())
    .and_then(|res| res.map_err(|e| format!("flag curl error: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("flag curl body read error: {e}"))?;
    let body_trimmed = body.trim().to_string();

    let success = status.is_success() && !expected.is_empty() && body_trimmed == expected;
    let success = success || (!expected.is_empty() && body_trimmed.contains(expected));
    let detail = if success {
        Some("flag 端点返回预期 flag".to_string())
    } else {
        Some(format!(
            "HTTP {status}; body={:?}（期望 flag={:?}）",
            truncate_str(&body_trimmed, 200),
            truncate_str(expected, 64)
        ))
    };

    Ok(TaskOutcome {
        success,
        exit_code: Some(status.as_u16() as i32),
        stdout: Some(body_trimmed),
        stderr: None,
        detail,
    })
}

/// 解析批量 JSON 输出：`[{"success": bool, "gamebox_ip"|"ip": "...", "error": "..."}]`
/// 或单对象 `{"success": bool, "error": "..."}`。
fn parse_batch_results(stdout: &str) -> Result<Vec<BatchRow>, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err("empty output".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|e| format!("invalid JSON: {e}"))?;

    let rows = match value {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(_) => vec![value],
        _ => return Err("expected JSON array or object".into()),
    };

    let mut out = Vec::with_capacity(rows.len());
    for item in rows {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let success = obj
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let ip = obj
            .get("gamebox_ip")
            .or_else(|| obj.get("ip"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let error = obj.get("error").and_then(|v| v.as_str()).map(String::from);
        out.push(BatchRow { ip, success, error });
    }
    if out.is_empty() {
        return Err("no result rows found".into());
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct BatchRow {
    ip: String,
    success: bool,
    error: Option<String>,
}

// ── Callback 重试（与 AWD JudgeServer 同策略）──

/// 回调最大尝试次数：初始 1 次 + 3 次重试。
const CALLBACK_MAX_ATTEMPTS: usize = 4;
/// 指数退避间隔（秒）：第 2 / 3 / 4 次尝试前分别等待 1s / 2s / 4s。
const CALLBACK_RETRY_DELAYS_SECS: [u64; 3] = [1, 2, 4];

/// 第 `attempt` 次尝试前的退避等待时长（attempt 从 1 开始；首次尝试不等待）。
fn callback_retry_delay(attempt: usize) -> Duration {
    Duration::from_secs(CALLBACK_RETRY_DELAYS_SECS[attempt - 2])
}

/// Judge 脚本执行的最小 env 白名单（安全边界，与 AWD JudgeServer 一致）。
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
/// 平台侧 judge_callback 已按 callback_id/task_id 幂等，重复投递不会重复记录；
/// 绝不能为每次重试生成新 ID。
async fn send_result(state: &AppState, task: &JudgeTask, result: TaskResult) -> Result<(), String> {
    let url = format!(
        "{}/internal/awdp/practice/judge/callback",
        state.platform_url
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
    fn parse_batch_results_accepts_array_and_single_object() {
        let array = r#"[{"success": true, "gamebox_ip": "10.42.2.10"},
                         {"success": false, "gamebox_ip": "10.42.2.11", "error": "boom"}]"#;
        let rows = parse_batch_results(array).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].success);
        assert_eq!(rows[0].ip, "10.42.2.10");
        assert!(!rows[1].success);
        assert_eq!(rows[1].error.as_deref(), Some("boom"));

        let single = r#"{"success": true}"#;
        let rows = parse_batch_results(single).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].success);
        assert_eq!(rows[0].ip, "");
    }

    #[test]
    fn parse_batch_results_rejects_garbage() {
        assert!(parse_batch_results("").is_err());
        assert!(parse_batch_results("not json").is_err());
        // 单对象无 success 字段 → 视为 failure 行（不报错，避免误杀输出格式）。
        let rows = parse_batch_results(r#"{"foo": 1}"#).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].success);
        assert!(parse_batch_results("[]").is_err()); // empty array
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
        "FloatCTF AWDP Practice JudgeServer starting on {} (max_concurrent={}, event={})",
        listen_addr,
        max_concurrent,
        state.event_id
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
