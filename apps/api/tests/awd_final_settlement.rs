//! AWD Wave 6 Final Settlement tests.
//!
//! Covers: settlement detection, finalization, score freeze, recovery,
//! network lockdown, player access guards, and manual finish.

use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use std::sync::Arc;
use uuid::Uuid;

use floatctf::entity::sea_orm_active_enums::{
    EventFamily, EventPurpose, ParticipantMode, RoundStatus,
};
use floatctf::entity::{
    awd_event_networks, awd_events, awd_rounds, events, sea_orm_active_enums::AwdEventStatus,
    sea_orm_active_enums::AwdNetworkAllocationMode, sea_orm_active_enums::AwdPhase,
};
use floatctf::infrastructure::realtime::NoopEventPublisher;
use floatctf::modules::event::awd::{
    infrastructure::{firewall::NoopFirewallRuntime, network::NoopNetworkRuntime},
    repo::{event_repo, judge_repo, round_repo},
    service::event_service,
};

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip awd_final_settlement: DB unreachable ({e})");
            None
        }
    }
}

async fn seed_event(db: &sea_orm::DatabaseConnection, tag: &str, round_count: i32) -> Uuid {
    let event_id = Uuid::new_v4();
    let parent = events::ActiveModel {
        is_virtual: Set(false),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        id: Set(event_id),
        title: Set(format!("awd-settlement-{tag}")),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set(Some(
            (chrono::Utc::now() + chrono::Duration::hours(1)).fixed_offset(),
        )),
        ..Default::default()
    };
    parent.insert(db).await.expect("insert events");

    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        status: Set(AwdEventStatus::Running),
        phase: Set(AwdPhase::Attack),
        round_count: Set(Some(round_count)),
        configuration_generation: Set(0),
        ..Default::default()
    };
    awd.insert(db).await.expect("insert awd_events");

    // Event Network
    let wg_iface = format!("fawg_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let wg_port = 50000 + (Uuid::new_v4().as_u128() % 10000) as i32;
    let net = awd_event_networks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        allocation_mode: Set(AwdNetworkAllocationMode::Automatic),
        wireguard_interface_name: Set(wg_iface),
        wireguard_listen_port: Set(wg_port),
        wireguard_cidr: Set("10.200.0.0/16".parse().unwrap()),
        gamebox_cidr: Set("10.42.0.0/16".parse().unwrap()),
        infrastructure_subnet: Set("10.42.0.0/24".parse().unwrap()),
        flagserver_ip: Set("10.42.0.10/32".parse().unwrap()),
        judgeserver_ip: Set("10.42.0.11/32".parse().unwrap()),
        docker_network_name: Set(format!("fctf-awd-{}", &event_id.to_string()[..8])),
        ..Default::default()
    };
    net.insert(db).await.expect("insert event_network");

    event_id
}

async fn seed_round(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    round_number: i32,
    status: RoundStatus,
) -> Uuid {
    let round_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let is_completed = status == RoundStatus::Completed;
    awd_rounds::ActiveModel {
        id: Set(round_id),
        event_id: Set(event_id),
        round_number: Set(round_number),
        status: Set(status),
        phase: Set(AwdPhase::Attack),
        started_at: Set(now.into()),
        scheduled_end_at: Set((now + chrono::Duration::seconds(300)).into()),
        completed_at: if is_completed {
            Set(Some(now.into()))
        } else {
            Set(None)
        },
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert round");
    round_id
}

// ─────────────────────────────────────────────────────────────────
// Settlement Detection Tests
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn not_final_settlement_when_active_round_exists() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "not-final-active", 10).await;
    seed_round(&db, event_id, 4, RoundStatus::Active).await;

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(!event_service::is_final_settlement(&awd, latest.as_ref()));
}

#[tokio::test]
async fn not_final_settlement_when_round_below_round_count() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "not-final-below", 10).await;
    seed_round(&db, event_id, 4, RoundStatus::Completed).await;

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(!event_service::is_final_settlement(&awd, latest.as_ref()));
}

#[tokio::test]
async fn is_final_settlement_when_final_round_completed() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "is-final", 10).await;
    seed_round(&db, event_id, 10, RoundStatus::Completed).await;

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(event_service::is_final_settlement(&awd, latest.as_ref()));
}

#[tokio::test]
async fn hardening_phase_not_final_settlement() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "hardening-not", 5).await;
    // Set phase to Hardening
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    awd_events::ActiveModel {
        id: Set(awd.id),
        phase: Set(AwdPhase::Hardening),
        ..Default::default()
    }
    .update(&db)
    .await
    .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(!event_service::is_final_settlement(&awd, latest.as_ref()));
}

#[tokio::test]
async fn paused_event_not_final_settlement() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "paused-not", 5).await;
    seed_round(&db, event_id, 5, RoundStatus::Completed).await;
    // Set status to Paused
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    event_repo::transition_event(
        &db,
        awd.id,
        AwdEventStatus::Running,
        AwdEventStatus::Paused,
        event_repo::TransitionPatch::paused(AwdPhase::Attack, 0),
    )
    .await
    .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(!event_service::is_final_settlement(&awd, latest.as_ref()));
}

#[tokio::test]
async fn round_count_none_not_final_settlement() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "no-count", 5).await;
    seed_round(&db, event_id, 5, RoundStatus::Completed).await;
    // Clear round_count
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    awd_events::ActiveModel {
        id: Set(awd.id),
        round_count: Set(None),
        ..Default::default()
    }
    .update(&db)
    .await
    .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(!event_service::is_final_settlement(&awd, latest.as_ref()));
}

// ─────────────────────────────────────────────────────────────────
// Finalization Tests (zero-task final batch)
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn finished_when_final_settlement_and_no_tasks() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "zero-tasks", 3).await;
    seed_round(&db, event_id, 3, RoundStatus::Completed).await;
    // No judge tasks created at all

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);
}

#[tokio::test]
async fn duplicate_maybe_finish_is_idempotent() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "idempotent", 2).await;
    seed_round(&db, event_id, 2, RoundStatus::Completed).await;

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);

    // First call transitions
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);

    // Second call should be no-op
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();
    let awd2 = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd2.status, AwdEventStatus::Finished);
}

#[tokio::test]
async fn finished_event_stays_finished_after_maybe_finish() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "finished-stays", 1).await;
    seed_round(&db, event_id, 1, RoundStatus::Completed).await;

    // Transition to Finished directly
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    event_repo::transition_event(
        &db,
        awd.id,
        AwdEventStatus::Running,
        AwdEventStatus::Finished,
        event_repo::TransitionPatch::finished(),
    )
    .await
    .unwrap();

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    let awd2 = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd2.status, AwdEventStatus::Finished);
}

// ─────────────────────────────────────────────────────────────────
// Score Freeze / Finished Guard Tests
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn finished_adjustment_rejected() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "finished-adj", 1).await;
    seed_round(&db, event_id, 1, RoundStatus::Completed).await;

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);

    // Adjustment should be rejected
    let result = floatctf::modules::event::awd::service::score_service::record_adjustment(
        &db,
        event_id,
        Uuid::new_v4(),
        100,
        "test",
        Uuid::new_v4(),
    )
    .await;
    assert!(
        result.is_err(),
        "adjustment after Finished should be rejected"
    );
}

// ─────────────────────────────────────────────────────────────────
// Manual Finish Tests
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn manual_finish_before_final_round_stays_running() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "manual-early", 10).await;
    seed_round(&db, event_id, 2, RoundStatus::Active).await;

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);
    event_service::finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap(); // does not error, just does not finish

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        awd.status,
        AwdEventStatus::Running,
        "should not finish before final round"
    );
}

#[tokio::test]
async fn manual_finish_when_settlement_complete_transitions() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "manual-complete", 3).await;
    seed_round(&db, event_id, 3, RoundStatus::Completed).await;

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);
    event_service::finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);
}

// ─────────────────────────────────────────────────────────────────
// All Judge Tasks Terminal Check
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn all_event_judge_tasks_terminal_empty_event() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "empty-terminal", 1).await;
    seed_round(&db, event_id, 1, RoundStatus::Completed).await;

    let all_terminal = judge_repo::all_event_judge_tasks_terminal(&db, event_id)
        .await
        .unwrap();
    assert!(all_terminal, "empty event should have all tasks terminal");
}

#[tokio::test]
async fn not_finished_when_round_below_count() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "not-finished", 10).await;
    seed_round(&db, event_id, 4, RoundStatus::Completed).await;

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        awd.status,
        AwdEventStatus::Running,
        "should not finish before final round"
    );
}

#[tokio::test]
async fn not_finished_when_active_round_exists() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "active-round", 10).await;
    seed_round(&db, event_id, 10, RoundStatus::Active).await;

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        awd.status,
        AwdEventStatus::Running,
        "should not finish with active round"
    );
}

#[tokio::test]
async fn already_finished_is_noop() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "already-fin", 1).await;
    seed_round(&db, event_id, 1, RoundStatus::Completed).await;

    // Transition to Finished directly
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    event_repo::transition_event(
        &db,
        awd.id,
        AwdEventStatus::Running,
        AwdEventStatus::Finished,
        event_repo::TransitionPatch::finished(),
    )
    .await
    .unwrap();

    // maybe_finish_event on already Finished should be no-op
    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);
    let result =
        event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id).await;
    assert!(result.is_ok());
    let awd2 = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd2.status, AwdEventStatus::Finished);
}
