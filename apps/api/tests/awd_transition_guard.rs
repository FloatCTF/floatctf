//! DB-gated integration tests for the AWD event state-machine guard (Phase 0 P0-1).
//!
//! These tests require a reachable PostgreSQL (soft-skip when unavailable), and run
//! entirely inside a rolled-back transaction so they never leave junk rows.
//!
//! Env:
//! - `DATABASE_URL` (default `postgres://postgres:postgres@127.0.0.1:5432/floatctf_db`)

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, TransactionTrait,
};
use uuid::Uuid;

use floatctf::entity::{
    awd_events, events, sea_orm_active_enums::AwdEventStatus, sea_orm_active_enums::AwdPhase,
};
use floatctf::modules::event::awd_team::{AwdError, domain::AwdEventStatusExt, repo::event_repo};

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip awd_transition_guard: DB unreachable ({e})");
            None
        }
    }
}

fn base_awd_event(event_id: Uuid, tag: &str) -> awd_events::ActiveModel {
    awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        gamebox_cidr: Set("10.42.0.0/16".into()),
        wireguard_cidr: Set("172.31.0.0/16".into()),
        wireguard_interface_name: Set(format!("wg-test-{}", tag)),
        wireguard_listen_port: Set(5_0000 + Uuid::new_v4().as_u128() as i32 % 1000),
        flagserver_ip: Set("10.42.0.10".into()),
        judgeserver_ip: Set("10.42.0.11".into()),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        ..Default::default()
    }
}

async fn seed_event(txn: &sea_orm::DatabaseTransaction, tag: &str) -> (Uuid, awd_events::Model) {
    let event_id = Uuid::new_v4();
    let parent = events::ActiveModel {
        id: Set(event_id),
        title: Set(format!("awd-transition-test-{tag}")),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set((chrono::Utc::now() + chrono::Duration::hours(1)).into()),
        ..Default::default()
    };
    parent.insert(txn).await.expect("insert parent events row");
    let model = base_awd_event(event_id, tag)
        .insert(txn)
        .await
        .expect("insert awd_events row");
    (event_id, model)
}

#[tokio::test]
async fn invalid_transition_rejected_by_guard() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");
    let (event_id, awd) = seed_event(&txn, "invalid").await;

    // draft -> running 非法（状态机表明确拒绝）
    let err = event_repo::transition_event(
        &txn,
        awd.id,
        AwdEventStatus::Draft,
        AwdEventStatus::Running,
        Default::default(),
    )
    .await
    .expect_err("Draft->Running must be rejected");

    assert!(
        matches!(err, AwdError::InvalidState(_)),
        "expected InvalidState, got {err:?}"
    );

    // DB 状态未被改动
    let row = awd_events::Entity::find_by_id(awd.id)
        .one(&txn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, AwdEventStatus::Draft);

    // repo 层最终防线：裸 update_status 同样拒绝
    let err = event_repo::update_status(&txn, awd.id, AwdEventStatus::Running)
        .await
        .expect_err("update_status must guard invalid transitions");
    assert!(err.to_string().contains("Invalid transition"));

    txn.rollback().await.ok();
    let _ = event_id;
}

#[tokio::test]
async fn valid_transition_with_patch_is_atomic() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");
    let (event_id, awd) = seed_event(&txn, "atomic").await;

    // 1. Draft -> Configuring
    event_repo::transition_event(
        &txn,
        awd.id,
        AwdEventStatus::Draft,
        AwdEventStatus::Configuring,
        Default::default(),
    )
    .await
    .expect("Draft->Configuring valid");

    // 2. Configuring -> Prechecking（Phase 0 补充路径）
    event_repo::transition_event(
        &txn,
        awd.id,
        AwdEventStatus::Configuring,
        AwdEventStatus::Prechecking,
        Default::default(),
    )
    .await
    .expect("Configuring->Prechecking valid");

    // 3. Prechecking -> Verified：verified_revision + verified_at 与状态同事务写入
    event_repo::transition_event(
        &txn,
        awd.id,
        AwdEventStatus::Prechecking,
        AwdEventStatus::Verified,
        event_repo::TransitionPatch::verified("rev-abc123"),
    )
    .await
    .expect("Prechecking->Verified valid");

    let row = awd_events::Entity::find_by_id(awd.id)
        .one(&txn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, AwdEventStatus::Verified);
    assert_eq!(row.verified_revision.as_deref(), Some("rev-abc123"));
    assert!(
        row.verified_at.is_some(),
        "verified_at must be written atomically"
    );

    // 4. Verified -> Running（phase 附属字段同事务）
    event_repo::transition_event(
        &txn,
        awd.id,
        AwdEventStatus::Verified,
        AwdEventStatus::Running,
        event_repo::TransitionPatch::running(AwdPhase::Hardening),
    )
    .await
    .expect("Verified->Running valid");
    let row = awd_events::Entity::find_by_id(awd.id)
        .one(&txn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, AwdEventStatus::Running);
    assert_eq!(row.phase, AwdPhase::Hardening);

    // 5. Running -> Paused：paused_phase + pause_remaining_secs 同事务写入（P0-1b 列）
    event_repo::transition_event(
        &txn,
        awd.id,
        AwdEventStatus::Running,
        AwdEventStatus::Paused,
        event_repo::TransitionPatch::paused(AwdPhase::Hardening, 120),
    )
    .await
    .expect("Running->Paused valid");
    let row = awd_events::Entity::find_by_id(awd.id)
        .one(&txn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, AwdEventStatus::Paused);
    assert_eq!(row.phase, AwdPhase::Pause);
    assert_eq!(row.paused_phase, Some(AwdPhase::Hardening));
    assert_eq!(row.pause_remaining_secs, Some(120));

    txn.rollback().await.ok();
    let _ = event_id;
}

#[tokio::test]
async fn cas_semantics_rejects_stale_expected_status() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");
    let (event_id, awd) = seed_event(&txn, "cas").await;

    // 先推进到 Running（Paused 从 Running 合法可达）
    for (from, to) in [
        (AwdEventStatus::Draft, AwdEventStatus::Configuring),
        (AwdEventStatus::Configuring, AwdEventStatus::Prechecking),
        (AwdEventStatus::Prechecking, AwdEventStatus::Verified),
        (AwdEventStatus::Verified, AwdEventStatus::Running),
    ] {
        event_repo::transition_event(&txn, awd.id, from, to, Default::default())
            .await
            .expect("forward transition valid");
    }

    // 并发场景：expected_status 过期（实际是 Running，却按 Verified 的预期提交）
    let err = event_repo::transition_event(
        &txn,
        awd.id,
        AwdEventStatus::Verified,
        AwdEventStatus::Paused,
        Default::default(),
    )
    .await
    .expect_err("stale expected status must be rejected");

    assert!(
        matches!(err, AwdError::Conflict(_)),
        "expected Conflict for stale CAS, got {err:?}"
    );

    // 状态未被改变
    let row = awd_events::Entity::find_by_id(awd.id)
        .one(&txn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, AwdEventStatus::Running);

    txn.rollback().await.ok();
    let _ = event_id;
}

#[tokio::test]
async fn legal_transitions_smoke_across_table() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");
    let (event_id, awd) = seed_event(&txn, "legal").await;

    // 按状态机表全链路合法推进
    let chain = [
        (AwdEventStatus::Draft, AwdEventStatus::Configuring),
        (AwdEventStatus::Configuring, AwdEventStatus::Deploying),
        (AwdEventStatus::Deploying, AwdEventStatus::Deployed),
        (AwdEventStatus::Deployed, AwdEventStatus::Prechecking),
        (AwdEventStatus::Prechecking, AwdEventStatus::Verified),
        (AwdEventStatus::Verified, AwdEventStatus::Running),
        (AwdEventStatus::Running, AwdEventStatus::Paused),
        (AwdEventStatus::Paused, AwdEventStatus::Running),
        (AwdEventStatus::Running, AwdEventStatus::NetworkError),
        (AwdEventStatus::NetworkError, AwdEventStatus::Paused),
        (AwdEventStatus::Paused, AwdEventStatus::Finished),
        (AwdEventStatus::Finished, AwdEventStatus::Archived),
    ];
    for (from, to) in chain {
        event_repo::transition_event(&txn, awd.id, from.clone(), to.clone(), Default::default())
            .await
            .unwrap_or_else(|e| panic!("{from:?}->{to:?} must be valid: {e}"));
    }

    // 终态无后继
    let row = awd_events::Entity::find_by_id(awd.id)
        .one(&txn)
        .await
        .unwrap()
        .unwrap();
    assert!(row.status.is_terminal());

    txn.rollback().await.ok();
    let _ = event_id;
}
