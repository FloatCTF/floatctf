//! Event Network 分配服务（§48-56）：allocate / manual / reallocate / lock / release。
//!
//! 并发安全：全部写路径在「DB 事务 + pg_advisory_xact_lock」内执行（§18）。
//! overlap 校验覆盖 gamebox/wireguard × automatic/manual 全组合（§19）。
//! 已锁定（locked_at 非空）的 Event Network addressing 禁止任何变更（§34）。

use crate::entity::sea_orm_active_enums::{
    AwdEventStatus, AwdNetworkAllocationKind, AwdNetworkAllocationMode,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, QuerySelect,
    Statement, TransactionTrait,
};

use super::super::AwdError;
use super::super::domain::network::{
    InfraHostPolicy, Ipv4Cidr, NetworkPool, WireGuardPortRange, docker_network_name,
    wireguard_interface_name,
};
use super::super::infrastructure::network::AwdNetworkRuntime;
use super::super::repo::{event_network_repo, network_allocation_repo, network_settings_repo};
use fcmc::AwdContainerRuntime;

/// pg_advisory_xact_lock 固定 key：分配给 AWD 网络分配器（任意稳定常数）。
const ALLOCATOR_LOCK_KEY: i64 = 0x41_57_44_4e_45_54; // "AW DNET" 风格魔数

/// 收集宿主外部保留网段（Phase 10 A2）——Docker 网络子网 + 宿主路由。
///
/// 分配器**必须**考虑这些占用：不能假设「FloatCTF 库里没有 = 空闲」。
/// 纯读操作（bollard list_networks / `ip -o route show`），绝不改动任何资源；
/// 返回去重、排序的确定性快照，供分配器在事务外使用。
pub async fn collect_external_reserved_cidrs(
    containers: &dyn AwdContainerRuntime,
    network: &dyn AwdNetworkRuntime,
) -> AwdResult2<Vec<Ipv4Cidr>> {
    let mut out: Vec<Ipv4Cidr> = Vec::new();

    // 1. Docker 网络子网（含 FloatCTF 持久基础设施网络与活跃 Event 网络）。
    //    失败即 fail-closed：无法确认宿主占用时不允许分配。
    let docker_cidrs = containers
        .list_docker_network_cidrs()
        .await
        .map_err(|e| AwdError::Docker(format!("list docker network subnets: {e}")))?;
    for s in docker_cidrs {
        if let Ok(c) = Ipv4Cidr::parse(&s) {
            out.push(c);
        }
    }

    // 2. 宿主路由（非 Docker 占用：libvirt/incus/VPN/手工路由）。
    let route_cidrs = network
        .list_host_route_cidrs()
        .await
        .map_err(|e| AwdError::Network(format!("list host routes: {e}")))?;
    for r in route_cidrs {
        if let Ok(c) = Ipv4Cidr::parse(&r) {
            out.push(c);
        }
    }

    // 确定性：按字符串排序 + 去重（Ipv4Cidr 无 Ord；to_string 排序稳定）。
    out.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
    out.dedup();
    Ok(out)
}

/// 手动分配请求（§24）：GameBox/WG CIDR 必须合法且全局不重叠；
/// 允许指定 pool 外网段（§92，UI 提示 outside configured pool）。
pub struct ManualNetworkRequest {
    pub gamebox_cidr: String,
    pub wireguard_cidr: String,
    pub wireguard_listen_port: Option<i32>,
}

/// 自动分配（automatic，§23）：从平台池枚举第一个空闲 event 级子网。
/// 幂等：已存在未锁定分配时直接返回现有（不重复分配）。
///
/// `external_reserved`：宿主外部占用（Docker 网络子网 + 路由）——即使 FloatCTF
/// 库中没有记录也必须避让（Phase 10 A2）。调用方先 `collect_external_reserved_cidrs`
/// 采集确定性快照再传入；本函数在事务内只读使用，不改动外部资源。
pub async fn allocate_automatic(
    db: &DatabaseConnection,
    event_id: uuid::Uuid,
    external_reserved: &[Ipv4Cidr],
) -> AwdResult2<crate::entity::awd_event_networks::Model> {
    let awd = event_network_repo::find_awd_event(db, event_id).await?;
    assert_network_editable(&awd.status)?;

    if let Some(existing) = event_network_repo::find_by_event_id(db, event_id).await? {
        if existing.locked_at.is_some() {
            return Err(AwdError::NetworkLocked(
                "network already locked after deploy".into(),
            ));
        }
        return Ok(existing); // 幂等：已分配未锁定
    }

    // 闭包为 'static async：外部保留网段必须转为 owned（Ipv4Cidr: Clone）。
    let external_reserved = external_reserved.to_vec();

    run_allocator_tx(db, event_id, move |txn, settings| {
        Box::pin(async move {
            let gb_pool = NetworkPool::new(
                Ipv4Cidr::parse(&settings.gamebox_pool.to_string())?,
                settings.gamebox_event_prefix as u8,
                settings.gamebox_team_prefix as u8,
            )?;
            let wg_pool = NetworkPool::new(
                Ipv4Cidr::parse(&settings.wireguard_pool.to_string())?,
                settings.wireguard_event_prefix as u8,
                settings.wireguard_team_prefix as u8,
            )?;

            let active = network_allocation_repo::list_active(txn).await?;
            let used_ports = list_used_ports(txn).await?;

            let gb_cidr =
                first_free_event_subnet(&gb_pool, &active, &external_reserved, "gamebox")?;
            let wg_cidr =
                first_free_event_subnet(&wg_pool, &active, &external_reserved, "wireguard")?;
            let port = allocate_port(
                WireGuardPortRange::new(
                    settings.wireguard_port_min as u16,
                    settings.wireguard_port_max as u16,
                )?,
                &used_ports,
            )?;

            build_and_persist(
                txn,
                event_id,
                AwdNetworkAllocationMode::Automatic,
                &gb_cidr,
                &wg_cidr,
                port,
                gb_pool.team_prefix,
            )
            .await
        })
    })
    .await
}

/// 手动分配（manual，§13）：CIDR 由管理员提供，仍走同一套 overlap 校验。
/// `external_reserved`：宿主 Docker 网络 / 路由占用，同样必须避让（Phase 10 A2）。
pub async fn allocate_manual(
    db: &DatabaseConnection,
    event_id: uuid::Uuid,
    req: ManualNetworkRequest,
    external_reserved: &[Ipv4Cidr],
) -> AwdResult2<crate::entity::awd_event_networks::Model> {
    let awd = event_network_repo::find_awd_event(db, event_id).await?;
    assert_network_editable(&awd.status)?;
    if event_network_repo::find_by_event_id(db, event_id)
        .await?
        .is_some()
    {
        return Err(AwdError::Conflict(
            "event network already allocated；请用 reallocate".into(),
        ));
    }

    let gb_cidr = Ipv4Cidr::parse(&req.gamebox_cidr.to_string())?;
    let wg_cidr = Ipv4Cidr::parse(&req.wireguard_cidr.to_string())?;
    if gb_cidr.overlaps(&wg_cidr) {
        return Err(AwdError::NetworkOverlap(format!(
            "gamebox {} 与 wireguard {} 重叠",
            gb_cidr.to_string(),
            wg_cidr.to_string()
        )));
    }

    // Phase 10 A2：手动 CIDR 也不得与宿主 Docker 网络/路由重叠
    for ext in external_reserved {
        if ext.overlaps(&gb_cidr) || ext.overlaps(&wg_cidr) {
            return Err(AwdError::NetworkOverlap(format!(
                "manual CIDR 与宿主外部占用 {} 重叠（Docker 网络/路由），请另选网段",
                ext.to_string()
            )));
        }
    }

    run_allocator_tx(db, event_id, move |txn, settings| {
        Box::pin(async move {
            // 手动模式同样要全局 overlap（§19/§69），且不豁免 pool 外网段
            let active = network_allocation_repo::list_active(txn).await?;
            for a in &active {
                let other = Ipv4Cidr::parse(&a.cidr.to_string())?;
                if other.overlaps(&gb_cidr) || other.overlaps(&wg_cidr) {
                    return Err(overlap_error(&other, &a.event_id));
                }
            }

            let used_ports = list_used_ports(txn).await?;
            let team_prefix = settings.gamebox_team_prefix as u8;
            if gb_cidr.prefix_len > team_prefix {
                return Err(AwdError::Validation(format!(
                    "manual gamebox_cidr /{} 不可小于 team prefix /{}（infra 派生需要）",
                    gb_cidr.prefix_len, team_prefix
                )));
            }

            let port = match req.wireguard_listen_port {
                Some(p) => {
                    let range = WireGuardPortRange::new(
                        settings.wireguard_port_min as u16,
                        settings.wireguard_port_max as u16,
                    )?;
                    if !range.contains(p as u16) {
                        return Err(AwdError::Validation(format!(
                            "wg port {} 不在平台端口池 {}-{} 内",
                            p, range.min, range.max
                        )));
                    }
                    if used_ports.contains(&p) {
                        return Err(AwdError::Conflict(format!(
                            "wg port {p} 已被其他 Event 占用"
                        )));
                    }
                    p
                }
                None => allocate_port(
                    WireGuardPortRange::new(
                        settings.wireguard_port_min as u16,
                        settings.wireguard_port_max as u16,
                    )?,
                    &used_ports,
                )?,
            };

            build_and_persist(
                txn,
                event_id,
                AwdNetworkAllocationMode::Manual,
                &gb_cidr,
                &wg_cidr,
                port,
                team_prefix,
            )
            .await
        })
    })
    .await
}

/// 重新分配（§33/§93）：事务内 reserve new → 更新 Event Network → release old。
/// 任何一步失败整体回滚，旧分配保持 active。
/// `external_reserved`：宿主 Docker 网络 / 路由占用，同样必须避让（Phase 10 A2）。
pub async fn reallocate(
    db: &DatabaseConnection,
    event_id: uuid::Uuid,
    external_reserved: &[Ipv4Cidr],
) -> AwdResult2<crate::entity::awd_event_networks::Model> {
    let awd = event_network_repo::find_awd_event(db, event_id).await?;
    assert_network_editable(&awd.status)?;
    let current = event_network_repo::require_by_event_id(db, event_id).await?;
    if current.locked_at.is_some() {
        return Err(AwdError::NetworkLocked(
            "network locked after deploy；不可 reallocate".into(),
        ));
    }

    // 闭包为 'static async：外部保留网段必须转为 owned（Ipv4Cidr: Clone）。
    let external_reserved = external_reserved.to_vec();

    run_allocator_tx(db, event_id, move |txn, settings| {
        Box::pin(async move {
            let gb_pool = NetworkPool::new(
                Ipv4Cidr::parse(&settings.gamebox_pool.to_string())?,
                settings.gamebox_event_prefix as u8,
                settings.gamebox_team_prefix as u8,
            )?;
            let wg_pool = NetworkPool::new(
                Ipv4Cidr::parse(&settings.wireguard_pool.to_string())?,
                settings.wireguard_event_prefix as u8,
                settings.wireguard_team_prefix as u8,
            )?;

            let active = network_allocation_repo::list_active(txn).await?;
            let used_ports = list_used_ports(txn).await?;

            // 新候选必须跳过全部 active allocations（含本 Event 的旧 CIDR）与宿主外部占用
            let gb_cidr =
                first_free_event_subnet(&gb_pool, &active, &external_reserved, "gamebox")?;
            let wg_cidr =
                first_free_event_subnet(&wg_pool, &active, &external_reserved, "wireguard")?;
            let port = allocate_port(
                WireGuardPortRange::new(
                    settings.wireguard_port_min as u16,
                    settings.wireguard_port_max as u16,
                )?,
                &used_ports,
            )?;

            let team_prefix = gb_pool.team_prefix;
            let infra = gb_cidr
                .nth_subnet(team_prefix, 0)
                .ok_or_else(|| AwdError::Validation("infra block 超出容量".into()))?;
            let flag_ip = InfraHostPolicy::service_ip(&infra, InfraHostPolicy::FLAGSERVER_OFFSET)
                .ok_or_else(|| AwdError::Internal("flagserver ip 派生失败".into()))?;
            let judge_ip = InfraHostPolicy::service_ip(&infra, InfraHostPolicy::JUDGESERVER_OFFSET)
                .ok_or_else(|| AwdError::Internal("judgeserver ip 派生失败".into()))?;

            // 先更新 Event Network（新值）
            let patch = event_network_repo::EventNetworkPatch {
                allocation_mode: Some(AwdNetworkAllocationMode::Automatic),
                gamebox_cidr: Some(gb_cidr.to_string()),
                wireguard_cidr: Some(wg_cidr.to_string()),
                infrastructure_subnet: Some(infra.to_string()),
                flagserver_ip: Some(flag_ip.to_string()),
                judgeserver_ip: Some(judge_ip.to_string()),
                wireguard_listen_port: Some(port),
                ..Default::default()
            };
            let updated = event_network_repo::update_in_tx(txn, &current, patch).await?;

            // 再交换账本：旧释放、新保留（同事务原子，失败整体回滚 → 旧仍 active）
            network_allocation_repo::release_all_in_tx(txn, event_id).await?;
            network_allocation_repo::create_in_tx(
                txn,
                event_id,
                AwdNetworkAllocationKind::Gamebox,
                &gb_cidr.to_string(),
            )
            .await?;
            network_allocation_repo::create_in_tx(
                txn,
                event_id,
                AwdNetworkAllocationKind::Wireguard,
                &wg_cidr.to_string(),
            )
            .await?;

            Ok(updated)
        })
    })
    .await
}

/// Deploy 时锁定（§34/§51）：locked_at = now()。锁定后 addressing 不可变。
pub async fn lock(db: &DatabaseConnection, event_id: uuid::Uuid) -> AwdResult2<()> {
    let net = event_network_repo::require_by_event_id(db, event_id).await?;
    if net.locked_at.is_none() {
        let patch = event_network_repo::EventNetworkPatch {
            locked_at: Some(chrono::Utc::now().into()),
            ..Default::default()
        };
        event_network_repo::update_in_tx(db, &net, patch).await?;
    }
    Ok(())
}

/// 部署失败后释放锁定（Phase 10 A3）：DeployFailed 时 locked_at 必须清除，
/// 否则唯一恢复路径是手工改库。成功部署保留锁；本函数只由 deploy_service
/// 在真正记录 DeployFailed 后调用（并发安全：仅当仍处于 Deploying 时）。
pub async fn unlock(db: &DatabaseConnection, event_id: uuid::Uuid) -> AwdResult2<()> {
    let net = event_network_repo::require_by_event_id(db, event_id).await?;
    if net.locked_at.is_some() {
        let patch = event_network_repo::EventNetworkPatch {
            unlock: Some(true),
            ..Default::default()
        };
        event_network_repo::update_in_tx(db, &net, patch).await?;
    }
    Ok(())
}

/// Archive runtime cleanup 成功后才释放（§56/§89）。Event Network 行保留（历史）。
pub async fn release_allocations(db: &DatabaseConnection, event_id: uuid::Uuid) -> AwdResult2<()> {
    let txn = db
        .begin()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    network_allocation_repo::release_all_in_tx(&txn, event_id).await?;
    txn.commit()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

/// 事件网络是否已锁定（供 UI/Precheck 使用）。
pub async fn is_locked(db: &DatabaseConnection, event_id: uuid::Uuid) -> AwdResult2<bool> {
    Ok(event_network_repo::find_by_event_id(db, event_id)
        .await?
        .map(|n| n.locked_at.is_some())
        .unwrap_or(false))
}

type AwdResult2<T> = Result<T, AwdError>;

/// 事务 + advisory lock 的统一执行框架。
async fn run_allocator_tx<F, T>(
    db: &DatabaseConnection,
    _event_id: uuid::Uuid,
    f: F,
) -> AwdResult2<T>
where
    F: for<'a> FnOnce(
        &'a sea_orm::DatabaseTransaction,
        &'a crate::entity::awd_network_settings::Model,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = AwdResult2<T>> + Send + 'a>,
    >,
{
    let txn = db
        .begin()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // §18：allocator 级 advisory lock（事务结束自动释放）
    let lock_sql = format!("SELECT pg_advisory_xact_lock({})", ALLOCATOR_LOCK_KEY);
    txn.execute(Statement::from_string(DbBackend::Postgres, lock_sql))
        .await
        .map_err(|e| AwdError::Database(format!("allocator lock: {e}")))?;

    let settings = match network_settings_repo::get(&txn).await {
        Ok(s) => s,
        Err(e) => {
            let _ = txn.rollback().await;
            return Err(e);
        }
    };

    let result = f(&txn, &settings).await;
    match result {
        Ok(v) => {
            txn.commit()
                .await
                .map_err(|e| AwdError::Database(format!("commit: {e}")))?;
            Ok(v)
        }
        Err(e) => {
            let _ = txn.rollback().await;
            Err(e)
        }
    }
}

/// 派生全部值并持久化 Event Network + 账本（automatic/manual 共用）。
async fn build_and_persist(
    txn: &sea_orm::DatabaseTransaction,
    event_id: uuid::Uuid,
    mode: AwdNetworkAllocationMode,
    gb_cidr: &Ipv4Cidr,
    wg_cidr: &Ipv4Cidr,
    port: i32,
    team_prefix: u8,
) -> AwdResult2<crate::entity::awd_event_networks::Model> {
    let infra = gb_cidr
        .nth_subnet(team_prefix, 0)
        .ok_or_else(|| AwdError::Validation("infra block 超出容量".into()))?;
    let flag_ip = InfraHostPolicy::service_ip(&infra, InfraHostPolicy::FLAGSERVER_OFFSET)
        .ok_or_else(|| AwdError::Internal("flagserver ip 派生失败".into()))?;
    let judge_ip = InfraHostPolicy::service_ip(&infra, InfraHostPolicy::JUDGESERVER_OFFSET)
        .ok_or_else(|| AwdError::Internal("judgeserver ip 派生失败".into()))?;

    let model = event_network_repo::create_in_tx(
        txn,
        event_network_repo::NewEventNetwork {
            event_id,
            allocation_mode: mode,
            gamebox_cidr: gb_cidr.to_string(),
            wireguard_cidr: wg_cidr.to_string(),
            infrastructure_subnet: infra.to_string(),
            flagserver_ip: flag_ip.to_string(),
            judgeserver_ip: judge_ip.to_string(),
            wireguard_interface_name: wireguard_interface_name(&event_id),
            wireguard_listen_port: port,
            docker_network_name: docker_network_name(&event_id),
            locked_at: None,
        },
    )
    .await?;

    network_allocation_repo::create_in_tx(
        txn,
        event_id,
        AwdNetworkAllocationKind::Gamebox,
        &gb_cidr.to_string(),
    )
    .await?;
    network_allocation_repo::create_in_tx(
        txn,
        event_id,
        AwdNetworkAllocationKind::Wireguard,
        &wg_cidr.to_string(),
    )
    .await?;

    Ok(model)
}

/// 从平台池找第一个不与任何 active allocation 或宿主外部占用重叠的 event 级子网（§79 惰性迭代）。
fn first_free_event_subnet(
    pool: &NetworkPool,
    active: &[crate::entity::awd_network_allocations::Model],
    external_reserved: &[Ipv4Cidr],
    label: &str,
) -> AwdResult2<Ipv4Cidr> {
    let capacity = pool.event_capacity();
    for i in 0..capacity {
        let candidate = pool
            .nth_event_subnet(i)
            .ok_or_else(|| AwdError::Internal(format!("event subnet {i} 超出容量 {capacity}")))?;
        let taken_by_db = active.iter().any(|a| {
            Ipv4Cidr::parse(&a.cidr.to_string())
                .map(|c| c.overlaps(&candidate))
                .unwrap_or(false)
        });
        // Phase 10 A2：宿主 Docker 网络/路由同样视为占用（即使库里没有记录）
        let taken_external = external_reserved.iter().any(|ext| ext.overlaps(&candidate));
        if !taken_by_db && !taken_external {
            return Ok(candidate);
        }
    }
    Err(AwdError::PoolExhausted(format!(
        "AWD_{}_POOL_EXHAUSTED: {label} 地址池 {} 已无空闲 event 子网（容量 {}）",
        label.to_uppercase(),
        pool.pool.to_string(),
        capacity
    )))
}

/// 找第一个未被占用的 WG 端口（§29，惰性枚举）。
fn allocate_port(range: WireGuardPortRange, used: &[i32]) -> AwdResult2<i32> {
    for p in range.min..=range.max {
        if !used.contains(&(p as i32)) {
            return Ok(p as i32);
        }
    }
    Err(AwdError::PoolExhausted(format!(
        "AWD_WG_PORT_POOL_EXHAUSTED: WG 端口池 {}-{} 已无空闲端口",
        range.min, range.max
    )))
}

/// 所有已占用 WG 端口（全部 Event Network 行，含已释放事件——端口不可复用需 host 校验）。
async fn list_used_ports<C: ConnectionTrait + Send>(db: &C) -> Result<Vec<i32>, AwdError> {
    use crate::entity::awd_event_networks;
    // 全列 select：sea-orm 1.1.20 按完整 Model 解码，select_only 会缺 id 列（ColumnNotFound）
    let rows = awd_event_networks::Entity::find()
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(rows.iter().map(|r| r.wireguard_listen_port).collect())
}

fn overlap_error(other: &Ipv4Cidr, other_event: &uuid::Uuid) -> AwdError {
    AwdError::NetworkOverlap(format!(
        "AWD_NETWORK_OVERLAP: 与 Event {} 的 {} 冲突",
        other_event,
        other.to_string()
    ))
}

/// §34：仅 Draft/Configuring 可编辑网络。
fn assert_network_editable(status: &AwdEventStatus) -> AwdResult2<()> {
    match status {
        AwdEventStatus::Draft | AwdEventStatus::Configuring => Ok(()),
        other => Err(AwdError::NetworkLocked(format!(
            "network addressing locked in status {:?}（仅 Draft/Configuring 可改）",
            other
        ))),
    }
}
