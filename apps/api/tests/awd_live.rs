//! Real-host AWD lifecycle validation.
//!
//! Ignored by default because it mutates the host Docker/WireGuard/nftables state through
//! floatctf-helper. Run only against an isolated PostgreSQL database.
//!
//! Required environment:
//! - `DATABASE_URL`
//! - `FLOATCTF_RUN_AWD_HOST_E2E=1`
//! - `FLOATCTF_TEST_GAMEBOX_IMAGE_ID=sha256:...`
//!
//! Optional:
//! - `FLOATCTF_TEST_DOCKER_SOCKET` (default `/run/floatctf/helper-docker.sock`)
//! - `FLOATCTF_TEST_HELPER_SOCKET` (default `/run/floatctf/helper-control.sock`)
//! - `FLOATCTF_TEST_FLAGSERVER_IMAGE` / `FLOATCTF_TEST_JUDGESERVER_IMAGE`
//! - `FLOATCTF_TEST_PLATFORM_INTERNAL_NETWORK` (e.g. `fctf-platform-control`)
//! - `FLOATCTF_TEST_PLATFORM_INTERNAL_URL` (default `http://10.42.8.2:9090` when network set)

use std::collections::HashSet;

use bollard::{API_DEFAULT_VERSION, Docker};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use fcmc::{AwdContainerRuntime, DockerRuntime};
use floatctf::{
    core::config::AwdStaticConfig,
    entity::{
        awd_event_gameboxes, awd_events, awd_precheck_runs, awd_runtime_resources, event_teams,
        events, gameboxes,
        sea_orm_active_enums::{AwdEventStatus, AwdPhase, ParticipantMode, PrecheckStatus},
    },
    infrastructure::realtime::NoopEventPublisher,
    modules::event::{
        awd::{
            crypto::AwdCrypto,
            domain::network::wireguard_interface_name,
            infrastructure::{
                firewall::{FirewallRuntime, HelperFirewallRuntime, TABLE_NAME},
                network::{AwdNetworkRuntime, EventNetworkIdentity, HelperNetworkRuntime},
            },
            repo::event_repo,
            service::{
                archive_service, deploy_service, event_network_service, event_service,
                precheck_service,
            },
        },
        common::application::admin_service::{self, CreateEventRequest},
    },
};

const DEFAULT_HELPER_SOCKET: &str = "/run/floatctf/helper-control.sock";
const DEFAULT_HELPER_DOCKER_SOCKET: &str = "/run/floatctf/helper-docker.sock";

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn optional_env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

async fn seed_awd_fixture(
    db: &sea_orm::DatabaseConnection,
    crypto: &AwdCrypto,
    image_id: &str,
) -> (Uuid, Uuid, Uuid) {
    let now = Utc::now();
    // Exactly one 30-second attack round => hardening duration = 0.
    let start = (now + Duration::minutes(5)).fixed_offset();
    let end = (now + Duration::minutes(5) + Duration::seconds(30)).fixed_offset();
    let event = admin_service::create_event(
        db,
        CreateEventRequest {
            family: floatctf::entity::sea_orm_active_enums::EventFamily::Awd,
            participant_mode: ParticipantMode::Team,
            purpose: None,
            title: format!("awd-live-{}", Uuid::new_v4().simple()),
            description: Some("real helper AWD lifecycle e2e".into()),
            hidden: true,
            allow_join: false,
            rules: "host-e2e".into(),
            flag_prefix: Some("flag".into()),
            start_time: start,
            end_time: end,
        },
    )
    .await
    .expect("create generic AWD event");

    let secret_blob = crypto
        .encrypt(
            &AwdCrypto::generate_event_secret(),
            &AwdCrypto::build_aad(event.id, "event_secret"),
            1,
        )
        .expect("encrypt event secret");
    let token_aad = AwdCrypto::build_aad(event.id, "internal_token");
    let flagserver_blob = crypto
        .encrypt(&AwdCrypto::generate_token(), &token_aad, 1)
        .expect("encrypt flagserver token");
    let judgeserver_blob = crypto
        .encrypt(&AwdCrypto::generate_token(), &token_aad, 1)
        .expect("encrypt judgeserver token");

    let awd_id = Uuid::new_v4();
    awd_events::ActiveModel {
        id: Set(awd_id),
        event_id: Set(event.id),
        status: Set(AwdEventStatus::Configuring),
        phase: Set(AwdPhase::Hardening),
        event_secret_ciphertext: Set(secret_blob.ciphertext),
        event_secret_nonce: Set(secret_blob.nonce),
        flagserver_token_ciphertext: Set(Some(flagserver_blob.ciphertext)),
        flagserver_token_nonce: Set(Some(flagserver_blob.nonce)),
        judgeserver_token_ciphertext: Set(Some(judgeserver_blob.ciphertext)),
        judgeserver_token_nonce: Set(Some(judgeserver_blob.nonce)),
        key_version: Set(1),
        free_reset_count: Set(1),
        extra_reset_penalty: Set(100),
        judge_max_concurrency: Set(2),
        judge_default_timeout_secs: Set(10),
        judge_retry_interval_secs: Set(2),
        judge_grace_period_secs: Set(2),
        round_duration_secs: Set(30),
        archive_retention_hours: Set(1),
        configuration_generation: Set(1),
        round_count: Set(Some(1)),
        initial_score: Set(1000),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert AWD event config");

    let team_id = Uuid::new_v4();
    event_teams::ActiveModel {
        id: Set(team_id),
        event_id: Set(event.id),
        name: Set(format!(
            "host-e2e-team-{}",
            &team_id.simple().to_string()[..8]
        )),
        points: Set(0.0),
        banned: Set(false),
        created_at: Set(Utc::now().into()),
        updated_at: Set(Utc::now().into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert AWD team");

    let gamebox_id = Uuid::new_v4();
    let safe_name = format!("awd-live-{}", &gamebox_id.simple().to_string()[..8]);
    gameboxes::ActiveModel {
        id: Set(gamebox_id),
        name: Set("AWD live GameBox".into()),
        safe_name: Set(safe_name),
        category: Set("other".into()),
        description: Set("real helper e2e".into()),
        hidden: Set(true),
        created_at: Set(Utc::now().into()),
        updated_at: Set(Utc::now().into()),
        version: Set(Some("e2e".into())),
        source_toml: Set(None),
        spec_json: Set(Some(serde_json::json!({"test":"awd-live"}))),
        spec_digest: Set(Some("awd-live-spec".into())),
        package_digest: Set(Some("awd-live-package".into())),
        image_ref: Set(Some("floatctf/awd-live-gamebox:e2e".into())),
        image_id: Set(Some(image_id.to_string())),
        image_repo_digest: Set(None),
        username: Set(Some("ctf".into())),
        recommended_cpu_millis: Set(250),
        recommended_memory_bytes: Set(128 * 1024 * 1024),
        recommended_pids_limit: Set(64),
        healthchecks_json: Set(Some(serde_json::json!([]))),
        judge_script_name: Set(Some("judge.sh".into())),
        judge_script_content: Set(Some("#!/bin/sh\nexit 0\n".into())),
        judge_args_json: Set(None),
        judge_timeout_secs: Set(Some(5)),
        judge_retry_interval_secs: Set(Some(1)),
        awdp_source_code_dir: Set(None),
        awdp_exploit_script_name: Set(None),
        awdp_exploit_script_content: Set(None),
        awdp_source_artifact_key: Set(None),
        awdp_source_artifact_digest: Set(None),
        build_status: Set(Some("ready".into())),
        build_error: Set(None),
    }
    .insert(db)
    .await
    .expect("insert GameBox");

    let event_gamebox_id = Uuid::new_v4();
    awd_event_gameboxes::ActiveModel {
        id: Set(event_gamebox_id),
        event_id: Set(event.id),
        gamebox_id: Set(gamebox_id),
        host_offset: Set(10),
        enabled: Set(true),
        hidden: Set(false),
        cpu_millis: Set(250),
        memory_bytes: Set(128 * 1024 * 1024),
        pids_limit: Set(64),
        healthcheck_override_json: Set(None),
        judge_timeout_secs: Set(Some(5)),
        judge_retry_interval_secs: Set(Some(1)),
        attack_score: Set(100),
        judge_down_penalty: Set(20),
        first_bonus: Set(10),
        created_at: Set(Utc::now().into()),
        updated_at: Set(Utc::now().into()),
    }
    .insert(db)
    .await
    .expect("insert EventGameBox");

    (event.id, awd_id, team_id)
}

async fn assert_event_container_kinds(
    runtime: &DockerRuntime,
    event_id: Uuid,
) -> Vec<fcmc::ContainerState> {
    let containers = runtime
        .list_event_containers(event_id)
        .await
        .expect("list event containers");
    assert_eq!(
        containers.len(),
        3,
        "flagserver + judgeserver + one GameBox"
    );
    assert!(
        containers.iter().all(|c| c.running),
        "all event containers running"
    );
    let names: HashSet<&str> = containers
        .iter()
        .map(|c| c.container_name.as_str())
        .collect();
    assert!(names.iter().any(|n| n.contains("flagserver")));
    assert!(names.iter().any(|n| n.contains("judgeserver")));
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("fctf-") && n.contains("-team"))
    );
    containers
}

async fn assert_control_network_membership(
    runtime: &DockerRuntime,
    event_network: &str,
    control_network: &str,
    containers: &[fcmc::ContainerState],
) {
    for state in containers {
        let inspected = runtime
            .inner()
            .inspect_container(
                &state.container_id,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
            .expect("inspect container networks through helper proxy");
        let networks = inspected
            .network_settings
            .and_then(|v| v.networks)
            .unwrap_or_default();
        assert!(
            networks.contains_key(event_network),
            "{} must remain attached to event data network {}",
            state.container_name,
            event_network
        );
        let is_infra = state.container_name.contains("flagserver")
            || state.container_name.contains("judgeserver");
        assert_eq!(
            networks.contains_key(control_network),
            is_infra,
            "only FlagServer/JudgeServer may join production control network: {} networks={:?}",
            state.container_name,
            networks.keys().collect::<Vec<_>>()
        );
    }
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL + real floatctf-helper + host Docker/WireGuard/nftables"]
async fn real_helper_deploy_precheck_pause_resume_archive() {
    assert_eq!(required_env("FLOATCTF_RUN_AWD_HOST_E2E"), "1");
    let db_url = required_env("DATABASE_URL");
    let gamebox_image_id = required_env("FLOATCTF_TEST_GAMEBOX_IMAGE_ID");
    assert!(gamebox_image_id.starts_with("sha256:"));

    let helper_socket = optional_env("FLOATCTF_TEST_HELPER_SOCKET", DEFAULT_HELPER_SOCKET);
    let docker_socket = optional_env("FLOATCTF_TEST_DOCKER_SOCKET", DEFAULT_HELPER_DOCKER_SOCKET);
    let flagserver_image = optional_env(
        "FLOATCTF_TEST_FLAGSERVER_IMAGE",
        "floatctf/awd-flagserver:latest",
    );
    let judgeserver_image = optional_env(
        "FLOATCTF_TEST_JUDGESERVER_IMAGE",
        "floatctf/awd-judgeserver:latest",
    );
    let platform_internal_network = std::env::var("FLOATCTF_TEST_PLATFORM_INTERNAL_NETWORK")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let platform_internal_url = if platform_internal_network.is_some() {
        optional_env(
            "FLOATCTF_TEST_PLATFORM_INTERNAL_URL",
            "http://10.42.8.2:9090",
        )
    } else {
        "http://127.0.0.1:19090".to_string()
    };

    let db = sea_orm::Database::connect(db_url)
        .await
        .expect("connect isolated AWD E2E DB");
    let docker = Docker::connect_with_unix(&docker_socket, 120, API_DEFAULT_VERSION)
        .expect("connect helper Docker socket");
    docker.ping().await.expect("helper Docker ping");
    let containers = DockerRuntime::new(docker);
    let network = HelperNetworkRuntime::new(helper_socket.clone());
    let firewall = HelperFirewallRuntime::new(helper_socket);
    let crypto = AwdCrypto::from_secret_bytes(b"floatctf-awd-live-host-e2e-master-key-2026")
        .expect("construct AWD crypto");

    let (event_id, awd_id, _team_id) = seed_awd_fixture(&db, &crypto, &gamebox_image_id).await;
    println!("AWD_LIVE_EVENT_ID={event_id}");

    let reserved = event_network_service::collect_external_reserved_cidrs(&containers, &network)
        .await
        .expect("collect host reserved CIDRs");
    let allocated = event_network_service::allocate_automatic(&db, event_id, &reserved)
        .await
        .expect("allocate real AWD event network");
    assert!(allocated.locked_at.is_none());

    let static_config = AwdStaticConfig {
        crypto_from_app_secret: false,
        network_runtime: "helper".into(),
        flagserver_image,
        judgeserver_image,
        platform_internal_url,
        platform_internal_network: platform_internal_network.clone(),
    };

    deploy_service::deploy_event(
        &db,
        &containers,
        &network,
        &firewall,
        &crypto,
        &static_config,
        event_id,
    )
    .await
    .expect("real helper AWD deploy");

    let deployed = event_repo::find_by_event_id(&db, event_id)
        .await
        .expect("query deployed AWD event")
        .expect("AWD event exists");
    assert_eq!(deployed.status, AwdEventStatus::Deployed);
    let allocated =
        floatctf::modules::event::awd::repo::event_network_repo::find_by_event_id(&db, event_id)
            .await
            .expect("query event network")
            .expect("event network exists");
    assert!(allocated.locked_at.is_some(), "deploy locks event network");
    let live_containers = assert_event_container_kinds(&containers, event_id).await;
    if let Some(control_network) = platform_internal_network.as_deref() {
        assert_control_network_membership(
            &containers,
            &allocated.docker_network_name,
            control_network,
            &live_containers,
        )
        .await;
    }

    let observed_network = network
        .inspect(EventNetworkIdentity {
            event_id,
            gamebox_cidr: allocated.gamebox_cidr.to_string(),
        })
        .await
        .expect("inspect helper WireGuard state");
    assert!(
        observed_network.wireguard_interface_up,
        "real helper WireGuard interface must be up: {:?}",
        observed_network.notes
    );
    let observed_firewall = firewall.inspect().await.expect("inspect nftables state");
    assert!(
        observed_firewall.table_exists,
        "{TABLE_NAME} nftables table must exist"
    );

    let precheck_id = precheck_service::run_precheck(
        &db,
        event_id,
        "host-e2e",
        &network,
        &firewall,
        &containers,
        &crypto,
    )
    .await
    .expect("real helper AWD precheck");
    let precheck = awd_precheck_runs::Entity::find_by_id(precheck_id)
        .one(&db)
        .await
        .expect("query precheck")
        .expect("precheck exists");
    assert_eq!(
        precheck.status,
        PrecheckStatus::Passed,
        "precheck details: {:?}",
        precheck.error_msg
    );
    let verified = event_repo::find_by_event_id(&db, event_id)
        .await
        .expect("query verified")
        .expect("verified event exists");
    assert_eq!(verified.status, AwdEventStatus::Verified);
    assert_eq!(
        verified.verified_generation,
        Some(verified.configuration_generation)
    );

    let publisher = NoopEventPublisher;
    event_service::start_event(&db, &network, &firewall, &publisher, event_id)
        .await
        .expect("start AWD event");
    let running = event_repo::find_by_event_id(&db, event_id)
        .await
        .expect("query running")
        .expect("running event exists");
    assert_eq!(running.status, AwdEventStatus::Running);
    assert_eq!(running.phase, AwdPhase::Attack);

    event_service::pause_event(&db, &network, &firewall, event_id)
        .await
        .expect("pause AWD event");
    let paused = event_repo::find_by_event_id(&db, event_id)
        .await
        .expect("query paused")
        .expect("paused event exists");
    assert_eq!(paused.status, AwdEventStatus::Paused);
    assert_eq!(paused.phase, AwdPhase::Pause);

    let tracked_network_id = awd_runtime_resources::Entity::find()
        .filter(awd_runtime_resources::Column::EventId.eq(event_id))
        .filter(awd_runtime_resources::Column::ResourceType.eq("docker_network"))
        .one(&db)
        .await
        .expect("query tracked Docker network")
        .expect("tracked Docker network exists")
        .resource_id;

    event_service::resume_event(&db, &network, &firewall, &publisher, event_id)
        .await
        .expect("resume AWD event");
    let resumed = event_repo::find_by_event_id(&db, event_id)
        .await
        .expect("query resumed")
        .expect("resumed event exists");
    assert_eq!(resumed.status, AwdEventStatus::Running);
    assert_eq!(resumed.phase, AwdPhase::Attack);

    // Final-settlement semantics have dedicated DB suites. For this real-host test, transition
    // through the legal Running -> Finished edge so archive can validate physical teardown.
    event_repo::transition_event(
        &db,
        awd_id,
        AwdEventStatus::Running,
        AwdEventStatus::Finished,
        event_repo::TransitionPatch::finished(),
    )
    .await
    .expect("mark live fixture finished");

    archive_service::archive_event(&db, &containers, &network, &firewall, event_id)
        .await
        .expect("archive and tear down real AWD resources");
    let archived = event_repo::find_by_event_id(&db, event_id)
        .await
        .expect("query archived")
        .expect("archived event exists");
    assert_eq!(archived.status, AwdEventStatus::Archived);
    assert!(
        containers
            .list_event_containers(event_id)
            .await
            .expect("list containers after archive")
            .is_empty(),
        "archive must remove FlagServer, JudgeServer and all GameBoxes"
    );
    assert!(
        containers
            .inspect_event_network(&tracked_network_id)
            .await
            .is_err(),
        "archive must remove event Docker network"
    );
    let after_network = network
        .inspect(EventNetworkIdentity {
            event_id,
            gamebox_cidr: allocated.gamebox_cidr.to_string(),
        })
        .await
        .expect("inspect network after archive");
    assert!(
        !after_network.wireguard_interface_up,
        "archive must remove {}",
        wireguard_interface_name(&event_id)
    );
    let after_firewall = firewall
        .inspect()
        .await
        .expect("inspect nftables after archive");
    assert!(
        !after_firewall.table_exists,
        "isolated DB has no remaining AWD event; archive must remove {TABLE_NAME}"
    );

    // DB rows intentionally remain for audit; the caller uses an isolated database.
    assert!(
        events::Entity::find_by_id(event_id)
            .one(&db)
            .await
            .expect("query generic event")
            .is_some()
    );
}
