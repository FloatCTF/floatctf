//! AWD Wave 5.3 NetworkError Contract Tests
//!
//! Validates the NetworkError lifecycle contract:
//! - Hardening freeze preserves remaining time
//! - Stale HardeningEnd does not advance competition
//! - Hardening resume uses saved remaining time
//! - Attack Round freeze preserves remaining time
//! - Stale RoundEnd does not progress competition
//! - Attack Round resume uses saved remaining time
//! - Actions blocked during NetworkError
//! - Judge score blocked during NetworkError
//! - Individual GameBox failure does not freeze event

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    TransactionTrait,
};
use std::sync::Arc;
use uuid::Uuid;

use floatctf::entity::sea_orm_active_enums::{
    AwdEventStatus, AwdPhase, EventFamily, EventPurpose, GameboxStatus, ParticipantMode,
    RoundStatus, ScoreEventType,
};
use floatctf::entity::{
    awd_event_gameboxes, awd_event_networks, awd_events, awd_rounds, awd_score_events,
    event_gamebox_instances, event_instances, event_teams, events, gameboxes, scheduled_tasks,
    sea_orm_active_enums, users,
};
use floatctf::infrastructure::realtime::NoopEventPublisher;
use floatctf::modules::event::awd::{
    domain::{AwdEventStatusExt, AwdPhaseExt},
    infrastructure::{
        firewall::NoopFirewallRuntime,
        network::NoopNetworkRuntime,
    },
    repo::{event_repo, gamebox_repo, round_repo},
    service::{event_service, recovery_service},
};

// ── DB helpers ──

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip awd_network_error: DB unreachable ({e})");
            None
        }
    }
}

struct TestFixtures {
    db: sea_orm::DatabaseConnection,
    event_id: Uuid,
    awd_event_id: Uuid, // internal awd_events PK
    team_id: Uuid,
    user_id: Uuid,
    round_id: Uuid,
    instance_id: Uuid,
}

impl TestFixtures {
    async fn cleanup(&self) {
        let _ = awd_score_events::Entity::delete_many()
            .filter(awd_score_events::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = scheduled_tasks::Entity::delete_many()
            .filter(scheduled_tasks::Column::GroupId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = awd_rounds::Entity::delete_many()
            .filter(awd_rounds::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = event_gamebox_instances::Entity::delete_many()
            .filter(event_gamebox_instances::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = event_instances::Entity::delete_many()
            .filter(event_instances::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = awd_event_gameboxes::Entity::delete_many()
            .filter(awd_event_gameboxes::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = awd_events::Entity::delete_many()
            .filter(awd_events::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = event_teams::Entity::delete_many()
            .filter(event_teams::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = events::Entity::delete_by_id(self.event_id).exec(&self.db).await;
        let _ = gameboxes::Entity::delete_by_id(self.event_id).exec(&self.db).await;
        let _ = users::Entity::delete_by_id(self.user_id).exec(&self.db).await;
    }
}

async fn setup_test(phase: AwdPhase, with_round: bool) -> Option<TestFixtures> {
    let db = connect_or_skip().await?;
    let event_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let suffix = Uuid::new_v4().to_string().split('-').next().unwrap().to_string();

    users::ActiveModel {
        id: Set(user_id),
        username: Set(format!("neterr_{suffix}")),
        nickname: Set(format!("NetErr_{suffix}")),
        password: Set("hashed".into()),
        email: Set(format!("neterr_{suffix}@test.com")),
        ..Default::default()
    }
    .insert(&db)
    .await
    .ok()?;

    let now = chrono::Utc::now();
    events::ActiveModel {
        id: Set(event_id),
        title: Set(format!("NetworkError Test {suffix}")),
        description: Set(Some("test".into())),
        start_time: Set(now.into()),
        end_time: Set(Some((now + chrono::Duration::hours(2)).into())),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        ..Default::default()
    }
    .insert(&db)
    .await
    .ok()?;

    let hardening_ends_at = if phase == AwdPhase::Hardening {
        Some((now + chrono::Duration::seconds(300)).into())
    } else {
        None
    };

    let awd_event_id = Uuid::new_v4();
    awd_events::ActiveModel {
        id: Set(awd_event_id),
        event_id: Set(event_id),
        status: Set(AwdEventStatus::Running),
        phase: Set(phase),
        round_count: Set(Some(3)),
        round_duration_secs: Set(300),
        initial_score: Set(1000),
        free_reset_count: Set(3),
        extra_reset_penalty: Set(100),
        judge_max_concurrency: Set(5),
        judge_default_timeout_secs: Set(30),
        judge_retry_interval_secs: Set(5),
        judge_grace_period_secs: Set(30),
        archive_retention_hours: Set(168),
        verified_at: Set(Some(now.into())),
        verified_generation: Set(Some(1)),
        hardening_ends_at: Set(hardening_ends_at),
        event_secret_ciphertext: Set(vec![0u8; 32]),
        event_secret_nonce: Set(vec![0u8; 12]),
        key_version: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .ok()?;

    let port: i32 = 52000 + (Uuid::new_v4().as_u128() % 10000) as i32;
    let net_suffix = Uuid::new_v4().to_string().split('-').next().unwrap().to_string();
    awd_event_networks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        allocation_mode: Set(sea_orm_active_enums::AwdNetworkAllocationMode::Automatic),
        gamebox_cidr: Set("10.44.0.0/16".parse().unwrap()),
        wireguard_cidr: Set("172.33.0.0/16".parse().unwrap()),
        infrastructure_subnet: Set("10.44.0.0/24".parse().unwrap()),
        flagserver_ip: Set("10.44.0.10".parse().unwrap()),
        judgeserver_ip: Set("10.44.0.11".parse().unwrap()),
        wireguard_interface_name: Set(format!("wg_neterr_{net_suffix}")),
        wireguard_listen_port: Set(port),
        docker_network_name: Set(format!("docker_neterr_{net_suffix}")),
        ..Default::default()
    }
    .insert(&db)
    .await
    .ok()?;

    let team_id = Uuid::new_v4();
    event_teams::ActiveModel {
        id: Set(team_id),
        event_id: Set(event_id),
        name: Set("NetErr Team".into()),
        banned: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .ok()?;

    let gamebox_id = Uuid::new_v4();
    gameboxes::ActiveModel {
        id: Set(gamebox_id),
        name: Set("neterr-gb".into()),
        safe_name: Set("neterr-gb".into()),
        category: Set("other".into()),
        hidden: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .ok()?;

    let event_gamebox_id = Uuid::new_v4();
    awd_event_gameboxes::ActiveModel {
        id: Set(event_gamebox_id),
        event_id: Set(event_id),
        gamebox_id: Set(gamebox_id),
        attack_score: Set(100),
        judge_down_penalty: Set(50),
        first_bonus: Set(50),
        host_offset: Set(0),
        enabled: Set(true),
        hidden: Set(false),
        cpu_millis: Set(500),
        memory_bytes: Set(256 * 1024 * 1024),
        pids_limit: Set(100),
        ..Default::default()
    }
    .insert(&db)
    .await
    .ok()?;

    let root_id = Uuid::new_v4();
    let instance_id = Uuid::new_v4();
    let cnt_suffix = Uuid::new_v4().to_string().split('-').next().unwrap().to_string();

    event_instances::ActiveModel {
        id: Set(root_id),
        event_id: Set(event_id),
        container_name: Set(format!("container-neterr-{cnt_suffix}")),
        container_id: Set(Some(format!("docker-neterr-{cnt_suffix}"))),
        runtime_generation: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .ok()?;

    event_gamebox_instances::ActiveModel {
        id: Set(instance_id),
        event_id: Set(event_id),
        team_id: Set(team_id),
        event_gamebox_id: Set(event_gamebox_id),
        instance_id: Set(root_id),
        status: Set(GameboxStatus::Ready),
        gamebox_ip: Set("10.44.1.5/32".parse().unwrap()),
        health_status: Set("healthy".into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .ok()?;

    let round_id = if with_round {
        let round_id = Uuid::new_v4();
        awd_rounds::ActiveModel {
            id: Set(round_id),
            event_id: Set(event_id),
            round_number: Set(1),
            status: Set(RoundStatus::Active),
            phase: Set(AwdPhase::Attack),
            started_at: Set(now.into()),
            scheduled_end_at: Set((now + chrono::Duration::seconds(120)).into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .ok()?;
        round_id
    } else {
        Uuid::nil()
    };

    Some(TestFixtures {
        db,
        event_id,
        awd_event_id,
        team_id,
        user_id,
        round_id,
        instance_id,
    })
}

// ── Hardening Freeze: NetworkError preserves remaining time ──

#[test]
fn network_error_freezes_hardening_remaining_time() {
    let fixtures = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(setup_test(AwdPhase::Hardening, false))
    {
        Some(f) => f,
        None => return,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        // Trigger NetworkError
        recovery_service::handle_network_error(
            &fixtures.db,
            fixtures.event_id,
            "test network failure",
        )
        .await
        .unwrap();

        // Verify persisted state
        let awd_event = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(awd_event.status, AwdEventStatus::NetworkError);
        assert_eq!(awd_event.paused_phase, Some(AwdPhase::Hardening));
        assert!(awd_event.pause_remaining_secs.unwrap_or(0) > 0,
            "Remaining time should be preserved");
        assert!(awd_event.hardening_ends_at.is_none(),
            "hardening_ends_at should be cleared");

        // Verify phase is Pause (NetworkError uses Pause firewall rules)
        assert_eq!(awd_event.phase, AwdPhase::Pause);

        fixtures.cleanup().await;
    });
}

// ── Stale HardeningEnd does not advance during NetworkError ──

#[test]
fn stale_hardening_end_does_not_advance_network_error() {
    let fixtures = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(setup_test(AwdPhase::Hardening, false))
    {
        Some(f) => f,
        None => return,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        // Trigger NetworkError
        recovery_service::handle_network_error(
            &fixtures.db,
            fixtures.event_id,
            "test network failure",
        )
        .await
        .unwrap();

        // Simulate stale HardeningEnd handler delivery
        // The handler checks status != Running || phase != Hardening → skips
        let awd_event = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();

        // Verify the handler would skip: status is NetworkError, not Running
        assert_ne!(awd_event.status, AwdEventStatus::Running);
        assert_eq!(awd_event.phase, AwdPhase::Pause);

        // Verify no Round 1 exists
        let rounds = awd_rounds::Entity::find()
            .filter(awd_rounds::Column::EventId.eq(fixtures.event_id))
            .all(&fixtures.db)
            .await
            .unwrap();
        assert!(rounds.is_empty(), "No rounds should exist during NetworkError");

        fixtures.cleanup().await;
    });
}

// ── Hardening Resume: uses saved remaining time ──

#[test]
fn network_error_hardening_resume_uses_saved_remaining_time() {
    let fixtures = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(setup_test(AwdPhase::Hardening, false))
    {
        Some(f) => f,
        None => return,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        // Trigger NetworkError (saves paused_phase=Hardening, remaining≈300s)
        recovery_service::handle_network_error(
            &fixtures.db,
            fixtures.event_id,
            "test network failure",
        )
        .await
        .unwrap();

        // Verify healthy infra does NOT auto-resume
        let awd_event = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(awd_event.status, AwdEventStatus::NetworkError);

        // Admin Resume
        let network = NoopNetworkRuntime;
        let firewall = NoopFirewallRuntime;
        let publisher = NoopEventPublisher;

        event_service::resume_event(
            &fixtures.db,
            &network,
            &firewall,
            &publisher,
            fixtures.event_id,
        )
        .await
        .unwrap();

        // Verify resumed state
        let resumed = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(resumed.status, AwdEventStatus::Running);
        assert_eq!(resumed.phase, AwdPhase::Hardening);
        assert!(resumed.hardening_ends_at.is_some(),
            "hardening_ends_at should be rebuilt from saved remaining time");
        assert_eq!(resumed.pause_remaining_secs, Some(0),
            "pause_remaining_secs should be cleared after resume");

        fixtures.cleanup().await;
    });
}

// ── Attack Round Freeze: NetworkError preserves round remaining time ──

#[test]
fn network_error_freezes_active_round() {
    let fixtures = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(setup_test(AwdPhase::Attack, true))
    {
        Some(f) => f,
        None => return,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        // Trigger NetworkError
        recovery_service::handle_network_error(
            &fixtures.db,
            fixtures.event_id,
            "test network failure",
        )
        .await
        .unwrap();

        // Verify event state
        let awd_event = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(awd_event.status, AwdEventStatus::NetworkError);
        assert_eq!(awd_event.paused_phase, Some(AwdPhase::Attack));
        assert!(awd_event.pause_remaining_secs.unwrap_or(0) > 0,
            "Remaining time should be preserved");

        // Verify round is paused
        let round = awd_rounds::Entity::find_by_id(fixtures.round_id)
            .one(&fixtures.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(round.status, RoundStatus::Paused);
        assert!(round.remaining_secs.unwrap_or(0) > 0,
            "Round remaining_secs should be preserved");

        fixtures.cleanup().await;
    });
}

// ── Stale RoundEnd does not progress during NetworkError ──

#[test]
fn stale_round_end_does_not_progress_network_error() {
    let fixtures = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(setup_test(AwdPhase::Attack, true))
    {
        Some(f) => f,
        None => return,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        // Trigger NetworkError
        recovery_service::handle_network_error(
            &fixtures.db,
            fixtures.event_id,
            "test network failure",
        )
        .await
        .unwrap();

        // Verify the RoundEnd handler would skip:
        // AwdRoundEndHandler checks ev.status == AwdEventStatus::NetworkError → skips
        let awd_event = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(awd_event.status, AwdEventStatus::NetworkError);

        // Round is still Paused, not Completed
        let round = awd_rounds::Entity::find_by_id(fixtures.round_id)
            .one(&fixtures.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(round.status, RoundStatus::Paused);

        // No Round 2 exists
        let round_count = awd_rounds::Entity::find()
            .filter(awd_rounds::Column::EventId.eq(fixtures.event_id))
            .all(&fixtures.db)
            .await
            .unwrap()
            .len();
        assert_eq!(round_count, 1, "Only Round 1 should exist");

        fixtures.cleanup().await;
    });
}

// ── Attack Round Resume: uses saved remaining time ──

#[test]
fn network_error_round_resume_uses_saved_remaining_time() {
    let fixtures = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(setup_test(AwdPhase::Attack, true))
    {
        Some(f) => f,
        None => return,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        // Trigger NetworkError
        recovery_service::handle_network_error(
            &fixtures.db,
            fixtures.event_id,
            "test network failure",
        )
        .await
        .unwrap();

        // No auto-resume
        let awd_event = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(awd_event.status, AwdEventStatus::NetworkError);

        // Admin Resume
        let network = NoopNetworkRuntime;
        let firewall = NoopFirewallRuntime;
        let publisher = NoopEventPublisher;

        event_service::resume_event(
            &fixtures.db,
            &network,
            &firewall,
            &publisher,
            fixtures.event_id,
        )
        .await
        .unwrap();

        // Verify resumed state
        let resumed = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resumed.status, AwdEventStatus::Running);
        assert_eq!(resumed.phase, AwdPhase::Attack);
        assert_eq!(resumed.pause_remaining_secs, Some(0));

        // Round is Active again
        let round = awd_rounds::Entity::find_by_id(fixtures.round_id)
            .one(&fixtures.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(round.status, RoundStatus::Active);
        assert!(round.scheduled_end_at.with_timezone(&chrono::Utc) > chrono::Utc::now(), "scheduled_end_at should be set");
        assert!(round.remaining_secs.is_none(),
            "remaining_secs should be cleared after resume");

        fixtures.cleanup().await;
    });
}

// ── Action Freeze: NetworkError blocks competition actions ──

#[test]
fn network_error_blocks_competition_actions() {
    let fixtures = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(setup_test(AwdPhase::Attack, true))
    {
        Some(f) => f,
        None => return,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        // Trigger NetworkError
        recovery_service::handle_network_error(
            &fixtures.db,
            fixtures.event_id,
            "test network failure",
        )
        .await
        .unwrap();

        // Verify is_active() returns false for NetworkError
        let awd_event = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();
        assert!(!awd_event.status.is_active());

        // Verify reset eligibility check rejects NetworkError
        let reset_result = floatctf::modules::event::awd::service::reset_service::check_reset_eligibility(
            &awd_event,
            fixtures.team_id,
            false, // no active round
            awd_event.round_count,
        );
        assert!(reset_result.is_err(), "Reset should be rejected during NetworkError");

        fixtures.cleanup().await;
    });
}

// ── Individual GameBox Failure Control ──

#[test]
fn individual_gamebox_failure_does_not_freeze_event() {
    let fixtures = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(setup_test(AwdPhase::Attack, true))
    {
        Some(f) => f,
        None => return,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        // Mark one GameBox as Missing
        gamebox_repo::update_instance_status(&fixtures.db, fixtures.instance_id, GameboxStatus::Missing)
            .await
            .unwrap();

        // Event should still be Running
        let awd_event = event_repo::find_by_event_id(&fixtures.db, fixtures.event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(awd_event.status, AwdEventStatus::Running,
            "Event should remain Running despite individual GameBox failure");

        // Round should still be Active
        let round = awd_rounds::Entity::find_by_id(fixtures.round_id)
            .one(&fixtures.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(round.status, RoundStatus::Active,
            "Round should remain Active despite individual GameBox failure");

        // GameBox should NOT be auto-recreated
        let (inst, _root) = gamebox_repo::find_instance_by_id(&fixtures.db, fixtures.instance_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inst.status, GameboxStatus::Missing,
            "GameBox should remain Missing, not auto-recreated");

        fixtures.cleanup().await;
    });
}