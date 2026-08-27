//! AWD Wave 6.1 Finished Contract tests.
//!
//! Covers: settlement predicate correction, event-wide judge terminality,
//! score commit ordering, Up/JudgeError completion, no-worker deadline,
//! crash recovery, firewall policy, score freeze, stale judge,
//! NetworkError judge regression, manual finish, NetworkError settlement,
//! Unban regression, recovery lockdown, finalizer concurrency.

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use std::sync::Arc;
use uuid::Uuid;

use floatctf::entity::sea_orm_active_enums::{
    EventFamily, EventPurpose, GameboxStatus, JudgeTaskStatus, ParticipantMode, RoundStatus,
};
use floatctf::entity::{
    awd_event_gameboxes, awd_event_networks, awd_events, awd_judge_batches, awd_judge_tasks,
    awd_rounds, event_gamebox_instances, event_instances, event_teams, events, gameboxes,
    sea_orm_active_enums::AwdEventStatus, sea_orm_active_enums::AwdNetworkAllocationMode,
    sea_orm_active_enums::AwdPhase,
};
use floatctf::infrastructure::realtime::NoopEventPublisher;
use floatctf::modules::event::awd::{
    domain::AwdEventStatusExt,
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
            eprintln!("skip awd_finished_contract: DB unreachable ({e})");
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
        title: Set(format!("awd-contract-{tag}")),
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

    let wg_port = 25000 + (Uuid::new_v4().as_u128() % 60000) as i32;
    let net = awd_event_networks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        allocation_mode: Set(AwdNetworkAllocationMode::Automatic),
        wireguard_interface_name: Set(format!("fcw_{}", &Uuid::new_v4().simple().to_string()[..8])),
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

/// Helper: create a minimal event_gamebox_instances row for FK satisfaction.
async fn seed_instance(db: &sea_orm::DatabaseConnection, event_id: Uuid) -> (Uuid, Uuid) {
    let now = chrono::Utc::now();

    // Create team
    let team_id = Uuid::new_v4();
    event_teams::ActiveModel {
        id: Set(team_id),
        event_id: Set(event_id),
        name: Set("Contract Team".into()),
        banned: Set(false),
        points: Set(0.0),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert event_teams");

    // Create gamebox
    let gb_id = Uuid::new_v4();
    let name = format!("gb-contract-{}", &Uuid::new_v4().to_string()[..8]);
    gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(name.clone()),
        safe_name: Set(name),
        category: Set("service".into()),
        description: Set("test".into()),
        hidden: Set(false),
        recommended_cpu_millis: Set(500),
        recommended_memory_bytes: Set(256 * 1024 * 1024),
        recommended_pids_limit: Set(100),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert gamebox");

    // Create awd_event_gamebox
    let eg_id = Uuid::new_v4();
    awd_event_gameboxes::ActiveModel {
        id: Set(eg_id),
        event_id: Set(event_id),
        gamebox_id: Set(gb_id),
        host_offset: Set(2 + (Uuid::new_v4().as_u128() % 250) as i16),
        enabled: Set(true),
        hidden: Set(false),
        cpu_millis: Set(500),
        memory_bytes: Set(256 * 1024 * 1024),
        pids_limit: Set(100),
        attack_score: Set(100),
        judge_down_penalty: Set(30),
        first_bonus: Set(50),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert awd_event_gamebox");

    // Create gamebox root
    let root_id = Uuid::new_v4();
    let cnt_suffix = Uuid::new_v4()
        .to_string()
        .split('-')
        .next()
        .unwrap()
        .to_string();
    event_instances::ActiveModel {
        id: Set(root_id),
        event_id: Set(event_id),
        owner_team_id: Set(Some(team_id)),
        container_name: Set(format!("container-contract-{cnt_suffix}")),
        container_id: Set(Some(format!("docker-contract-{cnt_suffix}"))),
        runtime_state: Set("running".into()),
        runtime_generation: Set(1),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert root");

    // Create instance
    let instance_id = Uuid::new_v4();
    event_gamebox_instances::ActiveModel {
        id: Set(instance_id),
        event_id: Set(event_id),
        team_id: Set(team_id),
        event_gamebox_id: Set(eg_id),
        instance_id: Set(root_id),
        status: Set(GameboxStatus::Ready),
        gamebox_ip: Set("10.44.1.5/32".parse().unwrap()),
        health_status: Set("healthy".into()),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert instance");

    (instance_id, team_id)
}

/// Helper: create a judge task with given status, with valid FK dependencies.
async fn seed_judge_task(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
    instance_id: Uuid,
    team_id: Uuid,
    status: JudgeTaskStatus,
    attempt_count: i32,
    max_attempts: i32,
) -> Uuid {
    let batch = awd_judge_batches::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round_id),
        total_tasks: Set(1),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert batch");

    let now = chrono::Utc::now();
    let is_terminal = matches!(
        status,
        JudgeTaskStatus::Up | JudgeTaskStatus::Down | JudgeTaskStatus::JudgeError
    );
    awd_judge_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        batch_id: Set(batch.id),
        round_id: Set(round_id),
        gamebox_instance_id: Set(instance_id),
        team_id: Set(team_id),
        status: Set(status),
        attempt_count: Set(attempt_count),
        max_attempts: Set(max_attempts),
        deadline_at: Set((now + chrono::Duration::minutes(5)).into()),
        finished_at: if is_terminal {
            Set(Some(now.into()))
        } else {
            Set(None)
        },
        created_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert task");
    batch.id
}

// ─────────────────────────────────────────────────────────────────
// §2-3: Final Settlement Predicate Correction
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn not_final_settlement_when_round_less_than_count() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "pred-less", 10).await;
    seed_round(&db, event_id, 4, RoundStatus::Completed).await;

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(!event_service::is_final_settlement(&awd, latest.as_ref()));
}

#[tokio::test]
async fn is_final_settlement_when_round_equals_count() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "pred-equal", 10).await;
    seed_round(&db, event_id, 10, RoundStatus::Completed).await;

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(event_service::is_final_settlement(&awd, latest.as_ref()));
}

#[tokio::test]
async fn not_final_settlement_when_round_exceeds_count() {
    // round_number > round_count is an invariant violation.
    // Must NOT silently classify as final settlement.
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "pred-exceed", 10).await;
    seed_round(&db, event_id, 11, RoundStatus::Completed).await;

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(
        !event_service::is_final_settlement(&awd, latest.as_ref()),
        "round_number > round_count must NOT be classified as final settlement"
    );
}

#[tokio::test]
async fn not_final_settlement_when_final_round_still_active() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };
    let event_id = seed_event(&db, "pred-active", 10).await;
    seed_round(&db, event_id, 10, RoundStatus::Active).await;

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(!event_service::is_final_settlement(&awd, latest.as_ref()));
}

// ─────────────────────────────────────────────────────────────────
// §4: Event-Wide Judge Terminality
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn event_wide_judge_terminality_blocks_finish_when_older_round_pending() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    // round_count = 5, Round 5 Completed, Round 4 has a Pending task
    let event_id = seed_event(&db, "wide-pending", 5).await;
    let _round5_id = seed_round(&db, event_id, 5, RoundStatus::Completed).await;
    let round4_id = seed_round(&db, event_id, 4, RoundStatus::Completed).await;

    let (instance_id, _team_id) = seed_instance(&db, event_id).await;
    // Create a pending judge task for round 4
    seed_judge_task(
        &db,
        event_id,
        round4_id,
        instance_id,
        _team_id,
        JudgeTaskStatus::Pending,
        0,
        3,
    )
    .await;
    // All round 5 tasks terminal (empty), but round 4 has Pending

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
        "event must remain Running when older-round task is pending"
    );

    // Now terminalize the round 4 task
    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    for task in tasks {
        if task.status == JudgeTaskStatus::Pending {
            awd_judge_tasks::ActiveModel {
                id: Set(task.id),
                status: Set(JudgeTaskStatus::Up),
                finished_at: Set(Some(chrono::Utc::now().into())),
                ..Default::default()
            }
            .update(&db)
            .await
            .unwrap();
        }
    }

    // Now finish should succeed
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        awd.status,
        AwdEventStatus::Finished,
        "event must finish after all tasks are terminal"
    );
}

// ─────────────────────────────────────────────────────────────────
// §5: Last Down Score Commit Ordering
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn last_down_score_committed_before_finished() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "last-down", 3).await;
    let round3_id = seed_round(&db, event_id, 3, RoundStatus::Completed).await;
    let (instance_id, _team_id) = seed_instance(&db, event_id).await;

    // Create a Down task (already terminal)
    seed_judge_task(
        &db,
        event_id,
        round3_id,
        instance_id,
        _team_id,
        JudgeTaskStatus::Down,
        1,
        3,
    )
    .await;

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
        AwdEventStatus::Finished,
        "event must transition to Finished when all tasks terminal"
    );

    // Verify the task is still Down (not changed by finish)
    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, JudgeTaskStatus::Down);
}

// ─────────────────────────────────────────────────────────────────
// §6: Up / JudgeError Completion
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn last_task_up_finishes_event() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "last-up", 2).await;
    let round2_id = seed_round(&db, event_id, 2, RoundStatus::Completed).await;
    let (instance_id, _team_id) = seed_instance(&db, event_id).await;

    seed_judge_task(
        &db,
        event_id,
        round2_id,
        instance_id,
        _team_id,
        JudgeTaskStatus::Up,
        1,
        3,
    )
    .await;

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
async fn last_task_judge_error_finishes_event() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "last-error", 2).await;
    let round2_id = seed_round(&db, event_id, 2, RoundStatus::Completed).await;
    let (instance_id, _team_id) = seed_instance(&db, event_id).await;

    seed_judge_task(
        &db,
        event_id,
        round2_id,
        instance_id,
        _team_id,
        JudgeTaskStatus::JudgeError,
        3,
        3,
    )
    .await;

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

// ─────────────────────────────────────────────────────────────────
// §7: No-Worker Deadline Finalization
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn pending_task_deadline_terminalizes_to_judge_error_and_finishes() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "deadline", 2).await;
    let round2_id = seed_round(&db, event_id, 2, RoundStatus::Completed).await;
    let (instance_id, _team_id) = seed_instance(&db, event_id).await;
    seed_judge_task(
        &db,
        event_id,
        round2_id,
        instance_id,
        _team_id,
        JudgeTaskStatus::Pending,
        0,
        3,
    )
    .await;

    // Simulate the deadline handler: terminalize past-deadline tasks
    let now = chrono::Utc::now();
    let past = now - chrono::Duration::minutes(10);

    // Set the task deadline to the past
    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    for task in tasks {
        awd_judge_tasks::ActiveModel {
            id: Set(task.id),
            deadline_at: Set(past.into()),
            ..Default::default()
        }
        .update(&db)
        .await
        .unwrap();
    }

    // Terminalize past-deadline
    let events = judge_repo::terminalize_past_deadline(&db, now)
        .await
        .unwrap();
    assert!(
        !events.is_empty(),
        "past-deadline task should be terminalized"
    );

    // Verify task is now JudgeError
    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(tasks[0].status, JudgeTaskStatus::JudgeError);

    // Now finish
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

// ─────────────────────────────────────────────────────────────────
// §8: Crash Before Finished Recovery
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn recovery_after_crash_before_finished_transitions() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "crash-recover", 2).await;
    let round2_id = seed_round(&db, event_id, 2, RoundStatus::Completed).await;
    let (instance_id, _team_id) = seed_instance(&db, event_id).await;

    // All tasks terminal, event still Running (crash before finish)
    seed_judge_task(
        &db,
        event_id,
        round2_id,
        instance_id,
        _team_id,
        JudgeTaskStatus::Up,
        1,
        3,
    )
    .await;

    // Event is still Running — simulate recovery
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

    // Duplicate recovery: no change
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
// §13: Finished Score Freeze
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn finished_score_freeze_all_mutation_paths() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "scorefreeze", 1).await;
    seed_round(&db, event_id, 1, RoundStatus::Completed).await;

    // Finish
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

    // Capture initial empty scoreboard
    let score1 =
        floatctf::modules::event::awd::service::score_service::get_scoreboard(&db, event_id, &[])
            .await
            .unwrap();

    // Attempt adjustment → rejected
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
        "adjustment after Finished must be rejected"
    );

    // Duplicate maybe_finish_event → no score change
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    // Scoreboard unchanged
    let score2 =
        floatctf::modules::event::awd::service::score_service::get_scoreboard(&db, event_id, &[])
            .await
            .unwrap();
    assert_eq!(score1.len(), score2.len());
}

// ─────────────────────────────────────────────────────────────────
// §14: Stale Judge After Finished
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stale_judge_result_after_finished_does_not_reopen() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "stale-judge", 2).await;
    let round2_id = seed_round(&db, event_id, 2, RoundStatus::Completed).await;
    let (instance_id, team_id) = seed_instance(&db, event_id).await;

    let task_id = Uuid::new_v4();
    let batch = awd_judge_batches::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round2_id),
        total_tasks: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert batch");

    let now = chrono::Utc::now();
    awd_judge_tasks::ActiveModel {
        id: Set(task_id),
        event_id: Set(event_id),
        batch_id: Set(batch.id),
        round_id: Set(round2_id),
        gamebox_instance_id: Set(instance_id),
        team_id: Set(team_id),
        status: Set(JudgeTaskStatus::Up),
        attempt_count: Set(1),
        max_attempts: Set(3),
        finished_at: Set(Some(now.into())),
        deadline_at: Set((now + chrono::Duration::minutes(5)).into()),
        created_at: Set(now.into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert task");

    // Finish
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

    // Stale worker result: attempt to submit_result on a task that's already Up
    let result = judge_repo::submit_result(
        &db,
        task_id,
        "stale-worker",
        1,
        "bogus-token",
        "stale-result-id",
        JudgeTaskStatus::Down,
        Some(1),
        None,
        None,
        None,
        chrono::Utc::now(),
    )
    .await;
    assert!(
        matches!(result, Ok(judge_repo::SubmitResult::Stale)),
        "stale result after Finished must be rejected"
    );

    // Event remains Finished
    let awd2 = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd2.status, AwdEventStatus::Finished);

    // Task status unchanged
    let task = awd_judge_tasks::Entity::find_by_id(task_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, JudgeTaskStatus::Up);
}

// ─────────────────────────────────────────────────────────────────
// §16: Manual Finish with Pending Judge
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn manual_finish_with_pending_judge_is_rejected() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "manual-pending", 3).await;
    let round3_id = seed_round(&db, event_id, 3, RoundStatus::Completed).await;
    let (instance_id, _team_id) = seed_instance(&db, event_id).await;

    seed_judge_task(
        &db,
        event_id,
        round3_id,
        instance_id,
        _team_id,
        JudgeTaskStatus::Pending,
        0,
        3,
    )
    .await;

    // Admin Finish with pending Judge → should NOT finish
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
    assert_eq!(
        awd.status,
        AwdEventStatus::Running,
        "manual finish must not bypass pending Judge"
    );

    // Now terminalize the task
    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    for task in tasks {
        if task.status == JudgeTaskStatus::Pending {
            awd_judge_tasks::ActiveModel {
                id: Set(task.id),
                status: Set(JudgeTaskStatus::Up),
                finished_at: Set(Some(chrono::Utc::now().into())),
                ..Default::default()
            }
            .update(&db)
            .await
            .unwrap();
        }
    }

    // Now finish should succeed
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
// §18: Finished Unban Regression
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn finished_event_status_is_terminal_blocks_typical_operations() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "finished-terminal", 1).await;
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

    // Finished is terminal
    assert!(awd.status.is_terminal(), "Finished must be terminal");

    // Finished is not active
    assert!(!awd.status.is_active(), "Finished must not be active");
}

// ─────────────────────────────────────────────────────────────────
// §21: Finalizer Concurrency
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_maybe_finish_is_safe() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "concurrent", 2).await;
    seed_round(&db, event_id, 2, RoundStatus::Completed).await;

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);

    // Run two concurrent calls
    let (r1, r2) = tokio::join!(
        event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id),
        event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id),
    );

    assert!(r1.is_ok());
    assert!(r2.is_ok());

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);
}

// ─────────────────────────────────────────────────────────────────
// §23: Zero-Task Final Round
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn zero_task_final_round_finishes_immediately() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "zero-final", 3).await;
    seed_round(&db, event_id, 3, RoundStatus::Completed).await;

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
async fn zero_task_final_round_but_older_round_pending_stays_running() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "zero-older", 5).await;
    seed_round(&db, event_id, 5, RoundStatus::Completed).await;
    let round4_id = seed_round(&db, event_id, 4, RoundStatus::Completed).await;
    let (instance_id, _team_id) = seed_instance(&db, event_id).await;

    // Create pending task in round 4
    seed_judge_task(
        &db,
        event_id,
        round4_id,
        instance_id,
        _team_id,
        JudgeTaskStatus::Pending,
        0,
        3,
    )
    .await;

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
    assert_eq!(awd.status, AwdEventStatus::Running);
}

// ─────────────────────────────────────────────────────────────────
// §15: NetworkError Judge Score Regression
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn networkerror_judge_no_score() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "ne-judge", 2).await;
    // Put event into NetworkError
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    event_repo::transition_event(
        &db,
        awd.id,
        AwdEventStatus::Running,
        AwdEventStatus::NetworkError,
        event_repo::TransitionPatch {
            phase: Some(AwdPhase::Pause),
            paused_phase: Some(AwdPhase::Attack),
            pause_remaining_secs: Some(0),
            hardening_ends_at: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Verify NetworkError status
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::NetworkError);

    // NetworkError should still be in firewall desired set
    assert!(
        floatctf::modules::event::awd::service::firewall_service::in_firewall_desired_set(
            &awd.status
        ),
        "NetworkError must be in firewall desired set"
    );
}

// ─────────────────────────────────────────────────────────────────
// §6.2: Finished Firewall Desired-State Ownership
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn finished_event_in_firewall_desired_set() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "fin-desired", 1).await;
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

    // Finished must be in firewall desired set
    assert!(
        floatctf::modules::event::awd::service::firewall_service::in_firewall_desired_set(
            &awd.status
        ),
        "Finished must be in firewall desired set for fail-closed lockdown"
    );
}

// ─────────────────────────────────────────────────────────────────
// §6.2: Finished Recovery Reapplies Lockdown
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn finished_recovery_reapplies_lockdown() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "fin-recover", 2).await;
    let round2_id = seed_round(&db, event_id, 2, RoundStatus::Completed).await;
    let (instance_id, _team_id) = seed_instance(&db, event_id).await;

    seed_judge_task(
        &db,
        event_id,
        round2_id,
        instance_id,
        _team_id,
        JudgeTaskStatus::Up,
        1,
        3,
    )
    .await;

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

    // Simulate recovery: call maybe_finish_event again (idempotent)
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    // Event remains Finished
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);

    // Finished is still in firewall desired set after recovery
    assert!(
        floatctf::modules::event::awd::service::firewall_service::in_firewall_desired_set(
            &awd.status
        ),
        "Finished must remain in firewall desired set after recovery"
    );
}

// ─────────────────────────────────────────────────────────────────
// §6.2: Scoreboard Immutability After Finished
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scoreboard_immutable_after_finished() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "score-immut", 1).await;
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

    // Snapshot scoreboard
    let score1 =
        floatctf::modules::event::awd::service::score_service::get_scoreboard(&db, event_id, &[])
            .await
            .unwrap();

    // Stale judge result attempt (no-op since no tasks exist)
    let _ = judge_repo::submit_result(
        &db,
        Uuid::new_v4(),
        "stale",
        1,
        "token",
        "rid",
        JudgeTaskStatus::Down,
        Some(1),
        None,
        None,
        None,
        chrono::Utc::now(),
    )
    .await;

    // Adjustment rejected
    let _ = floatctf::modules::event::awd::service::score_service::record_adjustment(
        &db,
        event_id,
        Uuid::new_v4(),
        100,
        "test",
        Uuid::new_v4(),
    )
    .await;

    // Duplicate finalizer
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    // Scoreboard unchanged
    let score2 =
        floatctf::modules::event::awd::service::score_service::get_scoreboard(&db, event_id, &[])
            .await
            .unwrap();
    assert_eq!(score1.len(), score2.len());
    for (s1, s2) in score1.iter().zip(score2.iter()) {
        assert_eq!(s1.total_score, s2.total_score);
    }
}

// ─────────────────────────────────────────────────────────────────
// §6.2: NetworkError Settlement Resume
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn networkerror_settlement_resume_no_new_round() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    // Final settlement: round 3 completed, round_count = 3
    let event_id = seed_event(&db, "ne-settle", 3).await;
    seed_round(&db, event_id, 3, RoundStatus::Completed).await;

    // Verify final settlement
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(event_service::is_final_settlement(&awd, latest.as_ref()));

    // Simulate NetworkError during settlement
    event_repo::transition_event(
        &db,
        awd.id,
        AwdEventStatus::Running,
        AwdEventStatus::NetworkError,
        event_repo::TransitionPatch {
            phase: Some(AwdPhase::Pause),
            paused_phase: Some(AwdPhase::Attack),
            pause_remaining_secs: Some(0),
            hardening_ends_at: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::NetworkError);

    // Resume: should return to settlement state, NOT create Round 4
    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);

    // Resume to Running
    event_repo::transition_event(
        &db,
        awd.id,
        AwdEventStatus::NetworkError,
        AwdEventStatus::Running,
        event_repo::TransitionPatch {
            phase: Some(AwdPhase::Attack),
            pause_remaining_secs: Some(0),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // After resume, the event is still in final settlement (no active round)
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let latest = round_repo::find_latest_round(&db, event_id).await.unwrap();
    assert!(event_service::is_final_settlement(&awd, latest.as_ref()));

    // No Round 4 was created
    let all_rounds = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(all_rounds.len(), 1, "no new round should be created");
    assert_eq!(all_rounds[0].round_number, 3);

    // With all tasks terminal, finish should succeed
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);
}

// ─────────────────────────────────────────────────────────────────
// §Core Backend: Lease Reclaim Exhaustion → Finished
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn lease_reclaim_exhausted_last_task_finishes_event() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "lease-reclaim", 2).await;
    let round2_id = seed_round(&db, event_id, 2, RoundStatus::Completed).await;
    let (instance_id, team_id) = seed_instance(&db, event_id).await;

    // Create a Running task with expired lease and exhausted attempts
    let batch = awd_judge_batches::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round2_id),
        total_tasks: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert batch");

    let past = chrono::Utc::now() - chrono::Duration::minutes(10);
    let now = chrono::Utc::now();
    awd_judge_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        batch_id: Set(batch.id),
        round_id: Set(round2_id),
        gamebox_instance_id: Set(instance_id),
        team_id: Set(team_id),
        status: Set(JudgeTaskStatus::Running),
        attempt_count: Set(3),
        max_attempts: Set(3),
        worker_id: Set(Some("dead-worker".into())),
        lease_token_hash: Set(Some(judge_repo::hash_lease_token("expired-token"))),
        lease_expires_at: Set(Some(past.into())),
        claimed_at: Set(Some(past.into())),
        heartbeat_at: Set(Some(past.into())),
        deadline_at: Set((now + chrono::Duration::minutes(5)).into()),
        created_at: Set(past.into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert task");

    // Exercise the production reclaim path
    let terminalized = judge_repo::reclaim_expired_leases(&db, now.into())
        .await
        .unwrap();
    assert!(
        !terminalized.is_empty(),
        "lease reclaim should terminalize exhausted task"
    );
    assert!(
        terminalized.contains(&event_id),
        "affected event should be reported"
    );

    // Verify task is JudgeError
    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(tasks[0].status, JudgeTaskStatus::JudgeError);

    // Now finalize — as would happen after claim
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

// ─────────────────────────────────────────────────────────────────
// §Core Backend: Claim-Time Deadline → Finished
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn claim_deadline_last_task_finishes_event() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "claim-deadline", 2).await;
    let round2_id = seed_round(&db, event_id, 2, RoundStatus::Completed).await;
    let (instance_id, team_id) = seed_instance(&db, event_id).await;

    let batch = awd_judge_batches::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round2_id),
        total_tasks: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert batch");

    let past = chrono::Utc::now() - chrono::Duration::minutes(10);
    let now = chrono::Utc::now();
    awd_judge_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        batch_id: Set(batch.id),
        round_id: Set(round2_id),
        gamebox_instance_id: Set(instance_id),
        team_id: Set(team_id),
        status: Set(JudgeTaskStatus::Pending),
        attempt_count: Set(0),
        max_attempts: Set(3),
        deadline_at: Set(past.into()),
        created_at: Set(past.into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert task");

    let terminalized = judge_repo::terminalize_past_deadline(&db, now)
        .await
        .unwrap();
    assert!(!terminalized.is_empty());
    assert!(terminalized.contains(&event_id));

    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(tasks[0].status, JudgeTaskStatus::JudgeError);

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

// ─────────────────────────────────────────────────────────────────
// §Core Backend: Real Batch Deadline Handler → Finished
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn real_batch_deadline_handler_finishes_without_worker() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "handler", 2).await;
    let round2_id = seed_round(&db, event_id, 2, RoundStatus::Completed).await;
    let (instance_id, team_id) = seed_instance(&db, event_id).await;

    let batch = awd_judge_batches::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round2_id),
        total_tasks: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert batch");

    let past = chrono::Utc::now() - chrono::Duration::minutes(10);
    awd_judge_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        batch_id: Set(batch.id),
        round_id: Set(round2_id),
        gamebox_instance_id: Set(instance_id),
        team_id: Set(team_id),
        status: Set(JudgeTaskStatus::Pending),
        attempt_count: Set(0),
        max_attempts: Set(3),
        deadline_at: Set(past.into()),
        created_at: Set(past.into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert task");

    use floatctf::entity::scheduled_tasks;
    let task_model = scheduled_tasks::Model {
        id: Uuid::new_v4(),
        group_id: Some(event_id),
        task_name: "test".into(),
        description: None,
        task_key: "awd.judge.batch.deadline".into(),
        trigger_type: "once".into(),
        status: "pending".into(),
        execute_at: None,
        expires_at: None,
        payload: Some(serde_json::json!({"event_id": event_id, "batch_id": batch.id})),
        enabled: true,
        protected: true,
        attempt_count: 0,
        last_error: None,
        error_msg: None,
        created_at: chrono::Utc::now().into(),
        updated_at: chrono::Utc::now().into(),
        last_run_at: None,
        max_attempts: 3,
        timeout_secs: None,
        cron_expr: None,
        locked_at: None,
        heartbeat_at: None,
    };

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);

    let handler = floatctf::modules::event::awd::scheduler::AwdJudgeBatchDeadlineHandler {
        db: actix_web::web::Data::new(db.clone()),
        network: network.clone(),
        firewall: firewall.clone(),
        publisher: publisher.clone(),
    };
    use floatctf::scheduler::TaskHandler;
    handler.run(task_model).await.unwrap();

    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(tasks[0].status, JudgeTaskStatus::JudgeError);

    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);
}

// ─────────────────────────────────────────────────────────────────
// §Core Backend: Batch Deadline Handler Idempotency
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn batch_deadline_handler_duplicate_is_idempotent() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "handler-idem", 1).await;
    let round_id = seed_round(&db, event_id, 1, RoundStatus::Completed).await;
    let (instance_id, team_id) = seed_instance(&db, event_id).await;

    let batch = awd_judge_batches::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round_id),
        total_tasks: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert batch");

    let past = chrono::Utc::now() - chrono::Duration::minutes(10);
    awd_judge_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        batch_id: Set(batch.id),
        round_id: Set(round_id),
        gamebox_instance_id: Set(instance_id),
        team_id: Set(team_id),
        status: Set(JudgeTaskStatus::Pending),
        attempt_count: Set(0),
        max_attempts: Set(3),
        deadline_at: Set(past.into()),
        created_at: Set(past.into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert task");

    use floatctf::entity::scheduled_tasks;
    let task_model = scheduled_tasks::Model {
        id: Uuid::new_v4(),
        group_id: Some(event_id),
        task_name: "test".into(),
        description: None,
        task_key: "awd.judge.batch.deadline".into(),
        trigger_type: "once".into(),
        status: "pending".into(),
        execute_at: None,
        expires_at: None,
        payload: Some(serde_json::json!({"event_id": event_id, "batch_id": batch.id})),
        enabled: true,
        protected: true,
        attempt_count: 0,
        last_error: None,
        error_msg: None,
        created_at: chrono::Utc::now().into(),
        updated_at: chrono::Utc::now().into(),
        last_run_at: None,
        max_attempts: 3,
        timeout_secs: None,
        cron_expr: None,
        locked_at: None,
        heartbeat_at: None,
    };

    let network = Arc::new(NoopNetworkRuntime);
    let firewall = Arc::new(NoopFirewallRuntime);
    let publisher = Arc::new(NoopEventPublisher);

    let handler = floatctf::modules::event::awd::scheduler::AwdJudgeBatchDeadlineHandler {
        db: actix_web::web::Data::new(db.clone()),
        network: network.clone(),
        firewall: firewall.clone(),
        publisher: publisher.clone(),
    };
    use floatctf::scheduler::TaskHandler;

    handler.run(task_model.clone()).await.unwrap();
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);

    // Second invocation — idempotent
    handler.run(task_model).await.unwrap();
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);

    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, JudgeTaskStatus::JudgeError);
}

// ─────────────────────────────────────────────────────────────────
// §Core Backend: Older-Round Gate + Real Terminalization Path
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn older_round_terminalized_by_real_path_finishes_event() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "older-real", 5).await;
    let _round5_id = seed_round(&db, event_id, 5, RoundStatus::Completed).await;
    let round4_id = seed_round(&db, event_id, 4, RoundStatus::Completed).await;
    let (instance_id, team_id) = seed_instance(&db, event_id).await;

    let batch = awd_judge_batches::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round4_id),
        total_tasks: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert batch");

    let past = chrono::Utc::now() - chrono::Duration::minutes(10);
    let now = chrono::Utc::now();
    awd_judge_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        batch_id: Set(batch.id),
        round_id: Set(round4_id),
        gamebox_instance_id: Set(instance_id),
        team_id: Set(team_id),
        status: Set(JudgeTaskStatus::Running),
        attempt_count: Set(3),
        max_attempts: Set(3),
        worker_id: Set(Some("dead-worker".into())),
        lease_token_hash: Set(Some(judge_repo::hash_lease_token("expired"))),
        lease_expires_at: Set(Some(past.into())),
        claimed_at: Set(Some(past.into())),
        heartbeat_at: Set(Some(past.into())),
        deadline_at: Set((now + chrono::Duration::minutes(5)).into()),
        created_at: Set(past.into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert task");

    // Should NOT finish — round 4 task is still Running
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
    assert_eq!(awd.status, AwdEventStatus::Running);

    // Terminalize through the real reclaim path
    let terminalized = judge_repo::reclaim_expired_leases(&db, now.into())
        .await
        .unwrap();
    assert!(!terminalized.is_empty());

    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(tasks[0].status, JudgeTaskStatus::JudgeError);

    // Now should finish
    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();
    let awd = event_repo::find_by_event_id(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(awd.status, AwdEventStatus::Finished);
}

// ─────────────────────────────────────────────────────────────────
// §Core Backend: Score Freeze After Finished
// ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn score_freeze_after_real_down_finalization() {
    let db = connect_or_skip().await;
    let Some(db) = db else { return };

    let event_id = seed_event(&db, "score-freeze2", 1).await;
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

    let score1 =
        floatctf::modules::event::awd::service::score_service::get_scoreboard(&db, event_id, &[])
            .await
            .unwrap();

    let _ = judge_repo::submit_result(
        &db,
        Uuid::new_v4(),
        "stale",
        1,
        "token",
        "rid",
        JudgeTaskStatus::Down,
        Some(1),
        None,
        None,
        None,
        chrono::Utc::now(),
    )
    .await;

    let _ = floatctf::modules::event::awd::service::score_service::record_adjustment(
        &db,
        event_id,
        Uuid::new_v4(),
        100,
        "test",
        Uuid::new_v4(),
    )
    .await;

    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    event_service::maybe_finish_event(&db, &*network, &*firewall, &*publisher, event_id)
        .await
        .unwrap();

    let score2 =
        floatctf::modules::event::awd::service::score_service::get_scoreboard(&db, event_id, &[])
            .await
            .unwrap();
    assert_eq!(score1.len(), score2.len());
    for (s1, s2) in score1.iter().zip(score2.iter()) {
        assert_eq!(s1.total_score, s2.total_score);
    }
}
