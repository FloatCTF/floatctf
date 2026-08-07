//! AWD JudgeServer — standalone worker for executing health-check scripts.
//!
//! # Architecture
//!
//! 1. Platform sends judge batches (list of tasks) to JudgeServer
//! 2. JudgeServer executes each task's script locally as a subprocess
//! 3. Script receives target IP via command-line args
//! 4. Script checks the target GameBox's services over the network
//! 5. Each task result is immediately POSTed back to the platform
//!
//! # Configuration (env vars)
//!
//! - `PLATFORM_INTERNAL_URL` — base URL of the FloatCTF platform
//! - `EVENT_ID` — UUID of the AWD event
//! - `INTERNAL_TOKEN` — Bearer token for platform auth
//! - `LISTEN_ADDR` — bind address (default "0.0.0.0:8082")
//! - `MAX_CONCURRENT` — max concurrent script executions (default 5)
//! - `WORK_DIR` — directory for temporary script files (default "/tmp/judge")

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

/// POST /batch — receive and execute a batch of judge tasks.
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

/// Execute one judge task: write script to file, run as subprocess, callback.
async fn execute_single_task(state: &AppState, task: JudgeTask) {
    tracing::info!("Executing task {} for {}", task.id, task.target_ip);

    let script_path = format!("{}/judge_{}.sh", state.work_dir, task.id);

    // Write script to temp file
    if let Err(e) = tokio::fs::write(&script_path, &task.script_content).await {
        tracing::error!("Failed to write script {}: {}", script_path, e);
        send_result(
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
            Command::new(&script_path).args(&args).output(),
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

                send_result(
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
                    send_result(
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
                    send_result(
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

/// POST the task result back to the platform.
async fn send_result(state: &AppState, task: &JudgeTask, result: TaskResult) {
    let url = format!(
        "{}/internal/awd/events/{}/judge/callback",
        state.platform_url, state.event_id
    );

    let res = state
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", state.internal_token))
        .json(&result)
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    match res {
        Ok(resp) => {
            if resp.status().is_success() {
                tracing::info!("Callback OK for task {}", task.id);
            } else {
                tracing::warn!(
                    "Callback failed for task {}: HTTP {}",
                    task.id,
                    resp.status()
                );
            }
        }
        Err(e) => {
            tracing::error!("Callback network error for task {}: {}", task.id, e);
        }
    }
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
}

/// GET /health — liveness check.
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
