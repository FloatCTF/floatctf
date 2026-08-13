//! AWDP 练习 Judge 服务：练习 docker 子网 + JudgeServer 部署/停止 + 例行检查派发。
//!
//! 架构（对照用户需求）：
//!   1. 全部练习实例加入同一 docker 子网 `fctf-awdp-practice`（runtime 启动时挂网，
//!      平台部署 JudgeServer 容器也在该子网内，按容器内网 IP 直达全部练习 GameBox）；
//!   2. 平台按配置周期把「练习实例 × 检查类型」批次派发给 JudgeServer：
//!      - `exploit`：运行 GameBox 的 awdp exploit 脚本 → 验证目标可被攻破；
//!      - `flag`：HTTP GET 目标 flag 端点（如 /flag.php）→ 验证 flag 已暴露；
//!   3. JudgeServer 逐任务执行并回调平台（/internal/awdp/practice/judge/callback），
//!      平台按 callback_id 幂等落库 awdp_judge_results 供管理端展示。

use std::collections::HashMap;
use std::time::Duration;

use bollard::Docker;
use fcmc::{ContainerRuntime, ContainerSpec, DockerContainerRuntime, ImageRuntime, ResourceLimits};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::core::config::AwdpStaticConfig;
use crate::entity::{awdp_practice_judge_settings, gameboxes};
use crate::infrastructure::settings::get_setting;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::{
        flag::awdp_flag,
        judge::{
            PRACTICE_JUDGE_CONTAINER_NAME, PRACTICE_JUDGE_PORT, PRACTICE_NETWORK_NAME,
            judge_callback_id, practice_judge_token,
        },
    },
    repo::{practice_judge_repo, run_repo},
};

/// 幂等 ensure 练习专用 docker 子网（不存在则创建；已存在复用）。
///
/// 子网：所有练习实例 + JudgeServer 共用。`internal=false`（练习靶机需要出网，
/// 且玩家仍经宿主端口访问 public endpoint；子网仅用于实例间与 Judge 互访）。
///
/// IP 规划：动态分配池 = 子网后半个（如 /24 → `.128/25`），前半段保留给固定
/// JudgeServer IP（config.awdp.practice_judge_ip），避免动态实例抢占固定地址。
///
/// 注意：**不用 `fcmc::create_network`**（它会先 remove 再 create；并发 ensure
/// 会拆掉正在使用的子网）。这里 inspect 后仅在缺失时创建，重复创建竞态按成功处理。
#[allow(deprecated)] // bollard CreateNetworkOptions（fcmc 同款；新 models 结构字段未稳定）
pub async fn ensure_practice_network(
    docker: &Docker,
    config: &AwdpStaticConfig,
) -> AwdpResult<String> {
    use bollard::network::CreateNetworkOptions;

    let runtime = DockerContainerRuntime::new(docker.clone());
    if runtime
        .inspect_network(PRACTICE_NETWORK_NAME)
        .await
        .map(|s| s.exists)
        .unwrap_or(false)
    {
        return Ok(PRACTICE_NETWORK_NAME.to_string());
    }

    let conf = CreateNetworkOptions {
        name: PRACTICE_NETWORK_NAME.to_string(),
        driver: "bridge".to_string(),
        internal: false,
        check_duplicate: true,
        ipam: bollard::secret::Ipam {
            config: Some(vec![bollard::secret::IpamConfig {
                subnet: Some(config.practice_network_subnet.clone()),
                ip_range: dynamic_ip_range(&config.practice_network_subnet),
                ..Default::default()
            }]),
            ..Default::default()
        },
        ..Default::default()
    };
    match docker.create_network(conf).await {
        Ok(_) => {
            info!(
                network = %PRACTICE_NETWORK_NAME,
                subnet = %config.practice_network_subnet,
                "AWDP practice docker network ensured"
            );
        }
        Err(e) if network_already_exists(&e) => {
            // 并发创建竞态：另一调用已建好。
        }
        Err(e) => {
            return Err(AwdpError::Docker(format!("create practice network: {e}")));
        }
    }
    Ok(PRACTICE_NETWORK_NAME.to_string())
}

/// 动态实例分配池：仅支持 `x.y.z.0/24` → `x.y.z.128/25`（后半段）。
/// 前半段（.1~.127）保留给固定地址（JudgeServer 等）；其他子网不限制（旧行为）。
fn dynamic_ip_range(subnet: &str) -> Option<String> {
    let (prefix, bits) = subnet.rsplit_once('/')?;
    if bits != "24" {
        return None;
    }
    let mut octets: Vec<&str> = prefix.split('.').collect();
    if octets.len() != 4 {
        return None;
    }
    let last = octets.pop()?;
    if last.parse::<u8>().ok()? != 0 {
        return None;
    }
    octets.push("128");
    Some(format!("{}/25", octets.join(".")))
}

fn network_already_exists(e: &bollard::errors::Error) -> bool {
    match e {
        bollard::errors::Error::DockerResponseServerError {
            status_code: 409, ..
        } => true,
        bollard::errors::Error::DockerResponseServerError {
            status_code: 500,
            message,
        } => message.to_lowercase().contains("already exists"),
        _ => false,
    }
}

/// 解析 JudgeServer 基址：配置显式 URL 优先，留空自动推导 `http://{judge_ip}:{port}`。
pub fn resolve_judge_server_url(
    settings: &awdp_practice_judge_settings::Model,
    config: &AwdpStaticConfig,
) -> String {
    let explicit = settings.judge_server_url.trim().trim_end_matches('/');
    if !explicit.is_empty() {
        return explicit.to_string();
    }
    format!(
        "http://{}:{}",
        config.practice_judge_ip, PRACTICE_JUDGE_PORT
    )
}

/// 部署练习 JudgeServer 容器（幂等：已 running 跳过；DB 记录与容器双核验）。
///
/// 容器进入练习子网固定 IP；env 注入平台回调基址 / 事件 id / 内部令牌
/// （令牌由平台 Secret HKDF 派生，不落库不落日志）。
pub async fn deploy_judge(
    db: &DatabaseConnection,
    docker: &Docker,
    config: &AwdpStaticConfig,
    jwt_secret: &[u8],
    event_id: Uuid,
) -> AwdpResult<()> {
    let runtime = DockerContainerRuntime::new(docker.clone());

    // 1. 子网 ensure（练习实例启动路径也会 ensure，这里部署侧再保证一次）。
    let _ = ensure_practice_network(docker, config).await?;

    // 2. 镜像 ensure（pin 不可变）。
    ImageRuntime::ensure_image(&runtime, &config.practice_judgeserver_image, None)
        .await
        .map_err(|e| {
            AwdpError::Docker(format!(
                "ensure judge image {}: {e}",
                config.practice_judgeserver_image
            ))
        })?;

    // 3. 已有容器：running → 幂等成功；停止/消失 → 重建。
    match runtime
        .inspect_container(PRACTICE_JUDGE_CONTAINER_NAME)
        .await
    {
        Ok(state) if state.running => {
            practice_judge_repo::update_container_state(
                db,
                event_id,
                "running",
                Some(&state.container_id),
            )
            .await?;
            info!("[PracticeJudge] judge container already running — skip");
            return Ok(());
        }
        Ok(_) => {
            warn!("[PracticeJudge] judge container exists but not running — recreating");
            let _ = runtime
                .stop_and_remove(PRACTICE_JUDGE_CONTAINER_NAME, fcmc::IMMEDIATE_STOP_TIMEOUT)
                .await;
        }
        Err(_) => {} // 不存在
    }

    let token = practice_judge_token(jwt_secret);
    let spec = ContainerSpec {
        name: PRACTICE_JUDGE_CONTAINER_NAME.to_string(),
        image: config.practice_judgeserver_image.clone(),
        env: vec![
            format!("PLATFORM_INTERNAL_URL={}", config.platform_internal_url),
            format!("EVENT_ID={event_id}"),
            format!("INTERNAL_TOKEN={token}"),
            format!("LISTEN_ADDR=0.0.0.0:{PRACTICE_JUDGE_PORT}"),
        ],
        labels: HashMap::from([
            ("io.floatctf.managed".into(), "true".into()),
            ("io.floatctf.resource".into(), "awdp-practice-judge".into()),
            ("io.floatctf.event_id".into(), event_id.to_string()),
        ]),
        network_name: Some(PRACTICE_NETWORK_NAME.to_string()),
        fixed_ip: Some(config.practice_judge_ip.clone()),
        port_bindings: vec![],
        auto_remove: false,
        resources: ResourceLimits {
            cpu_millis: Some(500),
            memory_bytes: Some(256 * 1024 * 1024),
            pids_limit: Some(128),
            cap_drop: vec![],
            privileged: false,
            extra_hosts: vec!["host.docker.internal:host-gateway".into()],
        },
        network_mode: None,
        healthcheck: None,
    };
    let handle = runtime
        .create_and_start(spec)
        .await
        .map_err(|e| AwdpError::Docker(format!("deploy practice judge: {e}")))?;
    practice_judge_repo::update_container_state(
        db,
        event_id,
        "running",
        Some(&handle.container_id),
    )
    .await?;
    info!(
        container = %PRACTICE_JUDGE_CONTAINER_NAME,
        id = %handle.container_id,
        ip = %config.practice_judge_ip,
        "AWDP practice judge deployed"
    );
    Ok(())
}

/// 停止练习 JudgeServer 容器（幂等：容器不存在视为成功）。
pub async fn stop_judge(
    db: &DatabaseConnection,
    docker: &Docker,
    event_id: Uuid,
) -> AwdpResult<()> {
    let runtime = DockerContainerRuntime::new(docker.clone());
    runtime
        .stop_and_remove(PRACTICE_JUDGE_CONTAINER_NAME, fcmc::IMMEDIATE_STOP_TIMEOUT)
        .await
        .map_err(|e| AwdpError::Docker(format!("stop practice judge: {e}")))?;
    practice_judge_repo::update_container_state(db, event_id, "stopped", None).await?;
    info!("AWDP practice judge stopped");
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// 例行检查派发（sweep）
// ────────────────────────────────────────────────────────────────────────────

/// 派发给 JudgeServer 的单个任务（与 crates/awdp-judgeserver JudgeTask 对齐）。
#[derive(Debug, Clone, Serialize)]
struct JudgeDispatchTask {
    id: Uuid,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    script_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    script_args_json: Option<String>,
    target_ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    flag_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_flag: Option<String>,
    timeout_secs: u64,
    callback_id: String,
}

#[derive(Debug, Serialize)]
struct JudgeDispatchBatch {
    tasks: Vec<JudgeDispatchTask>,
}

#[derive(Debug, Deserialize)]
struct JudgeDispatchResponse {
    accepted: bool,
}

/// sweep 汇总。
#[derive(Debug, Default, Clone)]
pub struct SweepSummary {
    /// 跳过（无容器 IP / 无可用检查的实例数）。
    pub skipped: usize,
    /// 实际派发的任务数。
    pub dispatched: usize,
    /// 是否因间隔未到而跳过本轮。
    pub throttled: bool,
}

/// 例行检查：对全部运行中的练习实例派发 exploit + flag 检查批次。
///
/// 门禁：enabled + 容器 running + 距上次派发 ≥ interval_secs + 有实例。
/// 幂等：每次派发新任务 id（结果按 callback_id 幂等落库）。
pub async fn sweep(
    db: &DatabaseConnection,
    docker: &Docker,
    config: &AwdpStaticConfig,
    jwt_secret: &[u8],
    event_id: Uuid,
) -> AwdpResult<SweepSummary> {
    let settings = practice_judge_repo::ensure_settings(db, event_id).await?;
    let mut summary = SweepSummary::default();

    // 1. enabled 门。
    if !settings.enabled {
        return Ok(summary);
    }
    // 2. 间隔门（last_sweep_at + interval_secs > now → 跳过）。
    if let Some(last) = settings.last_sweep_at {
        let elapsed = chrono::Utc::now() - last.with_timezone(&chrono::Utc);
        if elapsed.num_seconds() < settings.interval_secs as i64 {
            summary.throttled = true;
            return Ok(summary);
        }
    }
    // 3. 容器 running 门（DB 记录优先，真实容器兜底）。
    let container_running = match settings.container_status.as_str() {
        "running" => true,
        _ => {
            // 状态记录不一致时回退真实容器检查。
            matches!(
                DockerContainerRuntime::new(docker.clone())
                    .inspect_container(PRACTICE_JUDGE_CONTAINER_NAME)
                    .await,
                Ok(state) if state.running
            )
        }
    };
    if !container_running {
        warn!("[PracticeJudge] sweep skipped: judge container not running");
        return Ok(summary);
    }

    // 4. 收集运行中练习实例 + 构建任务。
    let rows = practice_judge_repo::list_running_practice_instances(db).await?;
    let runtime = DockerContainerRuntime::new(docker.clone());
    let flag_prefix = get_setting(db, "FLAG_PREFIX")
        .await
        .unwrap_or_else(|_| "flag".into());
    // 本次派发的 sweep id：同一实例不同轮次检查产生新结果行（callback_id 含 sweep）。
    let sweep_id = Uuid::new_v4();
    let mut tasks: Vec<JudgeDispatchTask> = Vec::new();

    for (instance, ext, run, gamebox) in &rows {
        // 容器 IP（练习子网内网地址）。
        let ip = match runtime.inspect_container(&instance.container_name).await {
            Ok(state) => state.ip_address,
            Err(e) => {
                warn!(
                    container = %instance.container_name,
                    error = %e,
                    "[PracticeJudge] inspect failed — skip instance"
                );
                summary.skipped += 1;
                continue;
            }
        };
        let Some(ip) = ip else {
            summary.skipped += 1;
            continue;
        };

        let expected_flag = awdp_flag(
            jwt_secret,
            run.id,
            ext.gamebox_id,
            ext.owner_user_id,
            ext.owner_team_id,
            &flag_prefix,
        );

        // flag 检查：取首个 http healthcheck 端口 + 配置的 flag_path。
        if let Some(port) = http_healthcheck_port(gamebox) {
            tasks.push(JudgeDispatchTask {
                id: Uuid::new_v4(),
                kind: "flag".to_string(),
                script_content: None,
                script_args_json: None,
                target_ip: ip.clone(),
                flag_url: Some(format!("http://{ip}:{port}{}", settings.flag_path)),
                expected_flag: Some(expected_flag.clone()),
                timeout_secs: 15,
                callback_id: judge_callback_id(sweep_id, run.id, instance.id, "flag"),
            });
        }
        // exploit 检查：GameBox awdp exploit 脚本。
        if let Some(script) = gamebox
            .awdp_exploit_script_content
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            tasks.push(JudgeDispatchTask {
                id: Uuid::new_v4(),
                kind: "exploit".to_string(),
                script_content: Some(script.to_string()),
                script_args_json: None,
                target_ip: ip.clone(),
                flag_url: None,
                expected_flag: None,
                timeout_secs: 60,
                callback_id: judge_callback_id(sweep_id, run.id, instance.id, "exploit"),
            });
        }
        if tasks.is_empty() {
            summary.skipped += 1;
        }
    }

    // 5. 派发。
    if tasks.is_empty() {
        // 无任务也刷新间隔，避免空转每 tick 全量扫描。
        practice_judge_repo::touch_last_sweep(db, event_id).await?;
        return Ok(summary);
    }
    let judge_url = resolve_judge_server_url(&settings, config);
    let token = practice_judge_token(jwt_secret);
    dispatch_batch(&judge_url, &token, &tasks).await?;
    summary.dispatched = tasks.len();

    practice_judge_repo::touch_last_sweep(db, event_id).await?;
    info!(
        event_id = %event_id,
        dispatched = summary.dispatched,
        skipped = summary.skipped,
        "AWDP practice judge sweep dispatched"
    );
    Ok(summary)
}

/// 解析 GameBox 首个 http healthcheck 端口（flag curl 验证目标端口）。
fn http_healthcheck_port(gamebox: &gameboxes::Model) -> Option<u16> {
    let healthchecks = crate::modules::gamebox::healthcheck::parse_healthchecks(
        &gamebox
            .healthchecks_json
            .clone()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .unwrap_or_default();
    healthchecks.into_iter().find_map(|hc| match hc {
        crate::modules::gamebox::healthcheck::AppHealthcheck::Http { port, .. } => Some(port),
        crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { .. } => None,
    })
}

/// 经 HTTP 向练习 JudgeServer 派发一批任务。
async fn dispatch_batch(
    judge_url: &str,
    token: &str,
    tasks: &[JudgeDispatchTask],
) -> AwdpResult<()> {
    let endpoint = {
        let base = judge_url.trim().trim_end_matches('/');
        if base.ends_with("/batch") {
            base.to_string()
        } else {
            format!("{base}/batch")
        }
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| AwdpError::Network(format!("build judge client: {e}")))?;
    let response = client
        .post(&endpoint)
        .bearer_auth(token)
        .json(&JudgeDispatchBatch {
            tasks: tasks.to_vec(),
        })
        .send()
        .await
        .map_err(|e| AwdpError::Network(format!("judge dispatch failed: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AwdpError::Network(format!(
            "judge rejected batch with HTTP {status}: {}",
            &body[..body.len().min(512)]
        )));
    }
    let accepted = response
        .json::<JudgeDispatchResponse>()
        .await
        .map_err(|e| AwdpError::Network(format!("invalid judge response: {e}")))?;
    if !accepted.accepted {
        return Err(AwdpError::Network("judge did not accept the batch".into()));
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// 回调落库（JudgeServer → 平台）
// ────────────────────────────────────────────────────────────────────────────

/// JudgeServer 回调请求体（与 crates/awdp-judgeserver TaskResult 对齐）。
#[derive(Debug, Deserialize)]
pub struct JudgeCallbackRequest {
    pub task_id: Uuid,
    pub callback_id: String,
    #[serde(default)]
    pub kind: String,
    pub status: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<i32>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

/// 记录一条练习 Judge 检查结果（按 callback_id 幂等）。
///
/// callback_id 编码 run/instance/kind；从 awdp_instances 解析 gamebox/owner。
pub async fn record_callback(db: &DatabaseConnection, cb: &JudgeCallbackRequest) -> AwdpResult<()> {
    // 解析 callback_id：awdp-practice-judge:{sweep}:{run}:{instance}:{kind}
    let parts: Vec<&str> = cb.callback_id.split(':').collect();
    if parts.len() < 5 || parts[0] != "awdp-practice-judge" {
        return Err(AwdpError::Validation(format!(
            "malformed callback_id: {}",
            cb.callback_id
        )));
    }
    let _sweep_id = Uuid::parse_str(parts[1])
        .map_err(|_| AwdpError::Validation("bad sweep_id in callback_id".into()))?;
    let run_id = Uuid::parse_str(parts[2])
        .map_err(|_| AwdpError::Validation("bad run_id in callback_id".into()))?;
    let instance_id = Uuid::parse_str(parts[3])
        .map_err(|_| AwdpError::Validation("bad instance_id in callback_id".into()))?;
    let kind = parts[4];
    let _ = run_id;

    // 归属/身份解析（instance → awdp extension）。
    let (instance, ext) =
        crate::modules::event::awdp::repo::instance_repo::find_by_instance_id(db, instance_id)
            .await?;

    let status = normalize_status(&cb.status);
    let detail = build_detail(cb);

    practice_judge_repo::insert_result(
        db,
        instance.event_id,
        ext.run_id,
        instance.id,
        ext.gamebox_id,
        ext.owner_user_id,
        ext.owner_team_id,
        kind,
        status,
        detail.as_deref(),
        &cb.callback_id,
    )
    .await?;

    // 幂等日志（重复回调不重复落库但记录一次 info 供排查）。
    info!(
        task_id = %cb.task_id,
        callback_id = %cb.callback_id,
        kind = %kind,
        status = %status,
        "AWDP practice judge callback recorded"
    );
    Ok(())
}

fn normalize_status(status: &str) -> &'static str {
    match status {
        "success" => "success",
        "failure" => "failure",
        _ => "error",
    }
}

fn build_detail(cb: &JudgeCallbackRequest) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(d) = cb.detail.as_deref().filter(|s| !s.trim().is_empty()) {
        parts.push(d.to_string());
    }
    if let Some(stdout) = cb.stdout.as_deref().filter(|s| !s.trim().is_empty()) {
        parts.push(format!("stdout: {}", truncate(stdout, 300)));
    }
    if let Some(stderr) = cb.stderr.as_deref().filter(|s| !s.trim().is_empty()) {
        parts.push(format!("stderr: {}", truncate(stderr, 300)));
    }
    if let Some(code) = cb.exit_code {
        parts.push(format!("exit={code}"));
    }
    if let Some(ms) = cb.duration_ms {
        parts.push(format!("{ms}ms"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" | "))
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…({} bytes)", &s[..max], s.len())
    }
}

/// 供管理端状态展示：JudgeServer 容器真实运行状态（DB 记录 + 真实容器双核验）。
/// 记录与实际不一致时回写 stopped（下次 deploy 幂等重建）。
pub async fn container_status(
    db: &DatabaseConnection,
    docker: &Docker,
    settings: &awdp_practice_judge_settings::Model,
) -> String {
    if settings.container_status != "running" {
        return settings.container_status.clone();
    }
    let running = match DockerContainerRuntime::new(docker.clone())
        .inspect_container(PRACTICE_JUDGE_CONTAINER_NAME)
        .await
    {
        Ok(state) => state.running,
        Err(_) => false,
    };
    if running {
        "running".to_string()
    } else {
        let _ = practice_judge_repo::update_container_state(db, settings.event_id, "stopped", None)
            .await;
        "stopped".to_string()
    }
}

/// 解析练习 run（供 deploy 侧校验 AWDPlusPractice 存在）。
pub async fn ensure_practice_event(db: &DatabaseConnection) -> AwdpResult<Uuid> {
    run_repo::ensure_practice_event(db)
        .await
        .map_err(|e| AwdpError::Internal(format!("ensure AWDPlusPractice event: {e}")))
}
