//! AWDP overview 展示 phase 解析（DB-gated，无 Docker）。
//!
//! 背景：overview/admin config 的 phase 原来只从 active competition run
//! （Pending/Break/Fix）派生，run 结束（Ended）或过渡态（PreparingFix）时
//! 回退为 Pending → 已结束赛事显示 "Starts in: 0s" + 空进度条。
//!
//! 覆盖：
//!   - run 走完 Pending→Break→PreparingFix→Fix→Ended 后：
//!     `find_active_competition_for_event` 为 None（动作类端点不再可用），
//!     `find_display_run_for_event` / `find_latest_for_event` 返回 Ended run
//!     （phase/timestamps/current_round 完整，供展示 Finished + 满进度条）
//!   - 停在 PreparingFix 时 display 查询如实返回 preparing_fix（过渡态可见）
//!   - 事件从未建 run 时两者均返回 None（overview 落回 pending 语义不变）

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    events,
    sea_orm_active_enums::{AwdpPhase, EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::awdp::domain::AwdpConfig;
use floatctf::modules::event::awdp::repo::run_repo;

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
    for row in events::Entity::find()
        .filter(events::Column::Title.like("awdp-it-oe-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = events::Entity::delete_by_id(row.id).exec(db).await;
    }
}

async fn create_event(db: &sea_orm::DatabaseConnection, title: &str) -> events::Model {
    let base = chrono::Utc::now();
    events::ActiveModel {
        is_virtual: Set(false),
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(None),
        title: Set(title.to_string()),
        description: Set(None),
        start_time: Set((base - chrono::Duration::minutes(5)).into()),
        hidden: Set(true),
        allow_join: Set(false),
        rules: Set(String::new()),
        flag_prefix: Set(None),
        end_time: Set(Some((base + chrono::Duration::hours(2)).into())),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

async fn create_run(db: &sea_orm::DatabaseConnection, event_id: Uuid) -> Uuid {
    run_repo::create_competition_run(db, event_id, &AwdpConfig::default())
        .await
        .unwrap()
        .id
}

/// 走完完整阶段链：Pending → Break → PreparingFix → Fix → Ended（transition_phase 纯 CAS，
/// 不依赖 docker/实例）。
async fn walk_to_ended(db: &sea_orm::DatabaseConnection, run_id: Uuid) {
    let now = chrono::Utc::now();
    let break_ends = now + chrono::Duration::seconds(3600);
    run_repo::transition_phase(
        db,
        run_id,
        AwdpPhase::Pending,
        AwdpPhase::Break,
        run_repo::PhaseTransitionPatch {
            started_at: Some(now),
            break_ends_at: Some(break_ends),
            next_action_at: Some(break_ends),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    run_repo::transition_phase(
        db,
        run_id,
        AwdpPhase::Break,
        AwdpPhase::PreparingFix,
        run_repo::PhaseTransitionPatch::default(),
    )
    .await
    .unwrap();
    let fix_start = now + chrono::Duration::seconds(3600);
    run_repo::transition_phase(
        db,
        run_id,
        AwdpPhase::PreparingFix,
        AwdpPhase::Fix,
        run_repo::PhaseTransitionPatch {
            fix_started_at: Some(fix_start),
            fix_ends_at: Some(fix_start + chrono::Duration::seconds(3600)),
            current_round: Some(1),
            next_action_at: Some(fix_start + chrono::Duration::seconds(600)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    run_repo::transition_phase(
        db,
        run_id,
        AwdpPhase::Fix,
        AwdpPhase::Ended,
        run_repo::PhaseTransitionPatch {
            finished_at: Some(fix_start + chrono::Duration::seconds(3600)),
            current_round: Some(6),
            ..Default::default()
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn display_run_after_ended_reports_ended_with_timestamps() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, "awdp-it-oe-ended").await;
    let run_id = create_run(&db, event.id).await;
    walk_to_ended(&db, run_id).await;

    let run = run_repo::require_by_id(&db, run_id).await.unwrap();
    assert_eq!(run.phase, AwdpPhase::Ended);
    assert!(run.started_at.is_some(), "ended run 保留 started_at");
    assert!(run.finished_at.is_some(), "ended run 保留 finished_at");
    assert_eq!(run.current_round, 6);

    // 动作类解析：Ended 不属于 active → None（start/break/patch 等端点继续拒绝）。
    assert!(
        run_repo::find_active_competition_for_event(&db, event.id)
            .await
            .unwrap()
            .is_none(),
        "Ended run 不应被当作 active"
    );

    // 展示类解析：必须返回 Ended run（前端据此显示 Finished + 满进度条）。
    let display = run_repo::find_display_run_for_event(&db, event.id)
        .await
        .unwrap()
        .expect("display run 应回退到最新 ended run");
    assert_eq!(display.phase, AwdpPhase::Ended);
    assert_eq!(display.id, run_id);
    assert!(display.finished_at.is_some());

    let latest = run_repo::find_latest_for_event(&db, event.id)
        .await
        .unwrap()
        .expect("latest run 应返回 ended run");
    assert_eq!(latest.id, run_id);
    assert_eq!(latest.phase, AwdpPhase::Ended);

    cleanup(&db).await;
}

#[tokio::test]
async fn display_run_in_preparing_fix_reports_preparing_fix() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, "awdp-it-oe-pfix").await;
    let run_id = create_run(&db, event.id).await;

    let now = chrono::Utc::now();
    run_repo::transition_phase(
        &db,
        run_id,
        AwdpPhase::Pending,
        AwdpPhase::Break,
        run_repo::PhaseTransitionPatch {
            started_at: Some(now),
            break_ends_at: Some(now + chrono::Duration::seconds(3600)),
            next_action_at: Some(now + chrono::Duration::seconds(3600)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    run_repo::transition_phase(
        &db,
        run_id,
        AwdpPhase::Break,
        AwdpPhase::PreparingFix,
        run_repo::PhaseTransitionPatch::default(),
    )
    .await
    .unwrap();

    assert!(
        run_repo::find_active_competition_for_event(&db, event.id)
            .await
            .unwrap()
            .is_none(),
        "PreparingFix 不在 active 三态内"
    );
    let display = run_repo::find_display_run_for_event(&db, event.id)
        .await
        .unwrap()
        .expect("display run 应返回 preparing_fix 过渡态");
    assert_eq!(display.phase, AwdpPhase::PreparingFix);
    assert_eq!(display.id, run_id);

    cleanup(&db).await;
}

#[tokio::test]
async fn display_run_none_when_event_never_has_run() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, "awdp-it-oe-norun").await;

    assert!(
        run_repo::find_display_run_for_event(&db, event.id)
            .await
            .unwrap()
            .is_none(),
        "无任何 run 时 overview 应保持 pending"
    );
    assert!(
        run_repo::find_latest_for_event(&db, event.id)
            .await
            .unwrap()
            .is_none()
    );

    cleanup(&db).await;
}
