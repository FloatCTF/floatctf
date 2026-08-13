//! AWDP 练习 Judge 服务：练习 docker 子网 + JudgeServer 部署/停止。
//!
//! 架构（Pull + Lease 模型，plan §12-§19）：
//!   - 全部练习实例加入 data network `fctf-awdp-practice`；
//!   - JudgeServer 是**主动拉取**的 worker（claim/heartbeat/result），平台不再 push /batch；
//!   - 本模块只负责：子网 ensure、容器部署/停止、配置/状态查询。
//!   - 旧的「例行检查 sweep + /batch 派发 + callback 落库」push 流程已移除（plan §61）。

use std::time::Duration;

use bollard::Docker;
use fcmc::{ContainerRuntime, ContainerSpec, DockerContainerRuntime, ImageRuntime, ResourceLimits};
use sea_orm::DatabaseConnection;
use tracing::{info, warn};
use uuid::Uuid;

use crate::core::config::AwdpStaticConfig;
use crate::entity::awdp_practice_judge_settings;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::judge::{
        PRACTICE_JUDGE_CONTAINER_NAME, PRACTICE_JUDGE_PORT, PRACTICE_NETWORK_NAME,
        practice_judge_token,
    },
    repo::{practice_judge_repo, run_repo},
};

/// 幂等 ensure 练习专用 docker 子网（不存在则创建；已存在复用）。
///
/// 子网：所有练习实例 + JudgeServer 共用（data plane）。`internal=false`（练习靶机需要出网，
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
    // data plane ACL（best-effort：nft 不可用/无权限时跳过，不阻塞实例启动）。
    let _ = crate::modules::event::awdp::service::practice_acl::apply_practice_acl(
        docker,
        &config.practice_judge_ip,
        &crate::modules::event::awdp::service::practice_acl::DEFAULT_BLOCKED_HOST_PORTS,
    )
    .await?;
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

/// 解析 JudgeServer 基址（data plane，供玩家/管理端展示）：配置显式 URL 优先，
/// 留空自动推导 `http://{judge_ip}:{port}`。注意：此地址不再用于平台→Judge 派发
/// （Pull 模型下 Judge 主动拉取，平台只提供 internal API）。
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

/// 部署练习 JudgeServer 容器（Pull worker，幂等：已 running 跳过；DB 记录与容器双核验）。
///
/// 容器进入练习 data 子网固定 IP；env 注入平台 internal API 基址 / worker 配置 /
/// 内部令牌（令牌由平台 Secret HKDF 派生，不落库不落日志）。
pub async fn deploy_judge(
    db: &DatabaseConnection,
    docker: &Docker,
    config: &AwdpStaticConfig,
    jwt_secret: &[u8],
    event_id: Uuid,
) -> AwdpResult<()> {
    let runtime = DockerContainerRuntime::new(docker.clone());

    // 1. 子网 ensure（练习实例启动路径也会 ensure，这里部署侧再保证一次）+
    //    control 子网 + data plane ACL（best-effort）。
    let _ = ensure_practice_network(docker, config).await?;
    let _ =
        crate::modules::event::awdp::service::practice_acl::ensure_control_network(docker).await?;
    let _ = crate::modules::event::awdp::service::practice_acl::apply_practice_acl(
        docker,
        &config.practice_judge_ip,
        &crate::modules::event::awdp::service::practice_acl::DEFAULT_BLOCKED_HOST_PORTS,
    )
    .await?;

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
            format!("WORKER_ID=practice-judge-{}", &event_id.to_string()[..8]),
            format!("DATA_LISTEN_ADDR=0.0.0.0:{PRACTICE_JUDGE_PORT}"),
            // Pull worker 配置（bounded concurrency）。
            "CLAIM_BATCH=16".to_string(),
            "MAX_CONCURRENCY=8".to_string(),
            "POLL_INTERVAL_SECS=5".to_string(),
            "HEARTBEAT_INTERVAL_SECS=30".to_string(),
        ],
        labels: std::collections::HashMap::from([
            ("io.floatctf.managed".into(), "true".into()),
            ("io.floatctf.resource".into(), "awdp-practice-judge".into()),
            ("io.floatctf.event_id".into(), event_id.to_string()),
        ]),
        network_name: Some(PRACTICE_NETWORK_NAME.to_string()),
        fixed_ip: Some(config.practice_judge_ip.clone()),
        // data plane DNS alias：GameBox 内 `awdp-judge` 可解析（不暴露动态 IP 契约）。
        network_aliases: vec![
            crate::modules::event::awdp::domain::judge::JUDGE_DATA_ALIAS.to_string(),
        ],
        port_bindings: vec![],
        auto_remove: false,
        resources: ResourceLimits {
            cpu_millis: Some(500),
            memory_bytes: Some(256 * 1024 * 1024),
            pids_limit: Some(128),
            // 收敛：cap_drop ALL（不再依赖容器内网络管理能力；Pull worker 无特权需求）。
            cap_drop: vec!["ALL".to_string()],
            privileged: false,
            // 不再使用 host.docker.internal（Phase C 收敛到 control network）。
            extra_hosts: vec![],
        },
        network_mode: None,
        healthcheck: None,
    };
    let handle = runtime
        .create_and_start(spec)
        .await
        .map_err(|e| AwdpError::Docker(format!("deploy practice judge: {e}")))?;

    // 4. 加入 control 网络（internal=true；GameBox 无权加入）。
    //    注意：control 网络无出站 → JudgeServer 到宿主 API 走 data 网络网关
    //    （PLATFORM_INTERNAL_URL 由配置给出，host firewall 限制 GameBox 访问）。
    if let Err(e) = docker
        .connect_network(
            crate::modules::event::awdp::domain::judge::CONTROL_NETWORK_NAME,
            bollard::network::ConnectNetworkOptions::<String> {
                container: handle.container_id.clone(),
                ..Default::default()
            },
        )
        .await
    {
        warn!(
            container = %handle.container_id,
            error = %e,
            "[PracticeJudge] connect control network failed (best-effort)"
        );
    }

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
        "AWDP practice judge (pull worker) deployed"
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

/// 供管理端展示 JudgeServer 容器最近心跳/状态（Pull worker 存活证据）。
pub async fn judge_worker_health(docker: &Docker, config: &AwdpStaticConfig) -> AwdpResult<String> {
    let runtime = DockerContainerRuntime::new(docker.clone());
    let running = match runtime
        .inspect_container(PRACTICE_JUDGE_CONTAINER_NAME)
        .await
    {
        Ok(state) => state.running,
        Err(_) => false,
    };
    if !running {
        return Ok("stopped".to_string());
    }
    // 数据面 /healthz（容器内网 IP 直连；平台与容器同宿主时可直接探测）。
    let url = format!(
        "http://{}:{}",
        config.practice_judge_ip, PRACTICE_JUDGE_PORT
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| AwdpError::Network(format!("build health client: {e}")))?;
    match client.get(format!("{url}/healthz")).send().await {
        Ok(resp) if resp.status().is_success() => Ok("healthy".to_string()),
        Ok(_) => Ok("unhealthy".to_string()),
        Err(_) => Ok("unreachable".to_string()),
    }
}
