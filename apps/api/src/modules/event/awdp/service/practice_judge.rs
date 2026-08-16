//! AWDP JudgeServer 服务：data 子网 + JudgeServer 部署（每赛事独立网络模型）。
//!
//! 架构（Pull + Lease 模型，plan §12-§19）：
//!   - 每个 AWDP 赛事拥有**独立** data network（`fctf-awdp-{event_id 前 12}`），
//!     该赛事全部 GameBox 实例 + 该赛事专属 JudgeServer 都在此网络内；
//!     练习（AWDPlusPractice 虚拟赛事）沿用固定网络 `fctf-awdp-practice`（兼容既有部署）；
//!   - JudgeServer 是**主动拉取**的 worker（claim/heartbeat/result），平台不再 push /batch；
//!   - 本模块负责：赛事子网 ensure（含子网池分配）、JudgeServer 容器部署（幂等）、
//!     赛事网络清理（finish 后 best-effort）。

use bollard::Docker;
use fcmc::{ContainerRuntime, ContainerSpec, DockerContainerRuntime, ImageRuntime, ResourceLimits};
use ipnetwork::IpNetwork;
use tracing::{info, warn};
use uuid::Uuid;

use crate::core::config::AwdpStaticConfig;
use crate::entity;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::judge::{
        CONTROL_NETWORK_NAME, JUDGE_DATA_ALIAS, PRACTICE_JUDGE_CONTAINER_NAME, PRACTICE_JUDGE_PORT,
        PRACTICE_NETWORK_NAME, event_acl_table_name, event_judge_container_name,
        event_judge_worker_id, event_network_name, is_practice_event, practice_judge_token,
    },
    repo::event_network_repo,
};

/// 收集 docker 实际已存在的网络子网（IPAM config 里的 subnet）——
/// DB 记录被删但网络残留时，分配器据此跳过已占用网段，避免 "Pool overlaps"。
async fn docker_occupied_subnets(docker: &Docker) -> Vec<IpNetwork> {
    use bollard::network::ListNetworksOptions;
    let Ok(nets) = docker
        .list_networks(Some(ListNetworksOptions::<String> {
            ..Default::default()
        }))
        .await
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for net in nets {
        if let Some(ipam) = net.ipam
            && let Some(configs) = ipam.config
        {
            for cfg in configs {
                if let Some(subnet) = cfg.subnet {
                    if let Ok(ipnet) = subnet.parse::<IpNetwork>() {
                        out.push(ipnet);
                    }
                }
            }
        }
    }
    out
}

/// 幂等 ensure 赛事专属 docker 子网（不存在则创建；已存在复用）。
///
/// 子网：该赛事全部 GameBox 实例 + JudgeServer 共用（data plane）。`internal=false`
/// （靶机需要出网，玩家仍经宿主端口访问 public endpoint；子网仅用于实例间与 Judge 互访）。
///
/// 练习（is_practice_event）：固定网络 `fctf-awdp-practice`（config.awdp.practice_network_subnet /
/// practice_judge_ip），不落 awdp_event_networks 表；
/// 比赛：从 config.awdp.network_pool 池分配独立子网（每赛事互不重叠，落表记录 judge 固定 IP
/// 与动态池），Docker 网络缺失时按行创建。
///
/// 注意：**不用 `fcmc::create_network`**（它会先 remove 再 create；并发 ensure
/// 会拆掉正在使用的子网）。这里 inspect 后仅在缺失时创建，重复创建竞态按成功处理。
#[allow(deprecated)] // bollard CreateNetworkOptions（fcmc 同款；新 models 结构字段未稳定）
pub async fn ensure_event_network(
    db: &sea_orm::DatabaseConnection,
    docker: &Docker,
    config: &AwdpStaticConfig,
    event_id: Uuid,
) -> AwdpResult<String> {
    if is_practice_event(event_id) {
        return ensure_fixed_network(docker, config, event_id).await;
    }
    // 比赛赛事：分配（幂等）→ 按行 ensure docker 网络 → ACL。
    let network_name = event_network_name(event_id);
    let pool = config.network_pool.parse::<IpNetwork>().map_err(|e| {
        AwdpError::Internal(format!(
            "invalid awdp.network_pool {}: {e}",
            config.network_pool
        ))
    })?;
    // docker 实际占用子网（含残留网络）并入分配检查，防止撞池。
    let docker_subnets = docker_occupied_subnets(docker).await;
    let row = event_network_repo::allocate_event_network(
        db,
        event_id,
        &network_name,
        &pool,
        config.event_netmask as u8,
        &docker_subnets,
    )
    .await?;
    ensure_network_exists(docker, &row).await?;
    apply_acl_best_effort(docker, &row.network_name, &row.judge_ip.ip().to_string()).await;
    Ok(network_name)
}

/// 练习固定网络 ensure：不存在则创建；存在但子网/网关与配置漂移 → **守护式自动替换**
/// （无运行中实例时：摘 judge → 删旧网 → 建新网；有实例在跑则拒换，等下个周期）。
async fn ensure_fixed_network(
    docker: &Docker,
    config: &AwdpStaticConfig,
    event_id: Uuid,
) -> AwdpResult<String> {
    use bollard::network::CreateNetworkOptions;

    let runtime = DockerContainerRuntime::new(docker.clone());

    // 1. 已存在且与配置一致（子网 + 显式网关）→ 复用。
    if practice_network_matches(docker, config).await {
        return Ok(PRACTICE_NETWORK_NAME.to_string());
    }

    // 2. 存在但漂移（容量/网段参数已变更）→ 守护式替换。
    if runtime
        .inspect_network(PRACTICE_NETWORK_NAME)
        .await
        .map(|s| s.exists)
        .unwrap_or(false)
    {
        if practice_network_has_instances(docker).await {
            warn!(
                "[PracticeNet] 练习子网配置已变更（{}），但练习网络上有运行中实例——拒绝自动替换，等待实例清空后下个周期重试",
                config.practice_network_subnet
            );
            // 保持旧网络可用；cron 每 30s 重查，清空后自动执行替换。
            return Ok(PRACTICE_NETWORK_NAME.to_string());
        }
        info!(
            subnet = %config.practice_network_subnet,
            "[PracticeNet] 练习子网漂移，自动替换：摘 judge → 删旧网 → 建新网"
        );
        // judge 必须先摘，否则网络删不掉（容器仍附着在网络上）。
        match runtime
            .stop_and_remove(PRACTICE_JUDGE_CONTAINER_NAME, fcmc::IMMEDIATE_STOP_TIMEOUT)
            .await
        {
            Ok(_) => info!("[PracticeNet] 旧 judge 已移除"),
            Err(e) => warn!(error = %e, "[PracticeNet] judge 移除失败（容错，deploy 会重建）"),
        }
        match runtime.remove_network(PRACTICE_NETWORK_NAME).await {
            Ok(_) => info!("[PracticeNet] 旧练习网络已移除"),
            Err(e) => warn!(error = %e, "[PracticeNet] 旧练习网络移除失败（容错）"),
        }
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
                // 显式 gateway = 平台 internal API 地址（宿主桥绑定该 IP），
                // 重建网络后 judge→platform 通路不漂移。
                gateway: platform_url_gateway(&config.platform_internal_url),
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
    apply_acl_best_effort(docker, PRACTICE_NETWORK_NAME, &config.practice_judge_ip).await;
    let _ = event_id;
    Ok(PRACTICE_NETWORK_NAME.to_string())
}

/// 练习网络是否与配置一致（子网 + 显式网关）。网络不存在视为不匹配（走创建）。
async fn practice_network_matches(docker: &Docker, config: &AwdpStaticConfig) -> bool {
    let Ok(net) = docker
        .inspect_network(
            PRACTICE_NETWORK_NAME,
            None::<bollard::network::InspectNetworkOptions<String>>,
        )
        .await
    else {
        return false;
    };
    let Some(cfg) = net.ipam.and_then(|i| i.config).and_then(|mut c| c.pop()) else {
        return false;
    };
    let Ok(want_subnet) = config
        .practice_network_subnet
        .parse::<ipnetwork::Ipv4Network>()
    else {
        return false;
    };
    let subnet_ok = cfg
        .subnet
        .as_deref()
        .and_then(|s| s.parse::<ipnetwork::Ipv4Network>().ok())
        == Some(want_subnet);
    let gateway_ok = match platform_url_gateway(&config.platform_internal_url) {
        Some(want) => cfg.gateway.as_deref() == Some(want.as_str()),
        None => true, // 配置无显式网关期望 → 不校验网关
    };
    subnet_ok && gateway_ok
}

/// 练习网络上是否有非 judge 的容器（运行中练习实例）。有则禁止网络替换。
async fn practice_network_has_instances(docker: &Docker) -> bool {
    match docker
        .inspect_network(
            PRACTICE_NETWORK_NAME,
            None::<bollard::network::InspectNetworkOptions<String>>,
        )
        .await
    {
        Ok(net) => net.containers.unwrap_or_default().values().any(|c| {
            c.name.as_deref().map(|n| n.trim_start_matches('/'))
                != Some(PRACTICE_JUDGE_CONTAINER_NAME)
        }),
        Err(_) => false,
    }
}

/// 动态实例分配池：子网后半段（/24 → x.y.z.128/25；/23 → x.y.z+1.0/24；类推）。
/// 前半段保留给固定地址（JudgeServer 等）；与赛事网络 `dynamic_pool_for` 同式，
/// 任意前缀均适用（不再局限于 /24 旧行为）。
fn dynamic_ip_range(subnet: &str) -> Option<String> {
    use std::net::Ipv4Addr;
    let ipnet = subnet.parse::<ipnetwork::Ipv4Network>().ok()?;
    let prefix = ipnet.prefix();
    if prefix >= 31 {
        return None; // /31 /32 无可用主机，不设 ip_range
    }
    let half = 1u32 << (31 - prefix);
    let base = u32::from(ipnet.network());
    Some(format!("{}/{}", Ipv4Addr::from(base + half), prefix + 1))
}

/// 从 `platform_internal_url` 提取主机 IP（练习网络显式 gateway）。
///
/// 宿主部署时平台 API 绑定在 data 网络网关地址上（config 注释契约）：
/// 显式指定 gateway = 平台可达地址，保证 docker 重建练习网络后 judge→platform
/// 通路不因网关漂移（默认取子网首地址）而断开。非 IP 主机名返回 None（docker 默认网关）。
fn platform_url_gateway(url: &str) -> Option<String> {
    let authority = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = authority.split(['/', '?']).next()?;
    let host = authority
        .rsplit_once(':')
        .filter(|(_, port)| port.parse::<u16>().is_ok())
        .map(|(h, _)| h)
        .unwrap_or(authority);
    host.parse::<std::net::Ipv4Addr>()
        .ok()
        .map(|ip| ip.to_string())
}

/// 按 DB 行 ensure 赛事网络（网络不存在时按行内 subnet/dynamic_pool 创建）。
#[allow(deprecated)] // bollard CreateNetworkOptions
async fn ensure_network_exists(
    docker: &Docker,
    row: &entity::awdp_event_networks::Model,
) -> AwdpResult<()> {
    use bollard::network::CreateNetworkOptions;

    let runtime = DockerContainerRuntime::new(docker.clone());
    if runtime
        .inspect_network(&row.network_name)
        .await
        .map(|s| s.exists)
        .unwrap_or(false)
    {
        return Ok(());
    }

    let conf = CreateNetworkOptions {
        name: row.network_name.clone(),
        driver: "bridge".to_string(),
        internal: false,
        check_duplicate: true,
        ipam: bollard::secret::Ipam {
            config: Some(vec![bollard::secret::IpamConfig {
                subnet: Some(row.subnet_cidr.to_string()),
                ip_range: Some(row.dynamic_pool_cidr.to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        },
        ..Default::default()
    };
    match docker.create_network(conf).await {
        Ok(_) => {
            info!(
                network = %row.network_name,
                subnet = %row.subnet_cidr,
                "AWDP event docker network ensured"
            );
        }
        Err(e) if network_already_exists(&e) => {}
        Err(e) => {
            return Err(AwdpError::Docker(format!("create event network: {e}")));
        }
    }
    Ok(())
}

async fn apply_acl_best_effort(docker: &Docker, network_name: &str, judge_ip: &str) {
    let _ = crate::modules::event::awdp::service::practice_acl::apply_practice_acl(
        docker,
        network_name,
        judge_ip,
        &crate::modules::event::awdp::service::practice_acl::DEFAULT_BLOCKED_HOST_PORTS,
    )
    .await;
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

/// 部署赛事 JudgeServer 容器（Pull worker，幂等：已 running 跳过）。
///
/// **默认启用**：练习 run Launch / 练习实例启动时自动调用（无管理端配置页）。
/// 容器进入赛事 data 子网固定 IP（练习 = config.practice_judge_ip；比赛 = 表内分配）；
/// env 注入平台 internal API 基址 / worker 配置 / 内部令牌（令牌由平台 Secret HKDF
/// 派生，不落库不落日志）。
///
/// 并发安全：多实例并发部署时容器名冲突视为「对方已创建成功」，重查后按成功返回。
pub async fn deploy_judge(
    db: &sea_orm::DatabaseConnection,
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

    // 1. 子网 ensure + data plane ACL（best-effort）。
    let (network_name, judge_ip) = if is_practice_event(event_id) {
        (
            PRACTICE_NETWORK_NAME.to_string(),
            config.practice_judge_ip.clone(),
        )
    } else {
        let row = event_network_repo::require_by_event_id(db, event_id)
            .await
            .map_err(|_| {
                AwdpError::InvalidState(format!(
                    "AWDP event network not allocated (event={event_id})"
                ))
            })?;
        ensure_network_exists(docker, &row).await?;
        (row.network_name.clone(), row.judge_ip.ip().to_string())
    };
    let _ =
        crate::modules::event::awdp::service::practice_acl::ensure_control_network(docker).await?;
    apply_acl_best_effort(docker, &network_name, &judge_ip).await;

    // 2. 镜像 ensure（pin 不可变）。
    ImageRuntime::ensure_image(&runtime, &config.practice_judgeserver_image, None)
        .await
        .map_err(|e| {
            AwdpError::Docker(format!(
                "ensure judge image {}: {e}",
                config.practice_judgeserver_image
            ))
        })?;

    // 3. 已有容器：running 且 env 与期望一致 → 幂等成功；env 漂移/停止/消失 → 重建。
    let container_name = event_judge_container_name(event_id);
    let worker_id = event_judge_worker_id(event_id);
    match runtime.inspect_container(&container_name).await {
        Ok(state)
            if state.running
                && running_judge_env_matches(docker, config, event_id, &container_name).await =>
        {
            info!(container = %container_name, "[Judge] judge container already running with matching env — skip");
            return Ok(());
        }
        Ok(_) => {
            warn!(container = %container_name, "[Judge] judge container 不存在/非 running/env 与期望不一致 — recreating");
            let _ = runtime
                .stop_and_remove(&container_name, fcmc::IMMEDIATE_STOP_TIMEOUT)
                .await;
        }
        Err(_) => {} // 不存在
    }

    let token = practice_judge_token(jwt_secret);
    let spec = ContainerSpec {
        name: container_name.clone(),
        image: config.practice_judgeserver_image.clone(),
        env: vec![
            format!("PLATFORM_INTERNAL_URL={}", config.platform_internal_url),
            format!("EVENT_ID={event_id}"),
            format!("INTERNAL_TOKEN={token}"),
            format!("WORKER_ID={worker_id}"),
            format!("DATA_LISTEN_ADDR=0.0.0.0:{PRACTICE_JUDGE_PORT}"),
            // Pull worker 配置（bounded concurrency）。
            "CLAIM_BATCH=16".to_string(),
            "MAX_CONCURRENCY=8".to_string(),
            "POLL_INTERVAL_SECS=5".to_string(),
            "HEARTBEAT_INTERVAL_SECS=30".to_string(),
        ],
        labels: std::collections::HashMap::from([
            ("io.floatctf.managed".into(), "true".into()),
            ("io.floatctf.resource".into(), "awdp-judge".into()),
            ("io.floatctf.event_id".into(), event_id.to_string()),
        ]),
        network_name: Some(network_name.clone()),
        fixed_ip: Some(judge_ip.clone()),
        // data plane DNS alias：GameBox 内 `judge-server` 可解析（不暴露动态 IP 契约）。
        network_aliases: vec![JUDGE_DATA_ALIAS.to_string()],
        port_bindings: vec![],
        auto_remove: false,
        resources: ResourceLimits {
            cpu_millis: Some(500),
            memory_bytes: Some(256 * 1024 * 1024),
            pids_limit: Some(128),
            // 用户决策：与 nginx 一致使用 Docker 默认能力集（含 NET_BIND_SERVICE，data plane
            // 监听 80 无需特殊处理）；不再做 cap_drop ALL 收敛（plan §37 hardening 有意放松）。
            cap_drop: vec![],
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
            info!("[Judge] judge container name conflict during concurrent deploy — re-inspect");
            match runtime.inspect_container(&container_name).await {
                Ok(state) if state.running => {
                    info!("[Judge] concurrent deploy won — judge running");
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
        Err(e) => return Err(AwdpError::Docker(format!("deploy judge: {e}"))),
    };

    // 4. 加入 control 网络（internal=true；GameBox 无权加入）。
    //    注意：control 网络无出站 → JudgeServer 到宿主 API 走 data 网络网关
    //    （PLATFORM_INTERNAL_URL 由配置给出，host firewall 限制 GameBox 访问）。
    if let Err(e) = docker
        .connect_network(
            CONTROL_NETWORK_NAME,
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
            "[Judge] connect control network failed (best-effort)"
        );
    }

    // 5. 记录真实 docker network id（赛事网络；练习跳过）。
    if !is_practice_event(event_id) {
        match runtime.inspect_network(&network_name).await {
            Ok(net) => {
                info!(event_id = %event_id, network = %network_name, net_id = %net.network_id, "[Judge] mark_deployed start");
                match event_network_repo::mark_deployed(db, event_id, &net.network_id).await {
                    Ok(_) => info!(event_id = %event_id, "[Judge] mark_deployed ok"),
                    Err(e) => {
                        warn!(event_id = %event_id, error = %e, "[Judge] mark_deployed failed")
                    }
                }
            }
            Err(e) => warn!(
                network = %network_name,
                error = %e,
                "[Judge] inspect network for mark_deployed failed"
            ),
        }
    }

    info!(
        container = %container_name,
        id = %handle.container_id,
        ip = %judge_ip,
        network = %network_name,
        "AWDP judge (pull worker) deployed"
    );
    Ok(())
}

/// 幂等 ensure 练习环境（练习 data 网络 + control 网络 + 练习 JudgeServer 容器）。
///
/// 供两类调度任务共用（练习常驻虚拟赛事，无赛事启动/实例事件驱动，必须主动 ensure）：
///   - `system.practice.check`（startup）：API 启动即就绪；
///   - `awdp.practice.judge`（cron 30s）：docker 清理/容器被杀后自动自愈。
///
/// 与 `runtime::start_instance` 的练习分支调用序列一致（ensure 网络 → deploy judge），
/// 两者均幂等：网络已存在复用、judge 容器 running 且 env 匹配则跳过。
pub async fn ensure_practice_environment(
    db: &sea_orm::DatabaseConnection,
    docker: &Docker,
    config: &AwdpStaticConfig,
    jwt_secret: &[u8],
) -> AwdpResult<()> {
    let event_id = crate::core::system_ids::EVENT_PRACTICE_AWDP;
    ensure_event_network(db, docker, config, event_id).await?;
    deploy_judge(db, docker, config, jwt_secret, event_id).await
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

/// 已运行的 judge 容器是否与期望一致（镜像 + PLATFORM_INTERNAL_URL / EVENT_ID / WORKER_ID）。
/// 不一致说明容器由旧配置/旧镜像/测试环境部署（如 host.docker.internal 残留、镜像改名），
/// 必须重建，否则真实实例的 /flag、claim、result 全链路都会 502。
async fn running_judge_env_matches(
    docker: &bollard::Docker,
    config: &AwdpStaticConfig,
    event_id: Uuid,
    container_name: &str,
) -> bool {
    let Ok(info) = docker
        .inspect_container(
            container_name,
            None::<bollard::container::InspectContainerOptions>,
        )
        .await
    else {
        return false;
    };
    let envs: Vec<String> = info
        .config
        .as_ref()
        .and_then(|c| c.env.clone())
        .unwrap_or_default();
    let get = |k: &str| envs.iter().find_map(|e| e.strip_prefix(k)).map(str::trim);
    let want_event = event_id.to_string();
    let want_worker = event_judge_worker_id(event_id);
    // 镜像 tag 比对：镜像改名/升级后旧镜像容器视为漂移，重建（配合 ensure 周期任务自愈）。
    let image_matches = info
        .config
        .as_ref()
        .and_then(|c| c.image.clone())
        .is_some_and(|img| img == config.practice_judgeserver_image);
    image_matches
        && get("PLATFORM_INTERNAL_URL=") == Some(config.platform_internal_url.trim())
        && get("EVENT_ID=") == Some(want_event.as_str())
        && get("WORKER_ID=") == Some(want_worker.as_str())
}

/// 清理赛事网络资源（finish 后 best-effort）：停/删 judge 容器 → 删 ACL 表 → 删网络 → 标记 released。
/// 练习（常驻虚拟赛事）不做清理。
pub async fn cleanup_event_network(
    db: &sea_orm::DatabaseConnection,
    docker: &Docker,
    event_id: Uuid,
) -> AwdpResult<()> {
    if is_practice_event(event_id) {
        return Ok(());
    }
    let runtime = DockerContainerRuntime::new(docker.clone());

    // 1. 停/删 judge 容器（404 容错）。
    let container_name = event_judge_container_name(event_id);
    match runtime
        .stop_and_remove(&container_name, fcmc::IMMEDIATE_STOP_TIMEOUT)
        .await
    {
        Ok(_) => info!(container = %container_name, "AWDP event judge container removed"),
        Err(e) => {
            warn!(container = %container_name, error = %e, "AWDP event judge remove failed (tolerated)")
        }
    }

    // 2. 按 DB 行删除网络 + ACL 表。
    if let Ok(row) = event_network_repo::require_by_event_id(db, event_id).await {
        // ACL 表删除（best-effort）。
        let _ = crate::modules::event::awdp::service::practice_acl::remove_acl_table(
            &event_acl_table_name(event_id),
        )
        .await;
        // 网络删除（404 容错）。
        match runtime.remove_network(&row.network_name).await {
            Ok(_) => info!(network = %row.network_name, "AWDP event network removed"),
            Err(e) => warn!(
                network = %row.network_name,
                error = %e,
                "AWDP event network remove failed (tolerated)"
            ),
        }
        let _ = event_network_repo::mark_released(db, event_id).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_pool_is_subnet_second_half_any_prefix() {
        // /24 → /25（旧语义不变）
        assert_eq!(
            dynamic_ip_range("10.42.2.0/24").as_deref(),
            Some("10.42.2.128/25")
        );
        // /23 → /24（本次扩容目标）
        assert_eq!(
            dynamic_ip_range("10.42.2.0/23").as_deref(),
            Some("10.42.3.0/24")
        );
        // /22 → /23（10.42.0.0/22 覆盖 .0~.3.x，后半段 512 个 = 10.42.2.0/23）
        assert_eq!(
            dynamic_ip_range("10.42.0.0/22").as_deref(),
            Some("10.42.2.0/23")
        );
        // /8 → /9
        assert_eq!(
            dynamic_ip_range("10.0.0.0/8").as_deref(),
            Some("10.128.0.0/9")
        );
        // /31 /32 无可用主机；非法输入
        assert!(dynamic_ip_range("10.42.2.0/31").is_none());
        assert!(dynamic_ip_range("not-a-cidr").is_none());
    }

    #[test]
    fn platform_gateway_extracts_host_ip_only() {
        assert_eq!(
            platform_url_gateway("http://10.42.2.128:9090").as_deref(),
            Some("10.42.2.128")
        );
        assert_eq!(
            platform_url_gateway("http://10.42.2.128").as_deref(),
            Some("10.42.2.128")
        );
        // 主机名（非 IP）→ 不设显式网关
        assert_eq!(platform_url_gateway("https://api.example.com:9090"), None);
        assert_eq!(platform_url_gateway(""), None);
        // 无 scheme 也兼容
        assert_eq!(
            platform_url_gateway("10.42.2.128:9090"),
            Some("10.42.2.128".to_string())
        );
    }
}
