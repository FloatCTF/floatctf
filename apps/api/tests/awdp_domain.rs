//! AWDP 核心领域 DB 集成测试（DB-gated）。
//!
//! 覆盖（plan §47 的 EventMode / config 子集）：
//!   - EventMode 组合：awdp practice individual / competition individual / competition team
//!     通过；awdp practice team 失败（DB CHECK + Rust validate 双保险）
//!   - awdp_events：ensure 默认配置 / 配置校验（divisibility）/ start 前可改 /
//!     start 后冻结 / expected_updated_at 乐观锁
//!   - 阶段迁移 CAS

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    awdp_events, events,
    sea_orm_active_enums::{AwdpPhase, EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::awdp::domain::{AwdpConfig, AwdpConfigPatch};
use floatctf::modules::event::awdp::repo::event_repo;
use floatctf::modules::event::common::domain::event_mode::EventMode;

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip: {e}");
            None
        }
    }
}

async fn cleanup(db: &sea_orm::DatabaseConnection) {
    // 仅清理带测试前缀的行。
    for row in events::Entity::find()
        .filter(events::Column::Title.like("awdp-it-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = events::Entity::delete_by_id(row.id).exec(db).await;
    }
}

async fn create_event(
    db: &sea_orm::DatabaseConnection,
    mode: &EventMode,
    title: &str,
    start_secs: i64,
    end_secs: i64,
) -> events::Model {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    let base = chrono::Utc::now();
    events::ActiveModel {
        id: Set(Uuid::new_v4()),
        family: Set(mode.family.clone()),
        purpose: Set(mode.purpose.clone()),
        participant_mode: Set(mode.participant_mode.clone()),
        system_key: Set(None),
        title: Set(title.to_string()),
        description: Set(None),
        start_time: Set((base + chrono::Duration::seconds(start_secs)).into()),
        hidden: Set(true),
        allow_join: Set(false),
        rules: Set(String::new()),
        flag_prefix: Set(None),
        end_time: Set(Some((base + chrono::Duration::seconds(end_secs)).into())),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

// ────────────────────────────────────────────────────────────────────────────
// EventMode 组合
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn event_mode_combinations() {
    assert!(
        EventMode::new(
            EventFamily::Awdp,
            EventPurpose::Practice,
            ParticipantMode::Individual
        )
        .is_ok()
    );
    assert!(
        EventMode::new(
            EventFamily::Awdp,
            EventPurpose::Competition,
            ParticipantMode::Individual
        )
        .is_ok()
    );
    assert!(
        EventMode::new(
            EventFamily::Awdp,
            EventPurpose::Competition,
            ParticipantMode::Team
        )
        .is_ok()
    );
    // AWDP + Practice + Team 不允许（plan §1）。
    assert!(
        EventMode::new(
            EventFamily::Awdp,
            EventPurpose::Practice,
            ParticipantMode::Team
        )
        .is_err()
    );
}

#[tokio::test]
async fn db_check_rejects_awdp_practice_team() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let mode = EventMode::new(
        EventFamily::Awdp,
        EventPurpose::Practice,
        ParticipantMode::Team,
    )
    .unwrap_err();
    let _ = mode; // validate 已拒绝

    // DB CHECK 兜底：绕过 Rust 层直接插入应失败。
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    let base = chrono::Utc::now();
    let res = events::ActiveModel {
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Practice),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        title: Set("awdp-it-badmode".into()),
        description: Set(None),
        start_time: Set((base + chrono::Duration::seconds(60)).into()),
        hidden: Set(true),
        allow_join: Set(false),
        rules: Set(String::new()),
        flag_prefix: Set(None),
        end_time: Set(Some((base + chrono::Duration::seconds(3600)).into())),
        ..Default::default()
    }
    .insert(&db)
    .await;
    assert!(res.is_err(), "DB must reject awdp practice team");
}

#[tokio::test]
async fn awdp_practice_allows_bounded_end_time() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    // awdp practice 允许 end_time NOT NULL（新 CHECK 分支）。
    let mode = EventMode::awdp_practice();
    let ev = create_event(&db, &mode, "awdp-it-practice-bounded", 60, 3600).await;
    let reload = events::Entity::find_by_id(ev.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        reload.end_time.is_some(),
        "awdp practice end_time must persist"
    );
    cleanup(&db).await;
}

// ────────────────────────────────────────────────────────────────────────────
// awdp_events 配置
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn config_defaults_and_update_lifecycle() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let mode = EventMode::awdp_individual_competition();
    let ev = create_event(&db, &mode, "awdp-it-config", 60, 3600).await;

    // ensure 默认配置。
    let awdp = event_repo::ensure_by_event_id(&db, ev.id, &AwdpConfig::default())
        .await
        .expect("ensure awdp event");
    assert_eq!(awdp.phase, AwdpPhase::Pending);
    assert_eq!(awdp.break_duration_secs, 3600);
    assert_eq!(awdp.fix_round_interval_secs, 600);
    assert_eq!(awdp.configuration_generation, 1);

    // start 前可改（乐观锁必填）。
    let patch = AwdpConfigPatch {
        expected_updated_at: Some(awdp.updated_at.into()),
        break_duration_secs: Some(1800),
        fix_duration_secs: Some(1800),
        fix_round_interval_secs: Some(600),
        break_score: Some(2000),
        fix_round_score: Some(300),
    };
    let updated = event_repo::update_config(&db, ev.id, patch)
        .await
        .expect("update before start");
    assert_eq!(updated.break_duration_secs, 1800);
    assert_eq!(updated.fix_round_score, 300);
    assert_eq!(updated.configuration_generation, 2);
    assert_eq!(updated.break_duration_secs, 1800);

    // 事件 end_time 与 start + break + fix 同步。
    let ev_reload = events::Entity::find_by_id(ev.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let expected_end =
        ev_reload.start_time + chrono::Duration::seconds(1800) + chrono::Duration::seconds(1800);
    assert_eq!(ev_reload.end_time.unwrap(), expected_end, "end_time synced");

    // 非整除 interval 拒绝。
    let cur = event_repo::require_by_event_id(&db, ev.id).await.unwrap();
    let bad = AwdpConfigPatch {
        expected_updated_at: Some(cur.updated_at.into()),
        fix_duration_secs: Some(3600),
        fix_round_interval_secs: Some(700),
        ..Default::default()
    };
    let err = event_repo::update_config(&db, ev.id, bad)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("divisible"), "{err}");

    // 乐观锁冲突。
    let cur = event_repo::require_by_event_id(&db, ev.id).await.unwrap();
    let stale = AwdpConfigPatch {
        expected_updated_at: Some((chrono::Utc::now() - chrono::Duration::hours(1)).into()),
        break_duration_secs: Some(3600),
        ..Default::default()
    };
    let err = event_repo::update_config(&db, ev.id, stale)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("mismatch"), "{err}");

    // 进入 Break 后配置冻结。
    event_repo::transition_phase(
        &db,
        ev.id,
        AwdpPhase::Pending,
        AwdpPhase::Break,
        event_repo::PhaseTransitionPatch {
            started_at: Some(chrono::Utc::now()),
            break_ends_at: Some(chrono::Utc::now() + chrono::Duration::seconds(1800)),
            next_action_at: Some(chrono::Utc::now() + chrono::Duration::seconds(1800)),
            ..Default::default()
        },
    )
    .await
    .expect("transition to break");
    let cur = event_repo::require_by_event_id(&db, ev.id).await.unwrap();
    let frozen = AwdpConfigPatch {
        expected_updated_at: Some(cur.updated_at.into()),
        fix_round_score: Some(500),
        ..Default::default()
    };
    let err = event_repo::update_config(&db, ev.id, frozen)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("locked"), "{err}");

    cleanup(&db).await;
}

#[tokio::test]
async fn phase_transitions_are_cas_guarded() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let mode = EventMode::awdp_practice();
    let ev = create_event(&db, &mode, "awdp-it-phase", 60, 3600).await;
    event_repo::ensure_by_event_id(&db, ev.id, &AwdpConfig::default())
        .await
        .expect("ensure");

    // 非法迁移：pending → fix 直接拒绝。
    let err = event_repo::transition_phase(
        &db,
        ev.id,
        AwdpPhase::Pending,
        AwdpPhase::Fix,
        Default::default(),
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("Invalid AWDP phase transition"),
        "{err}"
    );

    // CAS：期望阶段不符 → Conflict。
    let err = event_repo::transition_phase(
        &db,
        ev.id,
        AwdpPhase::Fix,
        AwdpPhase::Ended,
        Default::default(),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("concurrent"), "{err}");

    // 合法链 pending→break→fix→ended。
    event_repo::transition_phase(
        &db,
        ev.id,
        AwdpPhase::Pending,
        AwdpPhase::Break,
        Default::default(),
    )
    .await
    .unwrap();
    event_repo::transition_phase(
        &db,
        ev.id,
        AwdpPhase::Break,
        AwdpPhase::Fix,
        Default::default(),
    )
    .await
    .unwrap();
    event_repo::transition_phase(
        &db,
        ev.id,
        AwdpPhase::Fix,
        AwdpPhase::Ended,
        Default::default(),
    )
    .await
    .unwrap();

    let final_row = event_repo::require_by_event_id(&db, ev.id).await.unwrap();
    assert_eq!(final_row.phase, AwdpPhase::Ended);

    cleanup(&db).await;
}
