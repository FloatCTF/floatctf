//! AWDP 实例运行时服务：按需启动 GameBox（plan §13/§14/§15，run 中心化）。
//!
//! 关键不变量：
//!   - 逻辑实例（instances + awdp_instances）唯一：每 subject × run × gamebox 至多一个；
//!   - 用户可见 public endpoint 跨 reset 稳定（instance_endpoints 保存 host 端口，
//!     recreate 时复用 host_port 绑定）；
//!   - 一个 container 按 healthchecks 暴露多个 HTTP/TCP 端点（protocol+port 去重）；
//!   - 不使用 WireGuard（V1 沿用 Challenge 的随机 high port 暴露模型）。

use std::collections::HashMap;

use bollard::Docker;
use chrono::Utc;
use fcmc::{ContainerRuntime, DockerContainerRuntime, ImageRuntime, PortBinding, ResourceLimits};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::entity::{
    awdp_event_gameboxes, awdp_instances, awdp_runs, event_instances, gameboxes,
    instance_endpoints, sea_orm_active_enums::AwdpPhase,
};
use crate::infrastructure::settings::get_setting;
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    domain::{flag::awdp_flag, judge::PRACTICE_NETWORK_NAME},
    repo::{break_repo, event_gamebox_repo, instance_repo, run_repo},
};
use crate::modules::gamebox::{effective_image_ref_from_gamebox, healthcheck::parse_healthchecks};

/// 实例运行时视图（API DTO 数据源）。
#[derive(Debug, Clone)]
pub struct InstanceView {
    pub instance_id: Uuid,
    pub gamebox_id: Uuid,
    pub gamebox_name: String,
    pub gamebox_category: String,
    pub runtime_state: String,
    pub runtime_generation: i64,
    pub broken: bool,
    pub endpoints: Vec<EndpointView>,
}

#[derive(Debug, Clone)]
pub struct EndpointView {
    pub protocol: String,
    pub container_port: u16,
    pub public_host: String,
    pub public_port: u16,
}

/// 解析 participant（user XOR team）。
#[derive(Debug, Clone, Copy)]
pub struct Subject {
    pub user_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
}

impl Subject {
    pub fn user(u: Uuid) -> Self {
        Self {
            user_id: Some(u),
            team_id: None,
        }
    }
    pub fn team(t: Uuid) -> Self {
        Self {
            user_id: None,
            team_id: Some(t),
        }
    }
}

/// 解析 run 的 gamebox 运行规格：
///   practice（run.gamebox_id 命中）→ GameBox 默认规格（虚拟 event，无挂载行）；
///   competition（真实赛事）→ 赛事挂载行 override（eg）或 GameBox 默认。
pub async fn resolve_run_gamebox_spec(
    db: &DatabaseConnection,
    run: &awdp_runs::Model,
    gamebox_id: Uuid,
) -> AwdpResult<(Option<awdp_event_gameboxes::Model>, gameboxes::Model)> {
    if run.gamebox_id == Some(gamebox_id) {
        // practice：虚拟 event，直接 GameBox 默认规格。
        let gamebox = event_gamebox_repo::find_gamebox_identity(db, gamebox_id).await?;
        return Ok((None, gamebox));
    }
    // competition：gamebox 必须已挂载到赛事。
    let event_id = run.event_id;
    let eg = event_gamebox_repo::find_for_event_and_gamebox(db, event_id, gamebox_id)
        .await?
        .ok_or_else(|| AwdpError::Validation("gamebox 未挂载到本赛事".into()))?;
    let gamebox = event_gamebox_repo::find_gamebox_identity(db, gamebox_id).await?;
    Ok((Some(eg), gamebox))
}

/// 启动/复用实例（幂等：已 running 直接返回）。
///
/// `awdp_config` 提供练习子网配置（practice 实例启动前 ensure 练习 docker 子网）。
#[allow(clippy::too_many_arguments)]
pub async fn start_instance(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    awdp_config: &crate::core::config::AwdpStaticConfig,
    run_id: Uuid,
    gamebox_id: Uuid,
    subject: Subject,
    flag_prefix: &str,
) -> AwdpResult<InstanceView> {
    // 1. 阶段门：Break / Fix 才允许启动。
    let run = run_repo::require_by_id(db, run_id).await?;
    if !matches!(run.phase, AwdpPhase::Break | AwdpPhase::Fix) {
        return Err(AwdpError::InvalidState(format!(
            "AWDP phase {:?} does not allow instance start",
            run.phase
        )));
    }

    // 2. 解析运行规格。
    let (eg, gamebox) = resolve_run_gamebox_spec(db, &run, gamebox_id).await?;
    let image_ref = effective_image_ref_from_gamebox(&gamebox).map_err(AwdpError::from)?;
    let node_ip = get_setting(db, "NODE_IP")
        .await
        .map_err(|e| AwdpError::Internal(format!("get NODE_IP: {e}")))?;

    // 2.5 练习实例：ensure 统一练习 docker 子网（幂等；所有练习 GameBox 同一子网）。
    if eg.is_none() {
        super::practice_judge::ensure_practice_network(docker, awdp_config).await?;
    }

    // 3. 查找/创建逻辑实例。
    let (instance, ext) = match instance_repo::find_instance_for_subject(
        db,
        run_id,
        gamebox_id,
        subject.user_id,
        subject.team_id,
    )
    .await?
    {
        Some(found) => found,
        None => {
            let instance_id = Uuid::new_v4();
            let container_name =
                format!("awdp-{}", &instance_id.to_string().replace('-', "")[..20]);
            instance_repo::create_instance(
                db,
                run_id,
                gamebox_id,
                subject.user_id,
                subject.team_id,
                &container_name,
                &image_ref,
            )
            .await?
        }
    };

    // 4. 已 running → 幂等返回。
    if instance.runtime_state == "running" {
        return build_view(db, &instance, &ext, &eg, &gamebox).await;
    }

    // 5. 复用已有 host 端口（endpoint 稳定）或全新分配。
    let existing_endpoints = load_endpoints(db, instance.id).await?;
    let runtime = DockerContainerRuntime::new(docker.clone());

    // 镜像 ensure（pin 不可变）。
    ImageRuntime::ensure_image(&runtime, &image_ref, None)
        .await
        .map_err(|e| AwdpError::Docker(format!("ensure image {image_ref}: {e}")))?;

    let flag = awdp_flag(
        jwt_secret,
        run_id,
        gamebox_id,
        subject.user_id,
        subject.team_id,
        flag_prefix,
    );
    let generation = instance.runtime_generation;
    let (instance, _endpoints) = launch_container(
        db,
        &runtime,
        run_id,
        &instance,
        &ext,
        &eg,
        &gamebox,
        &image_ref,
        &flag,
        &existing_endpoints,
        &node_ip,
        generation,
    )
    .await?;

    build_view(db, &instance, &ext, &eg, &gamebox).await
}

/// 启动（或 reset 后重建）容器并发布端点。
/// 复用既有 instance_endpoints 的 host 端口 → public endpoint 稳定。
#[allow(clippy::too_many_arguments)]
async fn launch_container(
    db: &DatabaseConnection,
    runtime: &DockerContainerRuntime,
    run_id: Uuid,
    instance: &event_instances::Model,
    ext: &awdp_instances::Model,
    eg: &Option<awdp_event_gameboxes::Model>,
    gamebox: &gameboxes::Model,
    image_ref: &str,
    flag: &str,
    existing_endpoints: &[instance_endpoints::Model],
    node_ip: &str,
    generation: i64,
) -> AwdpResult<(event_instances::Model, Vec<instance_endpoints::Model>)> {
    // 按 healthchecks 暴露端口（protocol+port 去重）；复用既有 host 端口。
    let healthchecks = parse_healthchecks(&healthchecks_of(eg, gamebox))?;
    let mut seen: HashMap<(String, u16), ()> = HashMap::new();
    let mut port_bindings: Vec<PortBinding> = Vec::new();
    let mut endpoint_specs: Vec<(String, u16)> = Vec::new(); // (protocol, container_port)
    for check in &healthchecks {
        let (protocol, port) = match check {
            crate::modules::gamebox::healthcheck::AppHealthcheck::Http { port, .. } => {
                ("http", *port)
            }
            crate::modules::gamebox::healthcheck::AppHealthcheck::Tcp { port } => ("tcp", *port),
        };
        if seen.insert((protocol.to_string(), port), ()).is_some() {
            continue;
        }
        let host_port = existing_endpoints
            .iter()
            .find(|e| e.protocol == protocol && e.container_port == port as i32)
            .map(|e| e.public_port.to_string());
        port_bindings.push(PortBinding {
            container_port: format!("{port}/tcp"),
            host_ip: Some("0.0.0.0".into()),
            host_port,
        });
        endpoint_specs.push((protocol.to_string(), port));
    }
    if port_bindings.is_empty() {
        return Err(AwdpError::Validation(format!(
            "GameBox {} 没有声明 healthchecks，无法发布端点",
            gamebox.safe_name
        )));
    }

    let username = gamebox
        .username
        .clone()
        .unwrap_or_else(|| "ctf".to_string());
    let password = Uuid::new_v4().to_string();
    let env = vec![
        format!("FLAG={flag}"),
        format!("GAMEBOX_USERNAME={username}"),
        format!("GAMEBOX_USERPASS={password}"),
        format!(
            "FLOATCTF_SOURCE_DIR={}",
            gamebox.awdp_source_code_dir.as_deref().unwrap_or("/app")
        ),
    ];

    let labels = HashMap::from([
        ("io.floatctf.managed".into(), "true".into()),
        ("io.floatctf.resource".into(), "awdp-instance".into()),
        ("io.floatctf.run_id".into(), run_id.to_string()),
        ("io.floatctf.gamebox_id".into(), ext.gamebox_id.to_string()),
        ("io.floatctf.instance_id".into(), instance.id.to_string()),
    ]);

    let resources = match eg {
        Some(eg) => ResourceLimits {
            cpu_millis: Some(eg.cpu_millis),
            memory_bytes: Some(eg.memory_bytes),
            pids_limit: Some(eg.pids_limit),
            cap_drop: vec!["NET_ADMIN".into(), "NET_RAW".into(), "SYS_ADMIN".into()],
            privileged: false,
            extra_hosts: vec![],
        },
        None => ResourceLimits {
            cpu_millis: Some(gamebox.recommended_cpu_millis),
            memory_bytes: Some(gamebox.recommended_memory_bytes),
            pids_limit: Some(gamebox.recommended_pids_limit),
            cap_drop: vec!["NET_ADMIN".into(), "NET_RAW".into(), "SYS_ADMIN".into()],
            privileged: false,
            extra_hosts: vec![],
        },
    };

    let spec = fcmc::ContainerSpec {
        name: instance.container_name.clone(),
        image: image_ref.to_string(),
        env,
        labels,
        // 练习实例（eg=None）加入统一练习子网（JudgeServer 与实例互访）；
        // 竞赛实例保持默认 bridge（host 端口发布不变）。
        network_name: if eg.is_none() {
            Some(PRACTICE_NETWORK_NAME.to_string())
        } else {
            None
        },
        fixed_ip: None,
        network_aliases: vec![],
        port_bindings,
        auto_remove: true,
        resources,
        network_mode: None,
        healthcheck: None,
    };

    // create + start + inspect 端口。
    let handle = runtime
        .create_and_start(spec)
        .await
        .map_err(|e| AwdpError::Docker(format!("start instance container: {e}")))?;
    let state = runtime
        .inspect_container(&handle.container_id)
        .await
        .map_err(|e| AwdpError::Docker(format!("inspect instance container: {e}")))?;

    // 写 instance_endpoints（幂等 upsert，保留既有 public_port）。
    let mut endpoints = Vec::new();
    for (protocol, container_port) in &endpoint_specs {
        let public_port = state
            .published_ports
            .get(&format!("{container_port}/tcp"))
            .copied()
            .ok_or_else(|| {
                AwdpError::Docker(format!(
                    "host port not published for {protocol}:{container_port}"
                ))
            })?;
        upsert_endpoint(
            db,
            instance.id,
            protocol,
            *container_port,
            node_ip,
            public_port,
        )
        .await?;
        endpoints.push(load_endpoints(db, instance.id).await?);
    }

    // 更新 instances 行（返回最新模型）。
    let instance = instance_repo::update_runtime_state(
        db,
        instance.id,
        "running",
        Some(&handle.container_id),
        generation,
    )
    .await?;
    Ok((instance, endpoints.concat()))
}

/// healthcheck 源：赛事 override > GameBox 默认。
fn healthchecks_of(
    eg: &Option<awdp_event_gameboxes::Model>,
    gamebox: &gameboxes::Model,
) -> serde_json::Value {
    eg.as_ref()
        .and_then(|e| e.healthcheck_override_json.clone())
        .unwrap_or_else(|| {
            gamebox
                .healthchecks_json
                .clone()
                .unwrap_or_else(|| serde_json::json!([]))
        })
}

/// Reset：移除物理容器并从 pristine 镜像重建（保留逻辑实例 + 端点分配）。
/// 与 Patch / Evaluation / Manual check 使用同一把 instance advisory lock。
pub async fn reset_instance(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    instance_id: Uuid,
    subject: Subject,
    flag_prefix: &str,
) -> AwdpResult<InstanceView> {
    // 归属校验先行（拒绝未授权触发 reset）。
    let (instance, _ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
    assert_owner(&instance, subject)?;
    let lock = super::lock::InstanceAdvisoryLock::acquire(db, instance_id).await?;
    let result = reset_instance_unchecked(db, docker, jwt_secret, instance_id, flag_prefix).await;
    lock.release().await;
    result
}

/// 无归属校验的 reset（事件驱动 Break→Fix / 管理端，调用方负责授权）。
pub async fn reset_instance_unchecked(
    db: &DatabaseConnection,
    docker: &Docker,
    jwt_secret: &[u8],
    instance_id: Uuid,
    flag_prefix: &str,
) -> AwdpResult<InstanceView> {
    let (instance, ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
    let run = run_repo::require_by_id(db, ext.run_id).await?;
    let (eg, gamebox) = resolve_run_gamebox_spec(db, &run, ext.gamebox_id).await?;
    let image_ref = effective_image_ref_from_gamebox(&gamebox).map_err(AwdpError::from)?;
    let node_ip = get_setting(db, "NODE_IP")
        .await
        .map_err(|e| AwdpError::Internal(format!("get NODE_IP: {e}")))?;
    let existing_endpoints = load_endpoints(db, instance.id).await?;
    let runtime = DockerContainerRuntime::new(docker.clone());

    ImageRuntime::ensure_image(&runtime, &image_ref, None)
        .await
        .map_err(|e| AwdpError::Docker(format!("ensure image {image_ref}: {e}")))?;

    let flag = awdp_flag(
        jwt_secret,
        ext.run_id,
        ext.gamebox_id,
        ext.owner_user_id,
        ext.owner_team_id,
        flag_prefix,
    );
    let generation = instance.runtime_generation + 1;

    // 移除旧容器（pristine 重建：Break writable layer 必须清除）。
    // auto_remove 容器 stop 后由 daemon 异步移除——必须等待消失再同名重建，避免 409。
    remove_and_wait(&runtime, &instance.container_name, 15).await?;

    let (instance, _endpoints) = launch_container(
        db,
        &runtime,
        ext.run_id,
        &instance,
        &ext,
        &eg,
        &gamebox,
        &image_ref,
        &flag,
        &existing_endpoints,
        &node_ip,
        generation,
    )
    .await?;

    build_view(db, &instance, &ext, &eg, &gamebox).await
}

/// stop_and_remove 并等待容器从 daemon 消失（同名重建前必须，规避 auto_remove 竞态）。
async fn remove_and_wait(
    runtime: &DockerContainerRuntime,
    name: &str,
    timeout_secs: u64,
) -> AwdpResult<()> {
    runtime
        .stop_and_remove(name, fcmc::IMMEDIATE_STOP_TIMEOUT)
        .await
        .map_err(|e| AwdpError::Docker(format!("stop_and_remove: {e}")))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        match runtime.inspect_container(name).await {
            Ok(_) => {
                if std::time::Instant::now() >= deadline {
                    return Err(AwdpError::Docker(format!(
                        "container {name} 未在 {timeout_secs}s 内消失"
                    )));
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
            Err(_) => return Ok(()), // 404 = 已消失
        }
    }
}

/// 停止实例（保留逻辑实例与端点分配）。
pub async fn stop_instance(
    db: &DatabaseConnection,
    docker: &Docker,
    instance_id: Uuid,
    subject: Subject,
) -> AwdpResult<()> {
    let (instance, _ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
    assert_owner(&instance, subject)?;
    if instance.runtime_state == "running" {
        let runtime = DockerContainerRuntime::new(docker.clone());
        runtime
            .stop_and_remove(&instance.container_name, fcmc::IMMEDIATE_STOP_TIMEOUT)
            .await
            .map_err(|e| AwdpError::Docker(format!("stop instance: {e}")))?;
        instance_repo::update_runtime_state(
            db,
            instance_id,
            "stopped",
            None,
            instance.runtime_generation,
        )
        .await?;
    }
    Ok(())
}

/// 校验归属（Team 成员共享实例）。
fn assert_owner(instance: &event_instances::Model, subject: Subject) -> AwdpResult<()> {
    let owned = match subject {
        Subject {
            user_id: Some(u),
            team_id: None,
        } => instance.owner_user_id == Some(u),
        Subject {
            user_id: None,
            team_id: Some(t),
        } => instance.owner_team_id == Some(t),
        _ => false,
    };
    if !owned {
        return Err(AwdpError::Forbidden(
            "instance does not belong to you".into(),
        ));
    }
    Ok(())
}

/// 读取实例视图。
pub async fn get_instance_view(
    db: &DatabaseConnection,
    instance_id: Uuid,
    subject: Subject,
) -> AwdpResult<InstanceView> {
    let (instance, ext) = instance_repo::find_by_instance_id(db, instance_id).await?;
    assert_owner(&instance, subject)?;
    let run = run_repo::require_by_id(db, ext.run_id).await?;
    let (eg, gamebox) = resolve_run_gamebox_spec(db, &run, ext.gamebox_id).await?;
    build_view(db, &instance, &ext, &eg, &gamebox).await
}

/// 当前主体在该 run × gamebox 下的实例视图（未启动返回 None）。
pub async fn get_my_instance_view(
    db: &DatabaseConnection,
    run_id: Uuid,
    gamebox_id: Uuid,
    subject: Subject,
) -> AwdpResult<Option<InstanceView>> {
    let Some((instance, ext)) = instance_repo::find_instance_for_subject(
        db,
        run_id,
        gamebox_id,
        subject.user_id,
        subject.team_id,
    )
    .await?
    else {
        return Ok(None);
    };
    let run = run_repo::require_by_id(db, ext.run_id).await?;
    let (eg, gamebox) = resolve_run_gamebox_spec(db, &run, ext.gamebox_id).await?;
    Ok(Some(build_view(db, &instance, &ext, &eg, &gamebox).await?))
}

async fn build_view(
    db: &DatabaseConnection,
    instance: &event_instances::Model,
    ext: &awdp_instances::Model,
    _eg: &Option<awdp_event_gameboxes::Model>,
    gamebox: &gameboxes::Model,
) -> AwdpResult<InstanceView> {
    let endpoints = load_endpoints(db, instance.id).await?;
    let broken = break_repo::already_broken(
        db,
        ext.run_id,
        ext.gamebox_id,
        ext.owner_user_id,
        ext.owner_team_id,
    )
    .await?;
    Ok(InstanceView {
        instance_id: instance.id,
        gamebox_id: ext.gamebox_id,
        gamebox_name: gamebox.name.clone(),
        gamebox_category: gamebox.category.clone(),
        runtime_state: instance.runtime_state.clone(),
        runtime_generation: instance.runtime_generation,
        broken,
        endpoints: endpoints
            .into_iter()
            .map(|e| EndpointView {
                protocol: e.protocol,
                container_port: e.container_port as u16,
                public_host: e.public_host,
                public_port: e.public_port as u16,
            })
            .collect(),
    })
}

/// 公开端点（管理端 inspect 用）。
pub async fn instance_endpoints_for(
    db: &DatabaseConnection,
    instance_id: Uuid,
) -> AwdpResult<Vec<instance_endpoints::Model>> {
    load_endpoints(db, instance_id).await
}

async fn load_endpoints(
    db: &DatabaseConnection,
    instance_id: Uuid,
) -> AwdpResult<Vec<instance_endpoints::Model>> {
    use sea_orm::QueryOrder;
    instance_endpoints::Entity::find()
        .filter(instance_endpoints::Column::InstanceId.eq(instance_id))
        .order_by_asc(instance_endpoints::Column::ContainerPort)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

async fn upsert_endpoint(
    db: &DatabaseConnection,
    instance_id: Uuid,
    protocol: &str,
    container_port: u16,
    public_host: &str,
    public_port: u16,
) -> AwdpResult<()> {
    let exists = instance_endpoints::Entity::find()
        .filter(instance_endpoints::Column::InstanceId.eq(instance_id))
        .filter(instance_endpoints::Column::Protocol.eq(protocol))
        .filter(instance_endpoints::Column::ContainerPort.eq(container_port as i32))
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    if let Some(existing) = exists {
        // 保持既有 public_port（endpoint 稳定）；仅刷新 host（NODE_IP 变更场景）。
        let mut am: instance_endpoints::ActiveModel = existing.into();
        am.public_host = Set(public_host.to_string());
        am.update(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Ok(());
    }
    instance_endpoints::ActiveModel {
        id: Set(Uuid::new_v4()),
        instance_id: Set(instance_id),
        protocol: Set(protocol.to_string()),
        container_port: Set(container_port as i32),
        public_host: Set(public_host.to_string()),
        public_port: Set(public_port as i32),
        created_at: Set(Utc::now().into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

/// run 内全部实例视图。
pub async fn list_instances(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Vec<InstanceView>> {
    let rows = instance_repo::list_for_run(db, run_id).await?;
    let mut out = Vec::new();
    for (instance, ext) in rows {
        let run = run_repo::require_by_id(db, ext.run_id).await?;
        let (eg, gamebox) = resolve_run_gamebox_spec(db, &run, ext.gamebox_id).await?;
        out.push(build_view(db, &instance, &ext, &eg, &gamebox).await?);
    }
    Ok(out)
}

/// 事件下全部实例视图（管理端 inspect）。
pub async fn list_instances_for_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Vec<InstanceView>> {
    let rows = instance_repo::list_for_event(db, event_id).await?;
    let mut out = Vec::new();
    for (instance, ext) in rows {
        let run = run_repo::require_by_id(db, ext.run_id).await?;
        let (eg, gamebox) = resolve_run_gamebox_spec(db, &run, ext.gamebox_id).await?;
        out.push(build_view(db, &instance, &ext, &eg, &gamebox).await?);
    }
    Ok(out)
}
