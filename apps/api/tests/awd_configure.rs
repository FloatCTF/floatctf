//! DB-gated regression tests for the AWD Configure page.
//!
//! Tests soft-skip when PostgreSQL is unavailable and execute inside a rolled-back
//! transaction, so they leave no fixture rows behind.

use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};
use uuid::Uuid;

use floatctf::entity::{
    awd_events, events, scheduled_tasks,
    sea_orm_active_enums::{AwdEventStatus, AwdPhase, EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::{
    awd::{
        AwdError, scheduler,
        service::config_service::{self, AwdEventConfigPatch},
    },
    common::application::admin_service::{self as common_admin, PatchEventRequest},
};
use floatctf::scheduler::TaskKey;

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(error) => {
            eprintln!("skip awd_configure: DB unreachable ({error})");
            None
        }
    }
}

async fn seed_event<C: sea_orm::ConnectionTrait + Send>(
    db: &C,
    status: AwdEventStatus,
) -> (Uuid, awd_events::Model) {
    let event_id = Uuid::new_v4();
    let now = Utc::now();
    events::ActiveModel {
        id: Set(event_id),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        title: Set(format!("awd-configure-{}", event_id.simple())),
        start_time: Set((now + Duration::hours(4)).into()),
        end_time: Set(Some((now + Duration::hours(8)).fixed_offset())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert parent event");

    let verified = status == AwdEventStatus::Verified;
    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        status: Set(status),
        phase: Set(AwdPhase::Hardening),
        event_secret_ciphertext: Set(vec![1; 32]),
        event_secret_nonce: Set(vec![2; 24]),
        round_duration_secs: Set(300),
        configuration_generation: Set(7),
        verified_generation: Set(verified.then_some(7)),
        verified_revision: Set(verified.then(|| "revision-7".to_string())),
        verified_at: Set(verified.then(|| now.into())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert awd event");

    (event_id, awd)
}

#[tokio::test]
async fn runtime_change_invalidates_verified_and_reschedules_start() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin fixture transaction");
    let (event_id, awd) = seed_event(&txn, AwdEventStatus::Verified).await;
    let planned_start = (Utc::now() + Duration::hours(3)).fixed_offset();

    let updated = config_service::update_event_config(
        &txn,
        event_id,
        AwdEventConfigPatch {
            expected_updated_at: Some(awd.updated_at),
            round_duration_secs: Some(600),
            planned_start_at: Some(Some(planned_start)),
            ..Default::default()
        },
    )
    .await
    .expect("update Configure settings");

    assert_eq!(updated.status, AwdEventStatus::Configuring);
    assert_eq!(updated.round_duration_secs, 600);
    assert_eq!(updated.configuration_generation, 8);
    assert!(updated.verified_at.is_none());
    assert!(updated.verified_revision.is_none());
    assert!(updated.verified_generation.is_none());
    let stored_start = scheduler::find_event_start_schedule(&txn, event_id)
        .await
        .expect("read planned start")
        .expect("planned start exists");
    assert_eq!(
        stored_start.timestamp_micros(),
        planned_start.timestamp_micros()
    );

    let precheck = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq(TaskKey::AwdAutoPrecheck.to_string()))
        .filter(scheduled_tasks::Column::Status.eq("pending"))
        .one(&txn)
        .await
        .expect("read automatic precheck")
        .expect("automatic precheck scheduled");
    assert_eq!(
        precheck
            .execute_at
            .expect("precheck execute_at")
            .timestamp_micros(),
        (planned_start - Duration::hours(1)).timestamp_micros()
    );

    txn.rollback().await.ok();
}

#[tokio::test]
async fn stale_config_version_is_rejected_without_overwrite() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin fixture transaction");
    let (event_id, awd) = seed_event(&txn, AwdEventStatus::Configuring).await;

    let first = config_service::update_event_config(
        &txn,
        event_id,
        AwdEventConfigPatch {
            expected_updated_at: Some(awd.updated_at),
            round_duration_secs: Some(450),
            ..Default::default()
        },
    )
    .await
    .expect("first update");

    let error = config_service::update_event_config(
        &txn,
        event_id,
        AwdEventConfigPatch {
            expected_updated_at: Some(awd.updated_at),
            round_duration_secs: Some(900),
            ..Default::default()
        },
    )
    .await
    .expect_err("stale version must be rejected");
    assert!(matches!(error, AwdError::Conflict(_)));

    let actual = awd_events::Entity::find_by_id(awd.id)
        .one(&txn)
        .await
        .expect("read awd event")
        .expect("awd event exists");
    assert_eq!(actual.round_duration_secs, 450);
    assert_eq!(actual.updated_at, first.updated_at);

    txn.rollback().await.ok();
}

#[tokio::test]
async fn running_planned_start_task_cannot_be_rescheduled() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin fixture transaction");
    let (event_id, awd) = seed_event(&txn, AwdEventStatus::Configuring).await;
    let start_at = (Utc::now() + Duration::hours(3)).fixed_offset();
    scheduler::schedule_event_start(&txn, event_id, Some(start_at))
        .await
        .expect("schedule start task");
    let task = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq(TaskKey::AwdEventStart.to_string()))
        .one(&txn)
        .await
        .expect("read start task")
        .expect("start task exists");
    scheduled_tasks::ActiveModel {
        id: Set(task.id),
        status: Set("running".into()),
        locked_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .update(&txn)
    .await
    .expect("claim start task");

    let error = config_service::update_event_config(
        &txn,
        event_id,
        AwdEventConfigPatch {
            expected_updated_at: Some(awd.updated_at),
            planned_start_at: Some(Some((Utc::now() + Duration::hours(5)).fixed_offset())),
            ..Default::default()
        },
    )
    .await
    .expect_err("running task must reject reschedule");
    assert!(matches!(error, AwdError::Conflict(_)));

    txn.rollback().await.ok();
}

#[tokio::test]
async fn parent_start_time_change_reschedules_auto_precheck() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin fixture transaction");
    let (event_id, _) = seed_event(&txn, AwdEventStatus::Configuring).await;
    let original_start = (Utc::now() + Duration::hours(4)).fixed_offset();
    scheduler::schedule_auto_precheck(&txn, event_id, original_start, Utc::now())
        .await
        .expect("schedule original precheck");
    let new_start = (Utc::now() + Duration::hours(6)).fixed_offset();

    common_admin::patch_event(
        &txn,
        event_id,
        PatchEventRequest {
            title: None,
            description: None,
            hidden: None,
            allow_join: None,
            rules: None,
            flag_prefix: None,
            start_time: Some(new_start),
            end_time: Some((Utc::now() + Duration::hours(9)).fixed_offset()),
        },
    )
    .await
    .expect("patch parent event time");

    let precheck = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq(TaskKey::AwdAutoPrecheck.to_string()))
        .one(&txn)
        .await
        .expect("read automatic precheck")
        .expect("automatic precheck exists");
    assert_eq!(
        precheck
            .execute_at
            .expect("precheck execute_at")
            .timestamp_micros(),
        (new_start - Duration::hours(1)).timestamp_micros()
    );

    txn.rollback().await.ok();
}

#[tokio::test]
async fn legacy_draft_save_enters_configuring_without_generation_bump() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin fixture transaction");
    let (event_id, awd) = seed_event(&txn, AwdEventStatus::Draft).await;

    let updated = config_service::update_event_config(
        &txn,
        event_id,
        AwdEventConfigPatch {
            expected_updated_at: Some(awd.updated_at),
            ..Default::default()
        },
    )
    .await
    .expect("save legacy draft");

    assert_eq!(updated.status, AwdEventStatus::Configuring);
    assert_eq!(updated.configuration_generation, 7);

    txn.rollback().await.ok();
}
