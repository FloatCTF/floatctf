//! AWD Network Control Plane DB-gated 集成测试（§90-§100）。
//!
//! 覆盖平台网络设置校验（§90/§10/§11）、自动分配幂等与唯一性（§91）、
//! 并发分配唯一性（§18/§91）、手动分配校验（§92）、reallocate 失败保持旧分配（§93）、
//! Deploy 锁定（§34/§94）、Team 子网稳定唯一（§95）、Archive 释放（§56/§89/§100）。
//!
//! 需要可达的 PostgreSQL（soft-skip，不可达时 eprintln + return）；
//! 测试间通过随机 UUID/tag 隔离，结束后删除自建的 events 行（级联清理
//! awd_events / awd_event_networks / awd_network_allocations / awd_team_networks / event_teams）。
//!
//! 平台地址池是 singleton（awd_network_settings id=1），本文件内全部测试用全局
//! Mutex 串行化，避免设置变更与分配互相干扰。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use std::sync::Arc;
use uuid::Uuid;

use floatctf::entity::{
    awd_events, awd_network_settings, event_teams, events, sea_orm_active_enums,
    sea_orm_active_enums::AwdEventStatus,
};
use floatctf::modules::event::awd_team::{
    AwdError,
    crypto::AwdCrypto,
    domain::network::Ipv4Cidr,
    repo::{event_network_repo, network_allocation_repo, network_settings_repo},
    service::{
        event_network_service::{self, ManualNetworkRequest},
        platform_network_service, team_network_allocator,
    },
};

/// 平台池共享锁（singleton settings + allocator 全局账本），串行化本文件测试。
static POOL_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

async fn pool_lock() -> tokio::sync::MutexGuard<'static, ()> {
    POOL_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip awd_network_ipam: DB unreachable ({e})");
            None
        }
    }
}

/// 插入 events 父行 + awd_events 行（status = Configuring，网络可编辑）。
async fn seed_event(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
    let event_id = Uuid::new_v4();
    let parent = events::ActiveModel {
        id: Set(event_id),
        title: Set(format!("awd-network-ipam-{tag}")),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set((chrono::Utc::now() + chrono::Duration::hours(1)).into()),
        ..Default::default()
    };
    parent.insert(db).await.expect("insert events");

    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        status: Set(AwdEventStatus::Configuring),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        key_version: Set(1),
        round_duration_secs: Set(300),
        ..Default::default()
    };
    awd.insert(db).await.expect("insert awd_events");
    event_id
}

/// 删除自建 events 行：awd_events / awd_event_networks / awd_network_allocations /
/// 清掉全部 ipam 测试残留（事件删除级联 allocations / event_networks）。
async fn cleanup_all_ipam_events(db: &sea_orm::DatabaseConnection) {
    let _ = events::Entity::delete_many()
        .filter(events::Column::Title.like("awd-network-ipam-%"))
        .exec(db)
        .await;
}

/// awd_team_networks / event_teams 全部 ON DELETE CASCADE。
async fn cleanup_event(db: &sea_orm::DatabaseConnection, event_id: Uuid) {
    let _ = events::Entity::delete_many()
        .filter(events::Column::Id.eq(event_id))
        .exec(db)
        .await;
}

/// 插入 event_teams 行（offset_secs 控制 created_at 顺序，决定 subnet_index 分配顺序）。
async fn seed_team(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    tag: &str,
    offset_secs: i64,
) -> Uuid {
    let team_id = Uuid::new_v4();
    event_teams::ActiveModel {
        id: Set(team_id),
        event_id: Set(event_id),
        name: Set(format!("team-{tag}")),
        points: Set(0.0),
        banned: Set(false),
        created_at: Set((chrono::Utc::now() + chrono::Duration::seconds(offset_secs)).into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert event_teams");
    team_id
}

/// 把平台 singleton 设置恢复为 original 值（修改后显式调用）。
async fn restore_settings(
    db: &sea_orm::DatabaseConnection,
    original: &awd_network_settings::Model,
) {
    let patch = network_settings_repo::NetworkSettingsPatch {
        gamebox_pool: Some(original.gamebox_pool.to_string()),
        gamebox_event_prefix: Some(original.gamebox_event_prefix),
        gamebox_team_prefix: Some(original.gamebox_team_prefix),
        wireguard_pool: Some(original.wireguard_pool.to_string()),
        wireguard_event_prefix: Some(original.wireguard_event_prefix),
        wireguard_team_prefix: Some(original.wireguard_team_prefix),
        wireguard_port_min: Some(original.wireguard_port_min),
        wireguard_port_max: Some(original.wireguard_port_max),
        wireguard_public_endpoint: original.wireguard_public_endpoint.clone(),
    };
    network_settings_repo::update(db, patch)
        .await
        .expect("restore settings");
}

/// 进程级 crypto 配置（OnceLock 只初始化一次；照抄 awd_gamebox_domain.rs 模式）。
fn configure_crypto_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        AwdCrypto::configure_secret(floatctf::core::secret::Secret::new(
            "test-master-secret-12345678",
        ));
    });
}

// ────────────────────────────────────────────────────────────────────────────
// §90/§10/§11：平台网络设置校验
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn platform_settings_reject_invalid_pools() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let _guard = pool_lock().await;
    cleanup_all_ipam_events(&db).await;
    let original = network_settings_repo::get(&db)
        .await
        .expect("settings singleton");

    // §10：gamebox 池与 wireguard 池重叠 → Validation
    let err = platform_network_service::update_settings(
        &db,
        network_settings_repo::NetworkSettingsPatch {
            gamebox_pool: Some("10.0.0.0/8".into()),
            wireguard_pool: Some("10.128.0.0/9".into()),
            ..Default::default()
        },
    )
    .await
    .expect_err("overlapping pools must be rejected");
    assert!(
        matches!(err, AwdError::Validation(_)),
        "expected Validation, got {err:?}"
    );

    // §10：前缀顺序错误（team prefix < event prefix）→ Validation
    let err = platform_network_service::update_settings(
        &db,
        network_settings_repo::NetworkSettingsPatch {
            gamebox_pool: Some("10.0.0.0/8".into()),
            gamebox_event_prefix: Some(24),
            gamebox_team_prefix: Some(16),
            ..Default::default()
        },
    )
    .await
    .expect_err("team prefix shorter than event prefix must be rejected");
    assert!(
        matches!(err, AwdError::Validation(_)),
        "expected Validation, got {err:?}"
    );

    // §29：WG 端口范围非法（min > max）→ Validation
    let err = platform_network_service::update_settings(
        &db,
        network_settings_repo::NetworkSettingsPatch {
            wireguard_port_min: Some(50000),
            wireguard_port_max: Some(40000),
            ..Default::default()
        },
    )
    .await
    .expect_err("invalid port range must be rejected");
    assert!(
        matches!(err, AwdError::Validation(_)),
        "expected Validation, got {err:?}"
    );

    // §11：合法更新（pool 互不重叠 + 前缀顺序正确 + 端口范围合法）→ Ok
    let updated = platform_network_service::update_settings(
        &db,
        network_settings_repo::NetworkSettingsPatch {
            gamebox_pool: Some("10.0.0.0/8".into()),
            gamebox_event_prefix: Some(16),
            gamebox_team_prefix: Some(24),
            wireguard_pool: Some("172.16.0.0/12".into()),
            wireguard_event_prefix: Some(16),
            wireguard_team_prefix: Some(24),
            wireguard_port_min: Some(30001),
            wireguard_port_max: Some(39999),
            ..Default::default()
        },
    )
    .await
    .expect("legal settings update must succeed");
    assert_eq!(updated.wireguard_port_min, 30001);
    assert_eq!(updated.wireguard_port_max, 39999);

    restore_settings(&db, &original).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §91：自动分配 —— 每 Event 唯一 + 幂等
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn automatic_allocation_unique_per_event() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let _guard = pool_lock().await;
    cleanup_all_ipam_events(&db).await;
    let e1 = seed_event(&db, "uniq-a").await;
    let e2 = seed_event(&db, "uniq-b").await;

    let n1 = event_network_service::allocate_automatic(&db, e1)
        .await
        .expect("allocate e1");
    let n2 = event_network_service::allocate_automatic(&db, e2)
        .await
        .expect("allocate e2");

    assert_ne!(
        n1.gamebox_cidr.to_string(),
        n2.gamebox_cidr.to_string(),
        "gamebox cidr 必须互不相同"
    );
    assert_ne!(
        n1.wireguard_cidr.to_string(),
        n2.wireguard_cidr.to_string(),
        "wireguard cidr 必须互不相同"
    );
    assert_ne!(
        n1.wireguard_listen_port, n2.wireguard_listen_port,
        "wg 端口必须互不相同"
    );

    // 幂等：重复 allocate 返回同一行（不重复分配）
    let n1_again = event_network_service::allocate_automatic(&db, e1)
        .await
        .expect("idempotent allocate");
    assert_eq!(n1_again.id, n1.id, "重复分配必须幂等返回同一行");

    cleanup_event(&db, e1).await;
    cleanup_event(&db, e2).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §18/§91：并发自动分配 —— advisory lock 保证唯一
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn automatic_allocation_concurrent_no_duplicate() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let _guard = pool_lock().await;
    cleanup_all_ipam_events(&db).await;

    // 默认 wireguard 池 172.16.0.0/12 + /16 只有 16 个 slot；临时放大到 /24（4096）容纳 20 并发
    let original = network_settings_repo::get(&db).await.expect("get settings");
    let patch = network_settings_repo::NetworkSettingsPatch {
        wireguard_event_prefix: Some(24),
        ..Default::default()
    };
    network_settings_repo::update(&db, patch)
        .await
        .expect("enlarge wireguard pool");

    const N: usize = 20;
    let db = Arc::new(db);
    let mut handles = Vec::with_capacity(N);
    for i in 0..N {
        let db = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            let event_id = seed_event(&db, &format!("conc-{i}")).await;
            let res = event_network_service::allocate_automatic(&db, event_id).await;
            (event_id, res)
        }));
    }

    let mut nets = Vec::with_capacity(N);
    for h in handles {
        let (event_id, res) = h.await.expect("join handle");
        let net = res.expect("concurrent allocation must succeed");
        nets.push((event_id, net));
    }

    let mut gb: Vec<String> = nets
        .iter()
        .map(|(_, n)| n.gamebox_cidr.to_string())
        .collect();
    gb.sort();
    gb.dedup();
    assert_eq!(gb.len(), N, "并发分配 gamebox cidr 必须全部唯一");

    let mut ports: Vec<i32> = nets.iter().map(|(_, n)| n.wireguard_listen_port).collect();
    ports.sort();
    ports.dedup();
    assert_eq!(ports.len(), N, "并发分配 wg 端口必须全部唯一");

    for (event_id, _) in &nets {
        cleanup_event(&db, *event_id).await;
    }
    restore_settings(&db, &original).await;
    cleanup_all_ipam_events(&db).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §13/§19/§92：手动分配校验
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn manual_allocation_validation() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let _guard = pool_lock().await;
    cleanup_all_ipam_events(&db).await;
    let e1 = seed_event(&db, "manual-a").await;
    let e2 = seed_event(&db, "manual-b").await;
    let e3 = seed_event(&db, "manual-c").await;

    // §92：允许指定 pool 外网段
    let n1 = event_network_service::allocate_manual(
        &db,
        e1,
        ManualNetworkRequest {
            gamebox_cidr: "192.168.100.0/24".into(),
            wireguard_cidr: "192.168.200.0/24".into(),
            wireguard_listen_port: None,
        },
    )
    .await
    .expect("manual outside pool must be allowed");
    assert_eq!(n1.gamebox_cidr.to_string(), "192.168.100.0/24");

    // §19：manual 与已有分配重叠 → NetworkOverlap
    let err = event_network_service::allocate_manual(
        &db,
        e2,
        ManualNetworkRequest {
            gamebox_cidr: "192.168.100.0/24".into(),
            wireguard_cidr: "192.168.201.0/24".into(),
            wireguard_listen_port: None,
        },
    )
    .await
    .expect_err("overlap with existing allocation must be rejected");
    assert!(
        matches!(err, AwdError::NetworkOverlap(_)),
        "expected NetworkOverlap, got {err:?}"
    );

    // §19：manual 自身 gamebox/wireguard 重叠 → NetworkOverlap
    let err = event_network_service::allocate_manual(
        &db,
        e3,
        ManualNetworkRequest {
            gamebox_cidr: "10.99.0.0/16".into(),
            wireguard_cidr: "10.99.0.0/24".into(),
            wireguard_listen_port: None,
        },
    )
    .await
    .expect_err("self-overlapping cidrs must be rejected");
    assert!(
        matches!(err, AwdError::NetworkOverlap(_)),
        "expected NetworkOverlap, got {err:?}"
    );

    cleanup_event(&db, e1).await;
    cleanup_event(&db, e2).await;
    cleanup_event(&db, e3).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §33/§93：reallocate 失败时旧分配保持 active
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reallocate_keeps_old_on_failure() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let _guard = pool_lock().await;
    cleanup_all_ipam_events(&db).await;
    let original = network_settings_repo::get(&db)
        .await
        .expect("settings singleton");
    let orig_gb_pool = original.gamebox_pool.to_string();
    let orig_gb_event = original.gamebox_event_prefix;
    let orig_gb_team = original.gamebox_team_prefix;

    let event_id = seed_event(&db, "rea").await;
    let net = event_network_service::allocate_automatic(&db, event_id)
        .await
        .expect("allocate");
    let old_gb = net.gamebox_cidr.to_string();

    // §93：把 gamebox 池缩为「仅含当前分配」→ reallocate 唯一候选被占 → PoolExhausted
    platform_network_service::update_settings(
        &db,
        network_settings_repo::NetworkSettingsPatch {
            gamebox_pool: Some(old_gb.clone()),
            gamebox_event_prefix: Some(orig_gb_event),
            gamebox_team_prefix: Some(orig_gb_team),
            ..Default::default()
        },
    )
    .await
    .expect("narrow pool update");

    let err = event_network_service::reallocate(&db, event_id)
        .await
        .expect_err("reallocate must fail when pool cannot fit");
    assert!(
        matches!(err, AwdError::PoolExhausted(_)),
        "expected PoolExhausted, got {err:?}"
    );

    // 原分配保持 active 且 CIDR 未变
    let after = event_network_repo::find_by_event_id(&db, event_id)
        .await
        .expect("find")
        .expect("network row");
    assert_eq!(
        after.gamebox_cidr.to_string(),
        old_gb,
        "旧 gamebox CIDR 必须未变"
    );
    let active = network_allocation_repo::list_active(&db)
        .await
        .expect("list_active");
    assert!(
        active.iter().any(|a| {
            a.event_id == event_id
                && a.kind == sea_orm_active_enums::AwdNetworkAllocationKind::Gamebox
        }),
        "旧 gamebox 分配必须仍 active"
    );

    cleanup_event(&db, event_id).await;
    restore_settings(&db, &original).await;
    let restored = network_settings_repo::get(&db)
        .await
        .expect("settings restored");
    assert_eq!(restored.gamebox_pool.to_string(), orig_gb_pool);
}

// ────────────────────────────────────────────────────────────────────────────
// §34/§94：Deploy 锁定后拒绝任何变更
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn network_locked_after_deploy() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let _guard = pool_lock().await;
    cleanup_all_ipam_events(&db).await;
    let event_id = seed_event(&db, "lock").await;
    let net = event_network_service::allocate_automatic(&db, event_id)
        .await
        .expect("allocate");
    assert!(
        !event_network_service::is_locked(&db, event_id)
            .await
            .expect("is_locked"),
        "分配后未锁定"
    );

    // 模拟 Deploy 锁定（§34/§51）：locked_at = now()
    let patch = event_network_repo::EventNetworkPatch {
        locked_at: Some(chrono::Utc::now().into()),
        ..Default::default()
    };
    event_network_repo::update_in_tx(&db, &net, patch)
        .await
        .expect("lock network");
    assert!(
        event_network_service::is_locked(&db, event_id)
            .await
            .expect("is_locked"),
        "locked_at 置位后必须判定为锁定"
    );

    // §94：锁定后 reallocate 拒绝
    let err = event_network_service::reallocate(&db, event_id)
        .await
        .expect_err("reallocate on locked network must fail");
    assert!(
        matches!(err, AwdError::NetworkLocked(_)),
        "expected NetworkLocked, got {err:?}"
    );

    // 锁定后 manual 同样拒绝（已有分配 → Conflict）
    let err = event_network_service::allocate_manual(
        &db,
        event_id,
        ManualNetworkRequest {
            gamebox_cidr: "192.168.150.0/24".into(),
            wireguard_cidr: "192.168.151.0/24".into(),
            wireguard_listen_port: None,
        },
    )
    .await
    .expect_err("manual on allocated event must fail");
    assert!(
        matches!(err, AwdError::Conflict(_) | AwdError::NetworkLocked(_)),
        "expected Conflict/NetworkLocked, got {err:?}"
    );

    cleanup_event(&db, event_id).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §36-39/§95：Team 子网稳定且唯一
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn team_subnet_stable_and_unique() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let _guard = pool_lock().await;
    cleanup_all_ipam_events(&db).await;
    configure_crypto_once();

    let event_id = seed_event(&db, "teamnet").await;
    let net = event_network_service::allocate_automatic(&db, event_id)
        .await
        .expect("allocate");
    let gb_parent = Ipv4Cidr::parse(&net.gamebox_cidr.to_string()).expect("gb parse");
    let wg_parent = Ipv4Cidr::parse(&net.wireguard_cidr.to_string()).expect("wg parse");
    let team_prefix = Ipv4Cidr::parse(&net.infrastructure_subnet.to_string())
        .expect("infra parse")
        .prefix_len;

    let t1 = seed_team(&db, event_id, "t1", 0).await;
    let t2 = seed_team(&db, event_id, "t2", 1).await;

    let crypto = AwdCrypto::from_config_secret().expect("crypto configured");
    let rows = team_network_allocator::ensure_team_networks(&db, event_id, &crypto, 1)
        .await
        .expect("ensure team networks");
    assert_eq!(rows.len(), 2, "两个 team 各一条 team network");

    let r1 = rows.iter().find(|r| r.team_id == t1).expect("team1 row");
    let r2 = rows.iter().find(|r| r.team_id == t2).expect("team2 row");
    assert_ne!(r1.subnet_index, r2.subnet_index, "subnet_index 必须不同");
    let mut idxs = [r1.subnet_index, r2.subnet_index];
    idxs.sort();
    assert_eq!(idxs, [1, 2], "index 0 为 infra 保留，team 从 1 开始");

    // gamebox/wireguard 子网 = event cidr 的第 index 个 team-size 子网
    for r in [r1, r2] {
        let expected_gb = gb_parent
            .nth_subnet(team_prefix, r.subnet_index as u64)
            .expect("gb subnet")
            .to_string();
        let expected_wg = wg_parent
            .nth_subnet(team_prefix, r.subnet_index as u64)
            .expect("wg subnet")
            .to_string();
        assert_eq!(r.gamebox_subnet.to_string(), expected_gb);
        assert_eq!(r.wireguard_subnet.to_string(), expected_wg);
    }

    // §38：幂等——再次 ensure 行数不变、子网不变
    let rows2 = team_network_allocator::ensure_team_networks(&db, event_id, &crypto, 1)
        .await
        .expect("ensure again");
    assert_eq!(rows2.len(), 2, "幂等：行数不变");
    for r2_ in &rows2 {
        let r1_ = rows
            .iter()
            .find(|x| x.team_id == r2_.team_id)
            .expect("orig row");
        assert_eq!(r2_.gamebox_subnet, r1_.gamebox_subnet, "幂等：子网不变");
        assert_eq!(r2_.subnet_index, r1_.subnet_index, "幂等：index 不变");
    }

    // §95：重命名 team 不影响已有子网
    let mut am: event_teams::ActiveModel = event_teams::Entity::find_by_id(t1)
        .one(&db)
        .await
        .expect("find t1")
        .expect("t1 row")
        .into();
    am.name = Set("renamed-team".into());
    am.updated_at = Set(chrono::Utc::now().into());
    am.update(&db).await.expect("rename team");

    let rows3 = team_network_allocator::ensure_team_networks(&db, event_id, &crypto, 1)
        .await
        .expect("ensure after rename");
    assert_eq!(rows3.len(), 2, "重命名后行数不变");
    let renamed = rows3.iter().find(|r| r.team_id == t1).expect("t1 row");
    assert_eq!(
        renamed.gamebox_subnet.to_string(),
        r1.gamebox_subnet.to_string(),
        "重命名不改变已有子网"
    );
    assert_eq!(renamed.subnet_index, r1.subnet_index, "重命名不改变 index");

    cleanup_event(&db, event_id).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §56/§89/§100：Archive 释放 —— 释放后不再 active，released_at 落账
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn archive_release_only_after_cleanup() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let _guard = pool_lock().await;
    cleanup_all_ipam_events(&db).await;
    let event_id = seed_event(&db, "release").await;
    event_network_service::allocate_automatic(&db, event_id)
        .await
        .expect("allocate");

    let active_before = network_allocation_repo::list_active(&db)
        .await
        .expect("list_active");
    assert!(
        active_before.iter().any(|a| a.event_id == event_id),
        "分配必须出现在 active 账本"
    );

    // §56/§89：Archive runtime cleanup 成功后才释放
    event_network_service::release_allocations(&db, event_id)
        .await
        .expect("release");

    let active_after = network_allocation_repo::list_active(&db)
        .await
        .expect("list_active");
    assert!(
        !active_after.iter().any(|a| a.event_id == event_id),
        "释放后不再 active"
    );

    let for_event = network_allocation_repo::list_for_event(&db, event_id)
        .await
        .expect("list_for_event");
    assert_eq!(for_event.len(), 2, "gamebox + wireguard 两条账本记录");
    assert!(
        for_event.iter().all(|a| a.released_at.is_some()),
        "释放后 released_at 必须非空"
    );

    cleanup_event(&db, event_id).await;
}
