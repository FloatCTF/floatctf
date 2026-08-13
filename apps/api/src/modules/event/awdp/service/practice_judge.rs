//! AWDP 练习 JudgeServer 服务：练习 data 子网 + JudgeServer 部署。
//!
//! 架构（Pull + Lease 模型，plan §12-§19）：
//!   - 全部练习实例加入 data network `fctf-awdp-practice`；
//!   - JudgeServer 是**主动拉取**的 worker（claim/heartbeat/result），平台不再 push /batch；
//!   - 本模块只负责：子网 ensure、容器部署（**默认启用**：练习 run Launch / 实例启动时
//!     自动部署，无管理端配置页）；
//!   - 旧的「例行检查 sweep + /batch 派发 + callback 落库」push 流程已移除（plan §61），
//!     管理端配置表（awdp_practice_judge_settings）与历史结果表（awdp_judge_results）
//!     已随迁移删除。

use bollard::Docker;
use fcmc::{ContainerRuntime, ContainerSpec, DockerContainerRuntime, ImageRuntime, ResourceLimits};
use tracing::{info, warn};
use uuid::Uuid;

use crate::core::config::AwdpStaticConfig;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::judge::{
        PRACTICE_JUDGE_CONTAINER_NAME, PRACTICE_JUDGE_PORT, PRACTICE_NETWORK_NAME,
        practice_judge_token,
    },
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

/// 部署练习 JudgeServer 容器（Pull worker，幂等：已 running 跳过）。
///
/// **默认启用**：练习 run Launch / 练习实例启动时自动调用（无管理端配置页）。
/// 容器进入练习 data 子网固定 IP；env 注入平台 internal API 基址 / worker 配置 /
/// 内部令牌（令牌由平台 Secret HKDF 派生，不落库不落日志）。
///
/// 并发安全：多实例并发部署时容器名冲突视为「对方已创建成功」，重查后按成功返回。
pub async fn deploy_judge(
    docker: &Docker,
    config: &AwdpStaticConfig,
    jwt_secret: &[u8],
    event_id: Uuid,
) -> AwdpResult<()> {
    if config.platform_internal_url.trim().is_empty() {
        return Err(AwdpError::InvalidState(
            "awdp.platform_internal_url 必须显式配置（control/data 网络可达的 FloatCTF API 基址；不再默认 host.docker.internal）".into(),
        ));
    }
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
    let handle = match runtime.create_and_start(spec).await {
        Ok(handle) => handle,
        Err(e) if container_conflict(&e) => {
            // 并发部署：另一侧刚创建成功 → 重查后幂等视为成功。
            info!(
                "[PracticeJudge] judge container name conflict during concurrent deploy — re-inspect"
            );
            match runtime
                .inspect_container(PRACTICE_JUDGE_CONTAINER_NAME)
                .await
            {
                Ok(state) if state.running => {
                    info!("[PracticeJudge] concurrent deploy won — judge running");
                    return Ok(());
                }
                Ok(_) => {
                    return Err(AwdpError::Docker(format!(
                        "concurrent judge deploy left non-running container: {e}"
                    )));
                }
                Err(inspect_err) => {
                    return Err(AwdpError::Docker(format!(
                        "judge deploy name conflict and re-inspect failed: {e} / {inspect_err}"
                    )));
                }
            }
        }
        Err(e) => return Err(AwdpError::Docker(format!("deploy practice judge: {e}"))),
    };

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

    info!(
        container = %PRACTICE_JUDGE_CONTAINER_NAME,
        id = %handle.container_id,
        ip = %config.practice_judge_ip,
        "AWDP practice judge (pull worker) deployed"
    );
    Ok(())
}

fn container_conflict(e: &anyhow::Error) -> bool {
    // fcmc create_and_start 返回 anyhow::Error，沿 cause 链找 bollard 409（容器名冲突）。
    let mut cause: Option<&dyn std::error::Error> = Some(e.as_ref());
    while let Some(err) = cause {
        if let Some(bollard::errors::Error::DockerResponseServerError {
            status_code: 409, ..
        }) = err.downcast_ref::<bollard::errors::Error>()
        {
            return true;
        }
        cause = err.source();
    }
    false
}
