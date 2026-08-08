//! GameBox 领域重构回归测试（§65-71）：DB-gated，begin-tx + rollback 风格。
//!
//! 覆盖：
//!   §65  Revision digest 去重（same canonical spec → 不建重复 revision；变化 → N+1）
//!   §70  赛事 pin 隔离（Event A pin rev1，全局发布 rev2，A 仍解析 rev1）
//!   §71  计分按 Event 独立（同 GameBox 在不同 Event 有不同 break_points）
//!   §67  Reset 保持 logical identity（id/event_gamebox_id/team_id/IP/credential 不变，
//!        current_container 更换、runtime_generation +1）
//!   §68  Reset 使用 pinned Revision（全局 latest=rev2 时 Reset 仍按 rev1 镜像重建）

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    awd_event_gameboxes, awd_event_networks, awd_events, awd_gamebox_instances, awd_rounds,
    awd_team_networks, event_teams, events, gamebox_revisions, gameboxes, sea_orm_active_enums,
    sea_orm_active_enums::{AwdEventStatus, AwdPhase, GameboxStatus, RoundStatus},
};

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip awd_gamebox_domain: DB unreachable ({e})");
            None
        }
    }
}

/// 创建 events + awd_events（Running）双表 seed。
async fn seed_running_event(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
    let event_id = Uuid::new_v4();
    events::ActiveModel {
        id: Set(event_id),
        r#type: Set(floatctf::entity::sea_orm_active_enums::EventType::AwdTeam),
        title: Set(format!("awd-gb-domain-{tag}")),
        hidden: Set(true),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set((chrono::Utc::now() + chrono::Duration::hours(1)).into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert events");

    awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        status: Set(AwdEventStatus::Running),
        phase: Set(AwdPhase::Attack),
        event_secret_ciphertext: Set(vec![0u8; 32]),
        event_secret_nonce: Set(vec![0u8; 24]),
        key_version: Set(1),
        free_reset_count: Set(3),
        extra_reset_penalty: Set(100),
        reset_protection_secs: Set(0),
        judge_max_concurrency: Set(10),
        judge_default_timeout_secs: Set(30),
        judge_retry_interval_secs: Set(5),
        judge_grace_period_secs: Set(30),
        round_duration_secs: Set(300),
        archive_retention_hours: Set(168),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert awd_events");

    // Event Network（新模型）
    awd_event_networks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        allocation_mode: Set(sea_orm_active_enums::AwdNetworkAllocationMode::Automatic),
        gamebox_cidr: Set("10.42.0.0/16".parse().unwrap()),
        wireguard_cidr: Set("172.31.0.0/16".parse().unwrap()),
        infrastructure_subnet: Set("10.42.0.0/24".parse().unwrap()),
        flagserver_ip: Set("10.42.0.2".parse().unwrap()),
        judgeserver_ip: Set("10.42.0.3".parse().unwrap()),
        wireguard_interface_name: Set(format!("fawg_{}", &event_id.simple().to_string()[..8])),
        wireguard_listen_port: Set(52000 + (event_id.as_bytes()[0] as i32) % 1000),
        docker_network_name: Set(format!("fctf-awd-{}", &event_id.to_string()[..8])),
        locked_at: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert awd_event_networks");

    event_id
}

async fn seed_team(db: &sea_orm::DatabaseConnection, event_id: Uuid, tag: &str) -> Uuid {
    let team_id = Uuid::new_v4();
    event_teams::ActiveModel {
        id: Set(team_id),
        event_id: Set(event_id),
        name: Set(format!("team-{tag}")),
        points: Set(0.0),
        banned: Set(false),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert team");
    team_id
}

async fn seed_team_network(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    subnet: &str,
) -> Uuid {
    use floatctf::modules::event::awd_team::crypto::AwdCrypto;
    let crypto = AwdCrypto::from_config_secret().expect("crypto configured");
    let aad = AwdCrypto::build_aad(event_id, "ssh_password");
    let blob = crypto
        .encrypt(b"test-pass-123456".as_slice(), &aad, 1)
        .expect("encrypt");
    let net_id = Uuid::new_v4();
    awd_team_networks::ActiveModel {
        id: Set(net_id),
        event_id: Set(event_id),
        team_id: Set(team_id),
        gamebox_subnet: Set(subnet.parse().unwrap()),
        wireguard_subnet: Set("172.31.1.0/24".parse().unwrap()),
        ssh_password_ciphertext: Set(blob.ciphertext),
        ssh_password_nonce: Set(blob.nonce),
        key_version: Set(1),
        subnet_index: Set(1),
        next_wireguard_host: Set(2),
        status: Set("active".into()),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert team network");
    net_id
}

/// GameBox identity + Revision 1（digest 可指定）。
async fn seed_gamebox_with_revision(
    db: &sea_orm::DatabaseConnection,
    tag: &str,
    digest: &str,
    image_ref: &str,
) -> (Uuid, Uuid) {
    let now = chrono::Utc::now();
    let gb_id = Uuid::new_v4();
    gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(format!("gb-{tag}")),
        safe_name: Set(format!("gb-{tag}-{}", &gb_id.to_string()[..8])),
        category: Set("other".into()),
        description: Set(String::new()),
        hidden: Set(false),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert gamebox");

    let rev_id = Uuid::new_v4();
    gamebox_revisions::ActiveModel {
        id: Set(rev_id),
        gamebox_id: Set(gb_id),
        revision_number: Set(1),
        source_toml: Set(String::new()),
        spec_schema_version: Set(1),
        spec_json: Set(serde_json::json!({})),
        spec_digest: Set(digest.into()),
        image_ref: Set(image_ref.into()),
        image_digest: Set(None),
        username: Set("ctf".into()),
        default_cpu_millis: Set(1000),
        default_memory_bytes: Set(512 * 1024 * 1024),
        default_pids_limit: Set(100),
        healthcheck_json: Set(None),
        judge_script_name: Set(None),
        judge_script_content: Set(Some("#!/bin/sh\nexit 0".into())),
        judge_args_json: Set(None),
        default_judge_timeout_secs: Set(None),
        default_judge_retry_interval_secs: Set(None),
        created_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert revision");
    (gb_id, rev_id)
}

async fn seed_event_gamebox(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    gamebox_id: Uuid,
    revision_id: Uuid,
    host_offset: i16,
    break_points: i64,
) -> Uuid {
    let now = chrono::Utc::now();
    let eg_id = Uuid::new_v4();
    awd_event_gameboxes::ActiveModel {
        id: Set(eg_id),
        event_id: Set(event_id),
        gamebox_id: Set(gamebox_id),
        gamebox_revision_id: Set(revision_id),
        host_offset: Set(host_offset),
        enabled: Set(true),
        hidden: Set(false),
        cpu_millis: Set(1000),
        memory_bytes: Set(512 * 1024 * 1024),
        pids_limit: Set(100),
        healthcheck_override_json: Set(None),
        judge_timeout_secs: Set(None),
        judge_retry_interval_secs: Set(None),
        break_points: Set(break_points),
        loss_points: Set(50),
        fix_points: Set(80),
        down_points: Set(60),
        first_bonus: Set(20),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert event_gamebox");
    eg_id
}

// ────────────────────────────────────────────────────────────────────────────
// Mock AwdContainerRuntime（记录 reset 时收到的 spec）
// ────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockContainerRuntime {
    last_reset_spec: Mutex<Option<fcmc::GameBoxSpec>>,
    reset_count: Mutex<u64>,
}

#[async_trait::async_trait]
impl fcmc::AwdContainerRuntime for MockContainerRuntime {
    async fn create_event_network(
        &self,
        _spec: fcmc::EventNetworkSpec,
    ) -> anyhow::Result<fcmc::NetworkHandle> {
        unreachable!()
    }
    async fn inspect_event_network(&self, _id: &str) -> anyhow::Result<fcmc::NetworkState> {
        unreachable!()
    }
    async fn remove_event_network(&self, _id: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    async fn create_infrastructure_container(
        &self,
        _spec: fcmc::InfrastructureContainerSpec,
    ) -> anyhow::Result<fcmc::ContainerHandle> {
        unreachable!()
    }
    async fn create_gamebox(
        &self,
        _spec: fcmc::GameBoxSpec,
    ) -> anyhow::Result<fcmc::ContainerHandle> {
        unreachable!()
    }
    async fn reset_gamebox(
        &self,
        spec: fcmc::GameBoxResetSpec,
    ) -> anyhow::Result<fcmc::ContainerHandle> {
        *self.last_reset_spec.lock().unwrap() = Some(spec.recreate_spec.clone());
        *self.reset_count.lock().unwrap() += 1;
        Ok(fcmc::ContainerHandle {
            container_id: format!("cid-new-{}", spec.instance_id),
            container_name: spec.container_name,
        })
    }
    async fn stop_container(&self, _id: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    async fn remove_container(&self, _id: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    async fn inspect_container(&self, _id: &str) -> anyhow::Result<fcmc::ContainerState> {
        unreachable!()
    }
    async fn list_event_containers(
        &self,
        _event_id: Uuid,
    ) -> anyhow::Result<Vec<fcmc::ContainerState>> {
        unreachable!()
    }
    async fn container_logs(&self, _id: &str, _limit: usize) -> anyhow::Result<Vec<String>> {
        unreachable!()
    }
}

fn configure_crypto_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        floatctf::modules::event::awd_team::crypto::AwdCrypto::configure_secret(
            floatctf::core::secret::Secret::new("test-master-secret-12345678"),
        );
    });
}

// ────────────────────────────────────────────────────────────────────────────
// §65 Revision digest 去重
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn revision_digest_dedup_skips_identical_spec() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let (gb_id, _rev1) = seed_gamebox_with_revision(&db, "dedup", "digest-a", "img:v1").await;

    use floatctf::modules::event::awd_team::repo::gamebox_lib_repo;
    // 相同 digest → 不创建
    let none = gamebox_lib_repo::create_revision(
        &db,
        gb_id,
        gamebox_lib_repo::NewRevision {
            source_toml: String::new(),
            spec_json: serde_json::json!({}),
            spec_digest: "digest-a".into(),
            image_ref: "img:v1".into(),
            image_digest: None,
            username: "ctf".into(),
            default_cpu_millis: 1000,
            default_memory_bytes: 512 * 1024 * 1024,
            default_pids_limit: 100,
            healthcheck_json: None,
            judge_script_name: None,
            judge_script_content: None,
            judge_args_json: None,
            default_judge_timeout_secs: None,
            default_judge_retry_interval_secs: None,
        },
    )
    .await
    .expect("repo call");
    assert!(
        none.is_none(),
        "相同 canonical spec 不得创建重复 revision（§36）"
    );

    // 不同 digest → Revision 2，Revision 1 保持不动
    let some = gamebox_lib_repo::create_revision(
        &db,
        gb_id,
        gamebox_lib_repo::NewRevision {
            source_toml: String::new(),
            spec_json: serde_json::json!({"image_ref": "img:v2"}),
            spec_digest: "digest-b".into(),
            image_ref: "img:v2".into(),
            image_digest: None,
            username: "ctf".into(),
            default_cpu_millis: 2000,
            default_memory_bytes: 1024 * 1024 * 1024,
            default_pids_limit: 100,
            healthcheck_json: None,
            judge_script_name: None,
            judge_script_content: None,
            judge_args_json: None,
            default_judge_timeout_secs: None,
            default_judge_retry_interval_secs: None,
        },
    )
    .await
    .expect("repo call");
    let rev2 = some.expect("revision 2 created");
    assert_eq!(rev2.revision_number, 2);
    assert_eq!(rev2.image_ref, "img:v2");

    // 旧 revision 未变（immutable）
    let revs = gamebox_lib_repo::find_revisions_by_gamebox(&db, gb_id)
        .await
        .expect("list");
    assert_eq!(revs.len(), 2);
    let rev1 = revs.iter().find(|r| r.revision_number == 1).unwrap();
    assert_eq!(rev1.image_ref, "img:v1", "旧 revision 不可变（§6）");
}

// ────────────────────────────────────────────────────────────────────────────
// §70 赛事 pin 隔离 + §71 计分按 Event 独立
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn event_pins_revision_and_scores_are_independent() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    use floatctf::modules::event::awd_team::{repo::gamebox_lib_repo, service::gamebox_service};

    let (gb_id, rev1) = seed_gamebox_with_revision(&db, "pin", "digest-p1", "img:v1").await;
    let event_a = seed_running_event(&db, "pin-a").await;
    let event_b = seed_running_event(&db, "pin-b").await;

    // Event A 用 break=100 pin rev1；Event B 用 break=200 pin rev1（同 GameBox）
    let eg_a = seed_event_gamebox(&db, event_a, gb_id, rev1, 10, 100).await;
    let eg_b = seed_event_gamebox(&db, event_b, gb_id, rev1, 10, 200).await;

    // 全局发布 Revision 2（Event A 不受影响）
    let rev2 = gamebox_lib_repo::create_revision(
        &db,
        gb_id,
        gamebox_lib_repo::NewRevision {
            source_toml: String::new(),
            spec_json: serde_json::json!({"image_ref": "img:v2"}),
            spec_digest: "digest-p2".into(),
            image_ref: "img:v2".into(),
            image_digest: None,
            username: "ctf".into(),
            default_cpu_millis: 2000,
            default_memory_bytes: 1024 * 1024 * 1024,
            default_pids_limit: 100,
            healthcheck_json: None,
            judge_script_name: None,
            judge_script_content: None,
            judge_args_json: None,
            default_judge_timeout_secs: None,
            default_judge_retry_interval_secs: None,
        },
    )
    .await
    .expect("rev2");
    assert_eq!(rev2.as_ref().unwrap().revision_number, 2);

    // Event A 仍解析 rev1（pin 隔离）
    let ra = gamebox_service::resolve_event_gamebox_spec(&db, eg_a)
        .await
        .expect("resolve A");
    assert_eq!(ra.revision.id, rev1, "Event A 必须继续 pin rev1（§35/§68）");
    assert_eq!(ra.event_gamebox.break_points, 100);

    // Event B 独立计分
    let rb = gamebox_service::resolve_event_gamebox_spec(&db, eg_b)
        .await
        .expect("resolve B");
    assert_eq!(
        rb.event_gamebox.break_points, 200,
        "同 GameBox 在不同 Event 计分独立（§71）"
    );
    assert_eq!(ra.event_gamebox.break_points, 100);
}

// ────────────────────────────────────────────────────────────────────────────
// §67 Reset 保持 logical identity + runtime_generation+1 + §68 pinned revision
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reset_keeps_identity_bumps_generation_and_uses_pinned_revision() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    configure_crypto_once();
    use floatctf::modules::event::awd_team::{
        repo::gamebox_lib_repo,
        service::reset_service::{ResetActor, ResetContext, execute_reset},
    };

    let event_id = seed_running_event(&db, "reset").await;
    let team_id = seed_team(&db, event_id, "reset").await;
    let _net_id = seed_team_network(&db, event_id, team_id, "10.42.1.0/24").await;

    // Player user（awd_reset_records.requested_by FK → users）
    let user_id = Uuid::new_v4();
    let uname = format!("user-reset-{}", &user_id.to_string().replace('-', "")[..8]);
    floatctf::entity::users::ActiveModel {
        id: Set(user_id),
        username: Set(uname.clone()),
        nickname: Set(uname.clone()),
        password: Set("x".into()),
        email: Set(format!("{uname}@test.invalid")),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert user");

    let (gb_id, rev1) = seed_gamebox_with_revision(&db, "reset", "digest-r1", "img:v1").await;
    let eg_id = seed_event_gamebox(&db, event_id, gb_id, rev1, 10, 100).await;

    // 全局发布 rev2（Reset 必须仍用 rev1）
    gamebox_lib_repo::create_revision(
        &db,
        gb_id,
        gamebox_lib_repo::NewRevision {
            source_toml: String::new(),
            spec_json: serde_json::json!({"image_ref": "img:v2"}),
            spec_digest: "digest-r2".into(),
            image_ref: "img:v2".into(),
            image_digest: None,
            username: "ctf".into(),
            default_cpu_millis: 2000,
            default_memory_bytes: 1024 * 1024 * 1024,
            default_pids_limit: 100,
            healthcheck_json: None,
            judge_script_name: None,
            judge_script_content: None,
            judge_args_json: None,
            default_judge_timeout_secs: None,
            default_judge_retry_interval_secs: None,
        },
    )
    .await
    .expect("rev2");

    // Instance：generation=1, container=C1, Ready
    let instance_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    awd_gamebox_instances::ActiveModel {
        id: Set(instance_id),
        event_id: Set(event_id),
        event_gamebox_id: Set(eg_id),
        team_id: Set(team_id),
        status: Set(GameboxStatus::Ready),
        container_name: Set(format!("fctf-gb-reset-{}", &event_id.to_string()[..8])),
        gamebox_ip: Set("10.42.1.10".parse().unwrap()),
        runtime_generation: Set(1),
        current_container_id: Set(Some("cid-old".into())),
        health_status: Set("healthy".into()),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert instance");

    let runtime = Arc::new(MockContainerRuntime::default());
    let result = execute_reset(
        &db,
        runtime.as_ref() as &dyn fcmc::AwdContainerRuntime,
        ResetContext {
            event_id,
            instance_id,
            team_id,
            actor: ResetActor::Player { user_id, team_id },
        },
    )
    .await;
    result.expect("reset succeeds");

    // logical identity 不变；container 更换；generation +1
    let inst = floatctf::entity::awd_gamebox_instances::Entity::find_by_id(instance_id)
        .one(&db)
        .await
        .expect("reload")
        .expect("exists");
    assert_eq!(inst.id, instance_id);
    assert_eq!(inst.event_gamebox_id, eg_id, "event_gamebox_id 不变（§24）");
    assert_eq!(inst.team_id, team_id, "team_id 不变");
    assert_eq!(inst.gamebox_ip.to_string(), "10.42.1.10/32", "IP 不变");
    assert_eq!(inst.runtime_generation, 2, "runtime_generation +1（§20）");
    let new_cid = inst.current_container_id.as_deref().expect("new container");
    assert_ne!(new_cid, "cid-old", "current container 更换（C1 != C2）");
    assert!(new_cid.starts_with("cid-new-"), "容器 ID 来自 mock reset");
    assert_eq!(inst.status, GameboxStatus::Ready);

    // §68：Reset 使用 pinned rev1 的镜像（不是全局 latest rev2）
    let recorded = runtime.last_reset_spec.lock().unwrap().clone();
    let spec = recorded.expect("reset spec recorded");
    assert_eq!(
        spec.image_ref, "img:v1",
        "Reset 必须使用 Event pin 的 Revision（§68）"
    );
    assert_eq!(spec.runtime_generation, 2, "fcmc spec 携带新 generation");
    assert_eq!(spec.fixed_ip, "10.42.1.10");
    assert_eq!(spec.username, "ctf");
    assert_eq!(*runtime.reset_count.lock().unwrap(), 1);
}
