//! Pull Worker 核心逻辑：poll loop、claim、heartbeat、result delivery、task execution。
//!
//! 可测试：所有 HTTP 调用通过 `HttpClient` trait 抽象，可在测试中 mock。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

use crate::outcome::{self, OUTPUT_MAX_LEN, build_script_env, truncate_str};
use crate::protocol::*;

// ── HTTP Client abstraction (mockable) ──

/// HTTP 客户端抽象，使 worker 逻辑可脱离真实网络测试。
#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    async fn claim_tasks(
        &self,
        url: &str,
        auth_header: &str,
        body: &JudgeClaimRequest,
    ) -> Result<JudgeClaimResponse, String>;

    async fn send_heartbeat(
        &self,
        url: &str,
        auth_header: &str,
        body: &JudgeHeartbeatRequest,
    ) -> Result<HeartbeatStatus, String>;

    async fn submit_result(
        &self,
        url: &str,
        auth_header: &str,
        body: &JudgeResultRequest,
    ) -> Result<SubmitStatus, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatStatus {
    Ok,
    Stale,
    NotFound,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitStatus {
    Ok,
    Idempotent,
    Stale,
    NotFound,
    Error,
}

// ── Real HTTP client (reqwest) ──

pub struct RealHttpClient {
    client: reqwest::Client,
}

impl Clone for RealHttpClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
        }
    }
}

impl RealHttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }
}

#[async_trait::async_trait]
impl HttpClient for RealHttpClient {
    async fn claim_tasks(
        &self,
        url: &str,
        auth: &str,
        body: &JudgeClaimRequest,
    ) -> Result<JudgeClaimResponse, String> {
        let resp = self
            .client
            .post(url)
            .header("Authorization", auth)
            .json(body)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("network error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {status}: {body}"));
        }

        resp.json().await.map_err(|e| format!("deserialize: {e}"))
    }

    async fn send_heartbeat(
        &self,
        url: &str,
        auth: &str,
        body: &JudgeHeartbeatRequest,
    ) -> Result<HeartbeatStatus, String> {
        let resp = self
            .client
            .post(url)
            .header("Authorization", auth)
            .json(body)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("network error: {e}"))?;

        Ok(match resp.status().as_u16() {
            200..=299 => HeartbeatStatus::Ok,
            409 => HeartbeatStatus::Stale,
            404 => HeartbeatStatus::NotFound,
            _ => HeartbeatStatus::Error,
        })
    }

    async fn submit_result(
        &self,
        url: &str,
        auth: &str,
        body: &JudgeResultRequest,
    ) -> Result<SubmitStatus, String> {
        let resp = self
            .client
            .post(url)
            .header("Authorization", auth)
            .json(body)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("network error: {e}"))?;

        Ok(match resp.status().as_u16() {
            200..=299 => SubmitStatus::Ok,
            409 => SubmitStatus::Stale,
            404 => SubmitStatus::NotFound,
            _ => SubmitStatus::Error,
        })
    }
}

// ── Worker State ──

#[derive(Clone)]
pub struct WorkerState<H: HttpClient + Clone> {
    pub http: Arc<H>,
    pub platform_url: String,
    pub event_id: String,
    pub internal_token: String,
    pub worker_id: String,
    pub concurrency: Arc<Semaphore>,
    pub work_dir: String,
    pub poll_interval: Duration,
    pub heartbeat_interval: Duration,
    pub shutdown: Arc<AtomicBool>,
}

impl<H: HttpClient + Clone> WorkerState<H> {
    fn auth(&self) -> String {
        auth_header(&self.internal_token)
    }
}

// ── Poll Loop ──

/// 后台轮询主循环：claim → spawn → heartbeat → submit → repeat。
pub async fn poll_loop<H: HttpClient + Clone + 'static>(state: WorkerState<H>) {
    tracing::info!("Poll loop started (worker_id={})", state.worker_id);

    loop {
        if state.shutdown.load(Ordering::Relaxed) {
            tracing::info!("Shutdown signal received, poll loop exiting");
            break;
        }

        let available = state.concurrency.available_permits();
        if available == 0 {
            sleep(Duration::from_secs(1)).await;
            continue;
        }

        let url = claim_url(&state.platform_url, &state.event_id);
        let auth = state.auth();
        let body = JudgeClaimRequest {
            worker_id: state.worker_id.clone(),
            limit: available,
        };

        match state.http.claim_tasks(&url, &auth, &body).await {
            Ok(resp) => {
                if resp.tasks.is_empty() {
                    sleep(state.poll_interval).await;
                } else {
                    tracing::info!("Claimed {} tasks", resp.tasks.len());
                    let state_arc = Arc::new(state.clone());
                    for task in resp.tasks {
                        let state_clone = state_arc.clone();
                        let permit = state_clone
                            .concurrency
                            .clone()
                            .acquire_owned()
                            .await
                            .expect("Semaphore closed");
                        tokio::spawn(async move {
                            let _permit = permit;
                            execute_single_task(&state_clone, task).await;
                        });
                    }
                    sleep(Duration::from_millis(100)).await;
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Claim request failed: {} — retrying in {}s",
                    e,
                    state.poll_interval.as_secs()
                );
                sleep(state.poll_interval).await;
            }
        }
    }
}

// ── Task Execution ──

/// 执行单个裁判任务：写脚本文件、子进程运行、心跳、提交结果。
pub async fn execute_single_task<H: HttpClient + Clone + 'static>(state: &Arc<WorkerState<H>>, task: ClaimedTask) {
    tracing::info!("Executing task {} for {}", task.task_id, task.target_ip);

    let script_path = format!("{}/judge_{}.sh", state.work_dir, task.task_id);
    let result_id = Uuid::new_v4().to_string();

    // Write script
    if let Err(e) = tokio::fs::write(&script_path, &task.script_content).await {
        tracing::error!("Failed to write script {}: {}", script_path, e);
        let err_msg = format!("Script write error: {e}");
        let _ = deliver_result(state, &task, "worker_error", None, None, Some(&err_msg), None, &result_id).await;
        return;
    }

    let _ = tokio::process::Command::new("chmod")
        .args(["+x", &script_path])
        .output()
        .await;

    // Start heartbeat
    let heartbeat_handle = {
        let state_clone = state.clone();
        let task_id = task.task_id;
        let worker_id = state.worker_id.clone();
        let attempt = task.attempt;
        let lease_token = task.lease_token.clone();
        let interval = state.heartbeat_interval;

        tokio::spawn(async move {
            heartbeat_loop(&state_clone, task_id, &worker_id, attempt, &lease_token, interval).await
        })
    };

    // Build args
    let args: Vec<String> = task
        .script_args_json
        .as_ref()
        .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|arg| arg.replace("{target_ip}", &task.target_ip))
        .collect();

    let start = Instant::now();

    let exec_result = tokio::time::timeout(
        Duration::from_secs(task.timeout_secs as u64),
        tokio::process::Command::new(&script_path)
            .args(&args)
            .env_clear()
            .envs(build_script_env(std::env::vars()))
            .output(),
    )
    .await;

    let elapsed = start.elapsed();
    heartbeat_handle.abort();

    let (outcome, exit_code, stdout, stderr) = match exec_result {
        Ok(Ok(output)) => {
            let ec = output.status.code().unwrap_or(-1);
            let out = String::from_utf8_lossy(&output.stdout).to_string();
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            let outcome = outcome::exit_code_to_outcome(ec);
            (outcome, Some(ec), Some(truncate_str(&out, OUTPUT_MAX_LEN)), Some(truncate_str(&err, OUTPUT_MAX_LEN)))
        }
        Ok(Err(e)) => {
            tracing::error!("Task {} spawn failed: {}", task.task_id, e);
            ("worker_error", None, None, Some(format!("Execution error: {e}")))
        }
        Err(_timeout) => {
            tracing::warn!("Task {} timed out", task.task_id);
            ("target_timeout", None, None, Some("Execution timed out".to_string()))
        }
    };

    let duration_ms = Some(elapsed.as_millis() as i32);

    let _ = deliver_result(
        state, &task, outcome, exit_code,
        stdout.as_deref(), stderr.as_deref(), duration_ms, &result_id,
    ).await;

    let _ = tokio::fs::remove_file(&script_path).await;
}

// ── Heartbeat Loop ──

/// Heartbeat loop for a single task. Runs until aborted (task execution completes).
pub async fn heartbeat_loop<H: HttpClient + Clone>(
    state: &WorkerState<H>,
    task_id: Uuid,
    worker_id: &str,
    attempt: i32,
    lease_token: &str,
    interval: Duration,
) {
    sleep(interval).await;

    let url = heartbeat_url(&state.platform_url, &state.event_id, &task_id);
    let auth = state.auth();

    loop {
        let body = JudgeHeartbeatRequest {
            worker_id: worker_id.to_string(),
            attempt,
            lease_token: lease_token.to_string(),
        };

        match state.http.send_heartbeat(&url, &auth, &body).await {
            Ok(HeartbeatStatus::Ok) => {
                tracing::debug!("Heartbeat OK for task {}", task_id);
            }
            Ok(HeartbeatStatus::Stale) => {
                tracing::warn!("Heartbeat 409 stale for task {} — ownership lost", task_id);
                return;
            }
            Ok(HeartbeatStatus::NotFound) => {
                tracing::warn!("Heartbeat 404 for task {} — task not found", task_id);
                return;
            }
            Ok(HeartbeatStatus::Error) | Err(_) => {
                tracing::warn!("Heartbeat failed for task {} — will retry", task_id);
            }
        }

        sleep(interval).await;
    }
}

// ── Result Delivery ──

const RESULT_MAX_ATTEMPTS: usize = 4;
const RESULT_RETRY_DELAYS_SECS: [u64; 3] = [1, 2, 4];

/// 将任务结果 POST 回平台，失败时指数退避重试，复用同一 `result_id`。
pub async fn deliver_result<H: HttpClient + Clone>(
    state: &WorkerState<H>,
    task: &ClaimedTask,
    outcome: &str,
    exit_code: Option<i32>,
    stdout: Option<&str>,
    stderr: Option<&str>,
    duration_ms: Option<i32>,
    result_id: &str,
) -> Result<(), String> {
    let url = result_url(&state.platform_url, &state.event_id, &task.task_id);
    let auth = state.auth();
    let mut last_error = String::new();

    for attempt in 1..=RESULT_MAX_ATTEMPTS {
        if attempt > 1 {
            let delay = Duration::from_secs(RESULT_RETRY_DELAYS_SECS[attempt - 2]);
            tracing::warn!(
                "Result for task {} failed, retrying in {}s (attempt {}/{})",
                task.task_id, delay.as_secs(), attempt, RESULT_MAX_ATTEMPTS
            );
            sleep(delay).await;
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

        match state.http.submit_result(&url, &auth, &body).await {
            Ok(SubmitStatus::Ok) | Ok(SubmitStatus::Idempotent) => {
                tracing::info!("Result OK for task {} (outcome={})", task.task_id, outcome);
                return Ok(());
            }
            Ok(SubmitStatus::Stale) => {
                tracing::warn!("Result 409 stale for task {} — discarding", task.task_id);
                return Err("stale".to_string());
            }
            Ok(SubmitStatus::NotFound) => {
                tracing::warn!("Result 404 for task {} — stopping", task.task_id);
                return Err("not_found".to_string());
            }
            Ok(SubmitStatus::Error) => {
                last_error = "HTTP error".to_string();
                tracing::warn!("Result HTTP error for task {} (attempt {}/{})", task.task_id, attempt, RESULT_MAX_ATTEMPTS);
            }
            Err(e) => {
                last_error = format!("network error: {}", e);
                tracing::warn!("Result network error for task {}: {} (attempt {}/{})", task.task_id, e, attempt, RESULT_MAX_ATTEMPTS);
            }
        }
    }

    tracing::error!("Result permanently failed for task {} after {} attempts: {}", task.task_id, RESULT_MAX_ATTEMPTS, last_error);
    Err(last_error)
}

// ── Concurrency helpers (pure) ──

/// 计算本次 claim 应使用的 limit（不超过可用 permits）。
pub fn claim_limit(available_permits: usize, max_claim: usize) -> usize {
    available_permits.min(max_claim)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ── Mock HTTP Client ──

    #[derive(Clone)]
    struct MockHttp {
        claim_responses: Arc<Mutex<Vec<Result<JudgeClaimResponse, String>>>>,
        heartbeat_responses: Arc<Mutex<Vec<Result<HeartbeatStatus, String>>>>,
        result_responses: Arc<Mutex<Vec<Result<SubmitStatus, String>>>>,
        claim_call_count: Arc<Mutex<usize>>,
        heartbeat_calls: Arc<Mutex<Vec<JudgeHeartbeatRequest>>>,
        result_calls: Arc<Mutex<Vec<JudgeResultRequest>>>,
    }

    impl MockHttp {
        fn new() -> Self {
            Self {
                claim_responses: Arc::new(Mutex::new(vec![])),
                heartbeat_responses: Arc::new(Mutex::new(vec![])),
                result_responses: Arc::new(Mutex::new(vec![])),
                claim_call_count: Arc::new(Mutex::new(0)),
                heartbeat_calls: Arc::new(Mutex::new(vec![])),
                result_calls: Arc::new(Mutex::new(vec![])),
            }
        }

        fn set_claim_responses(&self, responses: Vec<Result<JudgeClaimResponse, String>>) {
            *self.claim_responses.lock().unwrap() = responses;
        }

        fn set_heartbeat_responses(&self, responses: Vec<Result<HeartbeatStatus, String>>) {
            *self.heartbeat_responses.lock().unwrap() = responses;
        }

        fn set_result_responses(&self, responses: Vec<Result<SubmitStatus, String>>) {
            *self.result_responses.lock().unwrap() = responses;
        }

        fn claim_call_count(&self) -> usize {
            *self.claim_call_count.lock().unwrap()
        }

        fn heartbeat_calls_snapshot(&self) -> Vec<JudgeHeartbeatRequest> {
            self.heartbeat_calls.lock().unwrap().clone()
        }

        fn result_calls_snapshot(&self) -> Vec<JudgeResultRequest> {
            self.result_calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl HttpClient for MockHttp {
        async fn claim_tasks(&self, _url: &str, _auth: &str, _body: &JudgeClaimRequest) -> Result<JudgeClaimResponse, String> {
            *self.claim_call_count.lock().unwrap() += 1;
            self.claim_responses.lock().unwrap().pop().unwrap_or(Ok(JudgeClaimResponse { tasks: vec![] }))
        }

        async fn send_heartbeat(&self, _url: &str, _auth: &str, body: &JudgeHeartbeatRequest) -> Result<HeartbeatStatus, String> {
            self.heartbeat_calls.lock().unwrap().push(body.clone());
            self.heartbeat_responses.lock().unwrap().pop().unwrap_or(Ok(HeartbeatStatus::Ok))
        }

        async fn submit_result(&self, _url: &str, _auth: &str, body: &JudgeResultRequest) -> Result<SubmitStatus, String> {
            self.result_calls.lock().unwrap().push(body.clone());
            self.result_responses.lock().unwrap().pop().unwrap_or(Ok(SubmitStatus::Ok))
        }
    }

    fn make_task(task_id: &str, attempt: i32) -> ClaimedTask {
        ClaimedTask {
            task_id: Uuid::parse_str(task_id).unwrap(),
            batch_id: Uuid::nil(),
            event_id: Uuid::nil(),
            round_id: Uuid::nil(),
            gamebox_instance_id: Uuid::nil(),
            event_gamebox_id: None,
            team_id: Uuid::nil(),
            attempt,
            lease_token: "test-lease-token".into(),
            lease_expires_at: "2026-01-01T00:00:00Z".into(),
            deadline_at: "2026-01-01T00:05:00Z".into(),
            script_content: "#!/bin/bash\necho ok".into(),
            script_args_json: None,
            target_ip: "10.0.0.1".into(),
            timeout_secs: 5,
        }
    }

    fn make_state(mock: MockHttp, max_concurrent: usize) -> WorkerState<MockHttp> {
        WorkerState {
            http: Arc::new(mock),
            platform_url: "http://test".into(),
            event_id: "evt".into(),
            internal_token: "tok".into(),
            worker_id: "w1".into(),
            concurrency: Arc::new(Semaphore::new(max_concurrent)),
            work_dir: "/tmp/judge_test".into(),
            poll_interval: Duration::from_millis(10),
            heartbeat_interval: Duration::from_millis(10),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    // ── Claim limit tests ──

    #[test]
    fn claim_limit_when_available_less_than_max() {
        assert_eq!(claim_limit(3, 20), 3);
    }

    #[test]
    fn claim_limit_capped_at_max() {
        assert_eq!(claim_limit(50, 20), 20);
    }

    #[test]
    fn claim_limit_zero_when_no_permits() {
        assert_eq!(claim_limit(0, 20), 0);
    }

    // ── Poll loop: no tasks → sleeps ──

    #[tokio::test]
    async fn poll_loop_sleeps_when_no_tasks() {
        let mock = MockHttp::new();
        mock.set_claim_responses(vec![Ok(JudgeClaimResponse { tasks: vec![] })]);
        let state = make_state(mock.clone(), 4);

        // Run poll for a short burst
        let shutdown = state.shutdown.clone();
        let poll_handle = tokio::spawn(async move {
            poll_loop(state).await;
        });

        // Give it time to do one poll iteration
        tokio::time::sleep(Duration::from_millis(50)).await;
        shutdown.store(true, Ordering::Relaxed);
        let _ = tokio::time::timeout(Duration::from_secs(1), poll_handle).await;

        assert!(mock.claim_call_count() >= 1);
    }

    // ── Poll loop: claim failure → survives ──

    #[tokio::test]
    async fn poll_loop_survives_claim_failure() {
        let mock = MockHttp::new();
        mock.set_claim_responses(vec![Err("network error".into())]);
        let state = make_state(mock.clone(), 4);

        let shutdown = state.shutdown.clone();
        let poll_handle = tokio::spawn(async move {
            poll_loop(state).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        shutdown.store(true, Ordering::Relaxed);
        let _ = tokio::time::timeout(Duration::from_secs(1), poll_handle).await;

        // Should have called claim at least once without panicking
        assert!(mock.claim_call_count() >= 1);
    }

    // ── Heartbeat: sends correct fields ──

    #[tokio::test]
    async fn heartbeat_sends_correct_fields() {
        let mock = MockHttp::new();
        mock.set_heartbeat_responses(vec![Ok(HeartbeatStatus::Ok)]);
        let state = make_state(mock.clone(), 4);
        let task_id = Uuid::new_v4();

        let hb_handle = tokio::spawn(async move {
            heartbeat_loop(&state, task_id, "w1", 1, "lease-tok", Duration::from_millis(5)).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        hb_handle.abort();

        let calls = mock.heartbeat_calls_snapshot();
        assert!(!calls.is_empty(), "Expected at least one heartbeat call");
        let hb = &calls[0];
        assert_eq!(hb.worker_id, "w1");
        assert_eq!(hb.attempt, 1);
        assert_eq!(hb.lease_token, "lease-tok");
    }

    // ── Heartbeat: 409 stale → exits loop ──

    #[tokio::test]
    async fn heartbeat_409_stale_exits_loop() {
        let mock = MockHttp::new();
        mock.set_heartbeat_responses(vec![Ok(HeartbeatStatus::Stale)]);
        let state = make_state(mock.clone(), 4);
        let task_id = Uuid::new_v4();

        let hb_handle = tokio::spawn(async move {
            heartbeat_loop(&state, task_id, "w1", 1, "lease-tok", Duration::from_millis(5)).await;
        });

        // Should complete quickly (stale → exit)
        let result = tokio::time::timeout(Duration::from_millis(100), hb_handle).await;
        assert!(result.is_ok(), "Heartbeat loop should exit on stale");

        // Only one heartbeat call made
        let calls = mock.heartbeat_calls_snapshot();
        assert_eq!(calls.len(), 1);
    }

    // ── Heartbeat: transient error → retries ──

    #[tokio::test]
    async fn heartbeat_transient_error_retries() {
        let mock = MockHttp::new();
        // First call: error, then Ok
        mock.set_heartbeat_responses(vec![
            Ok(HeartbeatStatus::Ok),
            Ok(HeartbeatStatus::Error),
        ]);
        let state = make_state(mock.clone(), 4);
        let task_id = Uuid::new_v4();

        let hb_handle = tokio::spawn(async move {
            heartbeat_loop(&state, task_id, "w1", 1, "lease-tok", Duration::from_millis(5)).await;
        });

        tokio::time::sleep(Duration::from_millis(30)).await;
        hb_handle.abort();

        let calls = mock.heartbeat_calls_snapshot();
        assert!(calls.len() >= 2, "Should have made multiple heartbeat calls after error");
    }

    // ── Result: stable result_id across retries ──

    #[tokio::test]
    async fn result_retries_use_same_result_id() {
        let mock = MockHttp::new();
        // First two calls fail, third succeeds
        mock.set_result_responses(vec![
            Ok(SubmitStatus::Ok),
            Ok(SubmitStatus::Error),
            Ok(SubmitStatus::Error),
        ]);
        let state = make_state(mock.clone(), 4);
        let task = make_task("00000000-0000-0000-0000-000000000001", 1);

        let result = deliver_result(
            &state, &task, "up", Some(0), Some("ok"), None, Some(100), "my-stable-result-id",
        ).await;

        assert!(result.is_ok());
        let calls = mock.result_calls_snapshot();
        assert_eq!(calls.len(), 3);
        // All calls have the same result_id
        for call in &calls {
            assert_eq!(call.result_id, "my-stable-result-id");
        }
    }

    // ── Result: 409 stale → stops immediately ──

    #[tokio::test]
    async fn result_409_stale_stops_immediately() {
        let mock = MockHttp::new();
        mock.set_result_responses(vec![Ok(SubmitStatus::Stale)]);
        let state = make_state(mock.clone(), 4);
        let task = make_task("00000000-0000-0000-0000-000000000001", 1);

        let result = deliver_result(
            &state, &task, "up", Some(0), None, None, Some(100), "rid",
        ).await;

        assert!(result.is_err());
        let calls = mock.result_calls_snapshot();
        assert_eq!(calls.len(), 1, "Should stop after first 409");
    }

    // ── Result: 404 → stops immediately ──

    #[tokio::test]
    async fn result_404_not_found_stops() {
        let mock = MockHttp::new();
        mock.set_result_responses(vec![Ok(SubmitStatus::NotFound)]);
        let state = make_state(mock.clone(), 4);
        let task = make_task("00000000-0000-0000-0000-000000000001", 1);

        let result = deliver_result(
            &state, &task, "up", Some(0), None, None, Some(100), "rid",
        ).await;

        assert!(result.is_err());
        let calls = mock.result_calls_snapshot();
        assert_eq!(calls.len(), 1, "Should stop after first 404");
    }

    // ── Result: 200 → stops immediately ──

    #[tokio::test]
    async fn result_200_stops_immediately() {
        let mock = MockHttp::new();
        mock.set_result_responses(vec![Ok(SubmitStatus::Ok)]);
        let state = make_state(mock.clone(), 4);
        let task = make_task("00000000-0000-0000-0000-000000000001", 1);

        let result = deliver_result(
            &state, &task, "up", Some(0), None, None, Some(100), "rid",
        ).await;

        assert!(result.is_ok());
        let calls = mock.result_calls_snapshot();
        assert_eq!(calls.len(), 1, "Should stop after first 200");
    }

    // ── Result: all retries exhausted → error ──

    #[tokio::test]
    async fn result_all_retries_exhausted() {
        let mock = MockHttp::new();
        // All 4 attempts fail
        mock.set_result_responses(vec![
            Ok(SubmitStatus::Error),
            Ok(SubmitStatus::Error),
            Ok(SubmitStatus::Error),
            Ok(SubmitStatus::Error),
        ]);
        let state = make_state(mock.clone(), 4);
        let task = make_task("00000000-0000-0000-0000-000000000001", 1);

        let result = deliver_result(
            &state, &task, "up", Some(0), None, None, Some(100), "rid",
        ).await;

        assert!(result.is_err());
        let calls = mock.result_calls_snapshot();
        assert_eq!(calls.len(), RESULT_MAX_ATTEMPTS);
    }
}