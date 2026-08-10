//! GameBox identity + Revision 领域回归测试（DB-gated）。
//!
//! 覆盖：
//!   - 身份元数据更新不改 revision
//!   - 赛事选择 pin ready revision；计分按 Event 独立
//!   - Reset 保持 logical identity，镜像来自 pinned revision

use std::sync::{Arc, Mutex};

/// 本文件 DB-gated 测试共享 cleanup 前缀，必须串行执行。
static TEST_SERIAL: Mutex<()> = Mutex::new(());

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    awd_event_gameboxes, awd_event_networks, awd_events, awd_gamebox_instances, awd_team_networks,
    event_teams, events, gamebox_revisions, gameboxes, sea_orm_active_enums,
    sea_orm_active_enums::{AwdEventStatus, AwdPhase, GameboxStatus},
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

async fn cleanup_domain_fixtures(db: &sea_orm::DatabaseConnection) {
    events::Entity::delete_many()
        .filter(events::Column::Title.like("awd-gb-domain-%"))
        .exec(db)
        .await
        .expect("cleanup events");
    // Revisions cascade from gameboxes
    gameboxes::Entity::delete_many()
        .filter(
            gameboxes::Column::SafeName
                .like("gb-edit-%")
                .or(gameboxes::Column::SafeName.like("gb-score-%"))
                .or(gameboxes::Column::SafeName.like("gb-reset-%")),
        )
        .exec(db)
        .await
        .expect("cleanup gameboxes");
}

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

/// Seed GameBox identity + ready revision with given image pin.
/// Returns (gamebox_id, revision_id).
async fn seed_gamebox_with_revision(
    db: &sea_orm::DatabaseConnection,
    tag: &str,
    image_pin: &str,
) -> (Uuid, Uuid) {
    let now = chrono::Utc::now();
    let gb_id = Uuid::new_v4();
    let rev_id = Uuid::new_v4();
    let safe = format!("gb-{tag}-{}", &gb_id.to_string()[..8]);

    gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(format!("gb-{tag}")),
        safe_name: Set(safe),
        category: Set("other".into()),
        description: Set(String::new()),
        hidden: Set(false),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
    }
    .insert(db)
    .await
    .expect("insert gamebox");

    // LocalOnly pin via image_id when pin looks like sha256:, else store as image_ref + image_id.
    let (image_ref, image_id, image_repo_digest) = if image_pin.contains("@sha256:") {
        (
            Some(image_pin.split('@').next().unwrap().to_string() + ":1.0.0"),
            None,
            Some(image_pin.to_string()),
        )
    } else if image_pin.starts_with("sha256:") {
        (
            Some(format!("floatctf/gameboxes/{tag}:1.0.0")),
            Some(image_pin.to_string()),
            None,
        )
    } else {
        // Tag form: store as image_id too so ready pin validation passes (LocalOnly style).
        (
            Some(image_pin.to_string()),
            Some(format!(
                "sha256:fake{}",
                &gb_id.to_string().replace('-', "")[..12]
            )),
            None,
        )
    };

    gamebox_revisions::ActiveModel {
        id: Set(rev_id),
        gamebox_id: Set(gb_id),
        version: Set("1.0.0".into()),
        revision_number: Set(1),
        source_toml: Set(String::new()),
        spec_json: Set(serde_json::json!({"name": tag})),
        spec_digest: Set("specdigest".into()),
        package_digest: Set("pkgdigest".into()),
        image_ref: Set(image_ref),
        image_id: Set(image_id),
        image_repo_digest: Set(image_repo_digest),
        username: Set("ctf".into()),
        recommended_cpu_millis: Set(1000),
        recommended_memory_bytes: Set(512 * 1024 * 1024),
        recommended_pids_limit: Set(100),
        healthchecks_json: Set(serde_json::json!([])),
        judge_script_name: Set(None),
        judge_script_content: Set(Some("#!/bin/sh\nexit 0".into())),
        judge_args_json: Set(None),
        judge_timeout_secs: Set(None),
        judge_retry_interval_secs: Set(None),
        build_status: Set("ready".into()),
        build_error: Set(None),
        created_at: Set(now.into()),
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
    }
    .insert(db)
    .await
    .expect("insert event_gamebox");
    eg_id
}

// ────────────────────────────────────────────────────────────────────────────
// Mock AwdContainerRuntime
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
// 身份元数据更新（不改 revision）
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_identity_does_not_touch_revision() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup_domain_fixtures(&db).await;

    let (gb_id, rev_id) = seed_gamebox_with_revision(&db, "edit", "img:v1").await;

    let updated =
        floatctf::modules::event::awd_team::service::gamebox_service::update_gamebox_identity(
            &db,
            gb_id,
            Some("new-name".into()),
            Some("pwn".into()),
            Some("desc".into()),
            Some(true),
        )
        .await
        .expect("update identity");
    assert_eq!(updated.name, "new-name");
    assert_eq!(updated.category, "pwn");
    assert!(updated.hidden);

    let rev = gamebox_revisions::Entity::find_by_id(rev_id)
        .one(&db)
        .await
        .expect("db")
        .expect("rev");
    assert_eq!(rev.version, "1.0.0");
    assert_eq!(rev.build_status, "ready");
    // image pin unchanged
    assert!(rev.image_id.is_some() || rev.image_repo_digest.is_some());

    cleanup_domain_fixtures(&db).await;
}

// ────────────────────────────────────────────────────────────────────────────
// 赛事选择 pin revision；计分按 Event 独立
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn event_scores_are_independent_with_pinned_revision() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup_domain_fixtures(&db).await;
    use floatctf::modules::event::awd_team::service::gamebox_service;

    let (gb_id, rev_id) = seed_gamebox_with_revision(&db, "score", "img:v1").await;
    let event_a = seed_running_event(&db, "score-a").await;
    let event_b = seed_running_event(&db, "score-b").await;

    let eg_a = seed_event_gamebox(&db, event_a, gb_id, rev_id, 10, 100).await;
    let eg_b = seed_event_gamebox(&db, event_b, gb_id, rev_id, 10, 200).await;

    let ra = gamebox_service::resolve_event_gamebox_spec(&db, eg_a)
        .await
        .expect("resolve A");
    let rb = gamebox_service::resolve_event_gamebox_spec(&db, eg_b)
        .await
        .expect("resolve B");
    assert_eq!(ra.revision.id, rev_id);
    assert_eq!(rb.revision.id, rev_id, "同 GameBox pin 同一 revision");
    assert_eq!(ra.event_gamebox.break_points, 100);
    assert_eq!(
        rb.event_gamebox.break_points, 200,
        "同 GameBox 在不同 Event 计分独立"
    );
    assert_eq!(ra.gamebox.id, gb_id);
    // effective image is pin (image_id LocalOnly style)
    let img = ra.effective_image_ref().expect("pin");
    assert!(img.starts_with("sha256:") || img.contains("@sha256:"));

    cleanup_domain_fixtures(&db).await;
}

// ────────────────────────────────────────────────────────────────────────────
// Reset 保持 identity + 使用 pinned revision 镜像
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reset_keeps_identity_and_uses_pinned_revision_image() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup_domain_fixtures(&db).await;
    configure_crypto_once();
    use floatctf::modules::event::awd_team::service::reset_service::{
        ResetActor, ResetContext, execute_reset,
    };

    let event_id = seed_running_event(&db, "reset").await;
    let team_id = seed_team(&db, event_id, "reset").await;
    let _net_id = seed_team_network(&db, event_id, team_id, "10.42.1.0/24").await;

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

    let (gb_id, rev_id) = seed_gamebox_with_revision(&db, "reset", "img:v1").await;
    let eg_id = seed_event_gamebox(&db, event_id, gb_id, rev_id, 10, 100).await;

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

    let inst = floatctf::entity::awd_gamebox_instances::Entity::find_by_id(instance_id)
        .one(&db)
        .await
        .expect("reload")
        .expect("exists");
    assert_eq!(inst.id, instance_id);
    assert_eq!(inst.event_gamebox_id, eg_id);
    assert_eq!(inst.team_id, team_id);
    assert_eq!(inst.gamebox_ip.to_string(), "10.42.1.10/32");
    assert_eq!(inst.runtime_generation, 2);
    let new_cid = inst.current_container_id.as_deref().expect("new container");
    assert_ne!(new_cid, "cid-old");
    assert!(new_cid.starts_with("cid-new-"));
    assert_eq!(inst.status, GameboxStatus::Ready);

    let recorded = runtime.last_reset_spec.lock().unwrap().clone();
    let spec = recorded.expect("reset spec recorded");
    // Pinned image is image_id (LocalOnly style fake sha256)
    assert!(
        spec.image_ref.starts_with("sha256:") || spec.image_ref.contains("@sha256:"),
        "Reset 使用 pinned revision 镜像, got {}",
        spec.image_ref
    );
    assert_eq!(spec.runtime_generation, 2);
    assert_eq!(spec.fixed_ip, "10.42.1.10");
    assert_eq!(spec.username, "ctf");
    assert_eq!(*runtime.reset_count.lock().unwrap(), 1);

    floatctf::entity::users::Entity::delete_many()
        .filter(floatctf::entity::users::Column::Username.eq(uname))
        .exec(&db)
        .await
        .expect("cleanup user");
    cleanup_domain_fixtures(&db).await;
}
