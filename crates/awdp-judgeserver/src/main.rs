//! FloatCTF AWDP Practice JudgeServer — Pull + Lease 评估 worker + Data Plane API。
//!
//! # 架构（plan §12-§19 / §23-§26）
//!
//! JudgeServer 是**主动拉取**的 worker，不再接收平台 push 批次：
//!
//! ```text
//!   FloatCTF API (control network)
//!        │  POST /internal/awdp/judge/jobs/claim        （领取，含 lease token）
//!        │  POST /internal/awdp/judge/jobs/{id}/heartbeat（延长 lease）
//!        │  POST /internal/awdp/judge/jobs/{id}/result   （提交结果）
//!        ▲
//!   awdp-judgeserver（pull loop + bounded concurrency）
//!        │  data network（fctf-awdp-practice）
//!        ▼
//!   GameBox 容器（内网 IP）——healthcheck / judge / exploit 都走容器内网地址
//! ```
//!
//! 每个 job 执行：internal Healthcheck（`target_ip:container_port`）→ Judge
//! （check.py）→ Official 才有 Exploit（exploit.py）。Manual 绝不执行 exploit。
//!
//! 结果语义（plan §25/§26）：脚本正常返回的业务失败 → 业务终态；脚本 spawn 失败 /
//! 超时 / 输出畸形 / 协议违规 → `platform_error`（平台侧释放重试，绝不判玩家失败）。
//!
//! Data Plane（对 GameBox 可达，仅极少数端点）：
//!   - `GET /healthz`
//!   - `GET /flag`（Phase D：Break flag，按 TCP source IP 识别实例）
//!
//! # 配置（环境变量）
//!
//! - `PLATFORM_INTERNAL_URL` — FloatCTF 平台基址（control network 可达）
//! - `INTERNAL_TOKEN` — 平台鉴权 Bearer 令牌（HKDF 派生，与平台侧一致）
//! - `WORKER_ID` — worker 标识（默认 `practice-judge-01`）
//! - `DATA_LISTEN_ADDR` — data plane 监听（默认 `0.0.0.0:8080`）
//! - `CLAIM_BATCH` — 单次领取上限（默认 16）
//! - `MAX_CONCURRENCY` — 最大并发 job 数（默认 8）
//! - `POLL_INTERVAL_SECS` — 空转轮询间隔（默认 5）
//! - `HEARTBEAT_INTERVAL_SECS` — 心跳间隔（默认 30，须明显小于 lease）
//! - `WORK_DIR` — 临时脚本目录（默认 `/tmp/judge`）

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, middleware::Logger, web};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::{Duration, Instant};

// ── Platform API types（与 apps/api 侧 judge_worker 对齐）──

#[derive(Debug, Serialize)]
struct ClaimRequest {
    worker_id: String,
    capacity: u64,
}

#[derive(Debug, Deserialize)]
struct ClaimResponse {
    lease_seconds: i64,
    jobs: Vec<ClaimedJob>,
}

#[derive(Debug, Deserialize, Clone)]
struct ClaimedJob {
    evaluation_id: uuid::Uuid,
    attempt: i32,
    lease_token: String,
    kind: String, // "manual" | "official"
    run_id: uuid::Uuid,
    round_id: Option<uuid::Uuid>,
    instance_id: uuid::Uuid,
    runtime_generation: i64,
    target_ip: Option<String>,
    healthchecks: Vec<serde_json::Value>,
    judge_script: Option<String>,
    exploit_script: Option<String>,
}

#[derive(Debug, Serialize)]
struct HeartbeatRequest {
    lease_token: String,
    worker_id: String,
}

#[derive(Debug, Serialize)]
struct ResultRequest {
    evaluation_id: uuid::Uuid,
    worker_id: String,
    lease_token: String,
    attempt: i32,
    runtime_generation: i64,
    status: String,
    healthcheck_result: Option<String>,
    judge_result: Option<String>,
    exploit_result: Option<String>,
    stdout_limited: Option<String>,
    stderr_limited: Option<String>,
}

// ── App State ──

#[derive(Clone)]
struct AppState {
    client: Client,
    platform_url: String,
    internal_token: String,
    worker_id: String,
    claim_batch: u64,
    concurrency: Arc<Semaphore>,
    poll_interval: Duration,
    heartbeat_interval: Duration,
    work_dir: String,
    inflight: Arc<AtomicUsize>,
}

// ── 结果状态 ──

/// 业务终态（玩家域内判定）。
const ST_SERVICE_DOWN: &str = "service_down";
const ST_FUNCTIONAL_BROKEN: &str = "functional_broken";
const ST_VULNERABLE: &str = "vulnerable";
const ST_PATCHED: &str = "patched";
/// 基础设施失败（可重试，平台侧释放回 pending）。
const ST_PLATFORM_ERROR: &str = "platform_error";

/// 执行输出解析后的判定。
enum JobOutcome {
    /// 业务终态 + 详情（含截断后的 stdout/stderr 供展示）。
    Business(
        &'static str,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
    /// 基础设施失败（spawn/超时/畸形输出/协议违规）→ platform_error。
    Infrastructure(String),
}

struct ExecResult {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    rows: Vec<BatchRow>,
    parse_error: Option<String>,
}

impl ExecResult {
    /// 截断后的 stdout/stderr（用于结果 detail 展示，不泄露完整脚本输出）。
    fn truncated(&self) -> (Option<String>, Option<String>) {
        let out = if self.stdout.trim().is_empty() {
            None
        } else {
            Some(truncate_str(&self.stdout, 4096))
        };
        let err = if self.stderr.trim().is_empty() {
            None
        } else {
            Some(truncate_str(&self.stderr, 4096))
        };
        (out, err)
    }
}

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

/// Judge 脚本执行的最小 env 白名单（安全边界）：只保留 PATH/HOME/LANG 与 JUDGE_* 前缀，
/// 其余宿主环境（含 INTERNAL_TOKEN 等敏感项）对任意脚本内容一律不可见。
fn build_script_env(host_env: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    host_env
        .filter(|(key, _)| {
            key == "PATH" || key == "HOME" || key == "LANG" || key.starts_with("JUDGE_")
        })
        .collect()
}

// ── 脚本执行 ──

/// 运行 check.py / exploit.py：`python3 <script> <target_ip>`，env 白名单，超时限制。
async fn run_target_script(
    state: &AppState,
    script_content: &str,
    target_ip: &str,
    timeout_secs: u64,
    _max_stdout: usize,
) -> Result<ExecResult, String> {
    let script_path = format!(
        "{}/script_{}_{}.py",
        state.work_dir,
        target_ip.replace('.', "_"),
        uuid::Uuid::new_v4()
    );
    tokio::fs::write(&script_path, script_content)
        .await
        .map_err(|e| format!("script write error: {e}"))?;
    let _ = Command::new("chmod")
        .args(["+x", &script_path])
        .output()
        .await;

    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new("python3")
            .arg(&script_path)
            .arg(target_ip)
            .env_clear()
            .envs(build_script_env(env::vars()))
            .output(),
    )
    .await
    .map_err(|_| "script timed out".to_string())
    .and_then(|res| res.map_err(|e| format!("script process error: {e}")))?;

    let _ = tokio::fs::remove_file(&script_path).await;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let rows = match parse_batch_results(&stdout) {
        Ok(rows) => rows,
        Err(e) => {
            return Ok(ExecResult {
                stdout,
                stderr,
                exit_code: output.status.code(),
                rows: vec![],
                parse_error: Some(e),
            });
        }
    };
    Ok(ExecResult {
        stdout,
        stderr,
        exit_code: output.status.code(),
        rows,
        parse_error: None,
    })
}

fn hit_row<'a>(rows: &'a [BatchRow], target_ip: &str) -> Option<&'a BatchRow> {
    rows.iter()
        .find(|r| r.ip == target_ip || r.ip.is_empty())
        .or_else(|| rows.first())
}

// ── job 执行管线 ──

/// 执行单个 job：internal healthcheck → judge → （official）exploit。
async fn execute_job(state: Arc<AppState>, job: ClaimedJob, hb_interval: Duration) {
    let _permit = state.concurrency.clone().acquire_owned().await;
    state.inflight.fetch_add(1, Ordering::SeqCst);
    tracing::info!(
        evaluation_id = %job.evaluation_id,
        kind = %job.kind,
        instance_id = %job.instance_id,
        run_id = %job.run_id,
        round_id = ?job.round_id,
        attempt = job.attempt,
        "job claimed"
    );

    let started = Instant::now();

    // 心跳任务（job 执行期间周期延长 lease）。
    let hb_state = state.clone();
    let hb_job = job.clone();
    let heartbeat_task = actix_web::rt::spawn(async move {
        loop {
            actix_web::rt::time::sleep(hb_interval).await;
            let url = format!(
                "{}/internal/awdp/judge/jobs/{}/heartbeat",
                hb_state.platform_url, hb_job.evaluation_id
            );
            let body = HeartbeatRequest {
                lease_token: hb_job.lease_token.clone(),
                worker_id: hb_state.worker_id.clone(),
            };
            match hb_state
                .client
                .post(&url)
                .bearer_auth(&hb_state.internal_token)
                .json(&body)
                .timeout(Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    tracing::debug!(evaluation_id = %hb_job.evaluation_id, "heartbeat ok");
                }
                Ok(resp) => {
                    tracing::warn!(
                        evaluation_id = %hb_job.evaluation_id,
                        status = %resp.status(),
                        "heartbeat rejected"
                    );
                }
                Err(e) => {
                    tracing::warn!(evaluation_id = %hb_job.evaluation_id, error = %e, "heartbeat error");
                }
            }
        }
    });

    let outcome = run_job(&state, &job).await;
    heartbeat_task.abort();

    let elapsed = started.elapsed();
    let (status, health_detail, judge_detail, exploit_detail, stdout, stderr) = match outcome {
        JobOutcome::Business(status, h, j, e, out, err) => (status, h, j, e, out, err),
        JobOutcome::Infrastructure(msg) => {
            (ST_PLATFORM_ERROR, None, None, None, None, Some(msg.clone()))
        }
    };

    let result = ResultRequest {
        evaluation_id: job.evaluation_id,
        worker_id: state.worker_id.clone(),
        lease_token: job.lease_token.clone(),
        attempt: job.attempt,
        runtime_generation: job.runtime_generation,
        status: status.to_string(),
        healthcheck_result: health_detail,
        judge_result: judge_detail,
        exploit_result: exploit_detail,
        stdout_limited: stdout,
        stderr_limited: stderr,
    };

    // 提交结果（带重试；stale 结果由平台 409 拒绝——正常路径无需处理）。
    let url = format!(
        "{}/internal/awdp/judge/jobs/{}/result",
        state.platform_url, job.evaluation_id
    );
    let mut last_error = String::new();
    for attempt_retry in 1..=4usize {
        if attempt_retry > 1 {
            tokio::time::sleep(Duration::from_secs(1 << (attempt_retry - 2))).await;
        }
        match state
            .client
            .post(&url)
            .bearer_auth(&state.internal_token)
            .json(&result)
            .timeout(Duration::from_secs(15))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(
                    evaluation_id = %job.evaluation_id,
                    status,
                    duration_ms = elapsed.as_millis() as i64,
                    "result posted"
                );
                break;
            }
            Ok(resp) => {
                last_error = format!("HTTP {}", resp.status());
                tracing::warn!(
                    evaluation_id = %job.evaluation_id,
                    status = %resp.status(),
                    "result rejected (stale/conflict?)"
                );
                break; // 4xx 重试无意义（stale token/attempt）；5xx 才重试
            }
            Err(e) => {
                last_error = format!("network: {e}");
                tracing::warn!(
                    evaluation_id = %job.evaluation_id,
                    error = %e,
                    "result post error (retry {attempt_retry}/4)"
                );
            }
        }
    }
    if last_error.is_empty() {
        state.inflight.fetch_sub(1, Ordering::SeqCst);
    } else {
        tracing::error!(
            evaluation_id = %job.evaluation_id,
            "result post permanently failed: {last_error}"
        );
        state.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// 执行管线：healthcheck → judge → （official）exploit。
async fn run_job(state: &AppState, job: &ClaimedJob) -> JobOutcome {
    // 1. 容器未运行（claim 时无 IP）→ service_down。
    let Some(target_ip) = job.target_ip.as_deref() else {
        return JobOutcome::Business(
            ST_SERVICE_DOWN,
            Some("instance not running".into()),
            None,
            None,
            None,
            None,
        );
    };

    // 2. internal healthcheck（target_ip:container_port，重试）。
    let health_ok = match healthcheck_all(state, target_ip, &job.healthchecks).await {
        Ok(ok) => ok,
        Err(e) => return JobOutcome::Infrastructure(e),
    };
    if !health_ok {
        return JobOutcome::Business(
            ST_SERVICE_DOWN,
            Some("healthcheck failed".into()),
            None,
            None,
            None,
            None,
        );
    }

    // 3. Judge（check.py）。
    let Some(judge_script) = job.judge_script.as_deref() else {
        return JobOutcome::Infrastructure("GameBox 未配置 judge 脚本".into());
    };
    let judge = match run_target_script(state, judge_script, target_ip, 30, 16 * 1024).await {
        Ok(r) => r,
        Err(e) => return JobOutcome::Infrastructure(e),
    };
    let (judge_out, judge_err) = judge.truncated();
    if let Some(err) = &judge.parse_error {
        // 畸形输出 / 空输出 → 平台故障（不判玩家失败）。
        return JobOutcome::Infrastructure(format!(
            "judge 输出解析失败: {err}; exit={:?}",
            judge.exit_code
        ));
    }
    let judge_row = hit_row(&judge.rows, target_ip);
    let (judge_ok, judge_detail) = match judge_row {
        Some(r) => (
            r.success,
            r.error
                .clone()
                .unwrap_or_else(|| format!("judge: {}", if r.success { "PASS" } else { "FAIL" })),
        ),
        None => (false, "judge 未返回该目标结果".into()),
    };
    if !judge_ok {
        return JobOutcome::Business(
            ST_FUNCTIONAL_BROKEN,
            None,
            Some(judge_detail),
            None,
            judge_out,
            judge_err,
        );
    }

    // 4. Exploit（official only）。
    if job.kind == "official" {
        let Some(exploit_script) = job.exploit_script.as_deref() else {
            return JobOutcome::Infrastructure("official 评估缺少 exploit 脚本".into());
        };
        let exploit = match run_target_script(state, exploit_script, target_ip, 60, 32 * 1024).await
        {
            Ok(r) => r,
            Err(e) => return JobOutcome::Infrastructure(e),
        };
        let (exploit_out, exploit_err) = exploit.truncated();
        if let Some(err) = &exploit.parse_error {
            return JobOutcome::Infrastructure(format!(
                "exploit 输出解析失败: {err}; exit={:?}",
                exploit.exit_code
            ));
        }
        let exploit_row = hit_row(&exploit.rows, target_ip);
        let (exploit_ok, exploit_detail) = match exploit_row {
            Some(r) => (
                r.success,
                r.error.clone().unwrap_or_else(|| "exploit executed".into()),
            ),
            None => (false, "exploit 未返回该目标结果".into()),
        };
        if exploit_ok {
            return JobOutcome::Business(
                ST_VULNERABLE,
                None,
                Some(judge_detail),
                Some(exploit_detail),
                exploit_out,
                exploit_err,
            );
        }
        return JobOutcome::Business(
            ST_PATCHED,
            None,
            Some(judge_detail),
            Some(exploit_detail),
            exploit_out,
            exploit_err,
        );
    }

    // manual：health + judge 全过 → patched（不计分；exploit 恒不执行）。
    JobOutcome::Business(
        ST_PATCHED,
        None,
        Some(judge_detail),
        None,
        judge_out,
        judge_err,
    )
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}... (truncated, {} bytes total)", &s[..max_len], s.len())
    }
}

/// 对 target_ip:container_port 逐个 healthcheck（HTTP/TCP，带重试）。
/// 返回 Err = 探针基础设施失败（非玩家域）。
async fn healthcheck_all(
    state: &AppState,
    target_ip: &str,
    healthchecks: &[serde_json::Value],
) -> Result<bool, String> {
    if healthchecks.is_empty() {
        return Ok(false);
    }
    for hc in healthchecks {
        let Some(obj) = hc.as_object() else {
            continue;
        };
        let htype = obj.get("type").and_then(|v| v.as_str()).unwrap_or("http");
        let Some(port) = obj.get("port").and_then(|v| v.as_u64()) else {
            continue;
        };
        let path = obj
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("/")
            .to_string();
        let expected_status = obj
            .get("expected_status")
            .and_then(|v| v.as_u64())
            .unwrap_or(200);

        let ok = match htype {
            "tcp" => probe_tcp(state, target_ip, port as u16).await?,
            _ => probe_http(state, target_ip, port as u16, &path, expected_status as u16).await?,
        };
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn probe_tcp(state: &AppState, ip: &str, port: u16) -> Result<bool, String> {
    let mut last = None;
    for _ in 0..3 {
        match tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::TcpStream::connect((ip, port)),
        )
        .await
        {
            Ok(Ok(_)) => return Ok(true),
            Ok(Err(e)) => last = Some(e.to_string()),
            Err(_) => last = Some("connect timeout".into()),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let _ = state;
    tracing::warn!(
        ip,
        port,
        error = last.unwrap_or_default(),
        "tcp probe failed"
    );
    Ok(false)
}

async fn probe_http(
    state: &AppState,
    ip: &str,
    port: u16,
    path: &str,
    expected: u16,
) -> Result<bool, String> {
    let url = format!("http://{ip}:{port}{path}");
    for _ in 0..3 {
        match state
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status == expected {
                    return Ok(true);
                }
                tracing::warn!(url, status, expected, "http probe status mismatch");
            }
            Err(e) => {
                tracing::warn!(url, error = %e, "http probe error");
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Ok(false)
}

// ── Pull Loop ──

/// 主循环：有容量就 claim，无任务短睡。
async fn pull_loop(state: Arc<AppState>) {
    loop {
        let available = state
            .concurrency
            .available_permits()
            .saturating_sub(state.inflight.load(Ordering::SeqCst))
            .max(0);
        if available > 0 {
            let capacity = (available as u64).min(state.claim_batch);
            match claim(&state, capacity).await {
                Ok(resp) => {
                    if !resp.jobs.is_empty() {
                        tracing::info!(
                            worker_id = %state.worker_id,
                            count = resp.jobs.len(),
                            lease_seconds = resp.lease_seconds,
                            "claimed jobs"
                        );
                        for job in resp.jobs {
                            let hb =
                                heartbeat_for_lease(resp.lease_seconds, state.heartbeat_interval);
                            actix_web::rt::spawn(execute_job(state.clone(), job, hb));
                        }
                        continue; // 立即再尝试领取（可能还有 pending）
                    }
                }
                Err(e) => {
                    tracing::warn!(worker_id = %state.worker_id, error = %e, "claim failed");
                }
            }
        }
        actix_web::rt::time::sleep(state.poll_interval).await;
    }
}

/// claim：调平台 internal API 领取 job。
async fn claim(state: &AppState, capacity: u64) -> Result<ClaimResponse, String> {
    let url = format!("{}/internal/awdp/judge/jobs/claim", state.platform_url);
    let body = ClaimRequest {
        worker_id: state.worker_id.clone(),
        capacity,
    };
    let resp = state
        .client
        .post(&url)
        .bearer_auth(&state.internal_token)
        .json(&body)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("claim network error: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("claim rejected: HTTP {status}"));
    }
    resp.json::<ClaimResponse>()
        .await
        .map_err(|e| format!("claim response parse: {e}"))
}

/// 心跳间隔 = min(配置, max(5s, lease/3))，保证明显小于 lease。
fn heartbeat_for_lease(lease_seconds: i64, configured: Duration) -> Duration {
    let from_lease = Duration::from_secs((lease_seconds.max(0) / 3).max(5) as u64);
    configured.min(from_lease)
}

// ── Data Plane API ──

/// GET /healthz —— 存活检查（GameBox 网络内可达）。
async fn healthz() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

/// GET /flag —— Break flag（plan §8/§9）。
///
/// 不接受任何身份参数：调用者身份 = 真实 TCP peer IP（`req.peer_addr()`）。
/// JudgeServer 把 source_ip 转发给 FloatCTF internal API 解析；
/// 仅 Break 阶段返回纯 flag 文本，其余 403/409。
async fn handle_flag(state: web::Data<AppState>, req: HttpRequest) -> HttpResponse {
    let Some(peer) = req.peer_addr() else {
        return HttpResponse::BadRequest().finish();
    };
    let source_ip = peer.ip().to_string();
    if source_ip.is_empty() || source_ip == "::1" {
        return HttpResponse::Forbidden().finish();
    }

    let url = format!("{}/internal/awdp/flag/resolve", state.platform_url);
    let body = serde_json::json!({ "source_ip": source_ip });
    match state
        .client
        .post(&url)
        .bearer_auth(&state.internal_token)
        .json(&body)
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            // UniResponse<{code,message,data,meta}> → 取 data 作为 flag 文本。
            match resp.json::<serde_json::Value>().await {
                Ok(v) => {
                    let data = v.get("data").and_then(|d| d.as_str());
                    match data {
                        Some(flag) => HttpResponse::Ok().body(flag.to_string()),
                        None => HttpResponse::InternalServerError().finish(),
                    }
                }
                Err(_) => HttpResponse::InternalServerError().finish(),
            }
        }
        Ok(resp) => {
            // 403（未知 source）/ 409（非 Break）→ 原样透传状态。
            tracing::warn!(source_ip, status = %resp.status(), "flag resolve rejected");
            let _ = resp;
            HttpResponse::Forbidden().finish()
        }
        Err(e) => {
            tracing::warn!(source_ip, error = %e, "flag resolve network error");
            HttpResponse::BadGateway().finish()
        }
    }
}

// ── main ──

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
        env::var("PLATFORM_INTERNAL_URL").unwrap_or_else(|_| "http://127.0.0.1:9090".to_string());
    let internal_token = env::var("INTERNAL_TOKEN").expect("INTERNAL_TOKEN must be set");
    let worker_id = env::var("WORKER_ID").unwrap_or_else(|_| "practice-judge-01".to_string());
    let data_listen = env::var("DATA_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let claim_batch: u64 = env::var("CLAIM_BATCH")
        .unwrap_or_else(|_| "16".to_string())
        .parse()
        .unwrap_or(16);
    let max_concurrency: usize = env::var("MAX_CONCURRENCY")
        .unwrap_or_else(|_| "8".to_string())
        .parse()
        .unwrap_or(8);
    let poll_interval = Duration::from_secs(
        env::var("POLL_INTERVAL_SECS")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .unwrap_or(5),
    );
    let heartbeat_interval = Duration::from_secs(
        env::var("HEARTBEAT_INTERVAL_SECS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30),
    );
    let work_dir = env::var("WORK_DIR").unwrap_or_else(|_| "/tmp/judge".to_string());

    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work directory");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to create HTTP client");

    let state = Arc::new(AppState {
        client,
        platform_url,
        internal_token,
        worker_id: worker_id.clone(),
        claim_batch,
        concurrency: Arc::new(Semaphore::new(max_concurrency)),
        poll_interval,
        heartbeat_interval,
        work_dir,
        inflight: Arc::new(AtomicUsize::new(0)),
    });

    tracing::info!(
        worker_id = %worker_id,
        data_listen = %data_listen,
        max_concurrency,
        claim_batch,
        "FloatCTF AWDP Practice JudgeServer (pull worker) starting"
    );

    // Pull loop（后台）。
    let loop_state = state.clone();
    actix_web::rt::spawn(pull_loop(loop_state));

    // Data plane listener（GameBox 可达；control 由 API 侧承接，本服务只发起出站调用）。
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(web::Data::new(state.clone()))
            .route("/healthz", web::get().to(healthz))
            .route("/flag", web::get().to(handle_flag))
    })
    .bind(&data_listen)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_interval_is_well_below_lease() {
        // lease 120s / 配置 30s → 30s（min）。
        assert_eq!(
            heartbeat_for_lease(120, Duration::from_secs(30)),
            Duration::from_secs(30)
        );
        // lease 30s / 配置 30s → max(5, 10) = 10s。
        assert_eq!(
            heartbeat_for_lease(30, Duration::from_secs(30)),
            Duration::from_secs(10)
        );
        // lease 9s → max(5, 3) = 5s 下限。
        assert_eq!(
            heartbeat_for_lease(9, Duration::from_secs(30)),
            Duration::from_secs(5)
        );
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
        assert!(keys.contains(&"JUDGE_TIMEOUT_FACTOR"));
        assert!(!keys.contains(&"INTERNAL_TOKEN"));
        assert!(!keys.contains(&"PLATFORM_INTERNAL_URL"));
        assert!(!keys.contains(&"RUST_LOG"));
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
        assert!(parse_batch_results("[]").is_err());
    }

    #[test]
    fn hit_row_prefers_exact_ip_then_first() {
        let rows = vec![
            BatchRow {
                ip: "10.0.0.1".into(),
                success: false,
                error: None,
            },
            BatchRow {
                ip: "10.0.0.2".into(),
                success: true,
                error: None,
            },
        ];
        assert!(hit_row(&rows, "10.0.0.2").unwrap().success);
        // 空 IP 兜底命中目标行。
        let empty = vec![BatchRow {
            ip: String::new(),
            success: true,
            error: None,
        }];
        assert!(hit_row(&empty, "10.9.9.9").unwrap().success);
    }
}
