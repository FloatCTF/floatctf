//! AWDP 赛事开始自动启动目标枚举 + Event 时间修改一致性（DB-gated，无 Docker）。
//!
//! 覆盖：
//!   - auto_start_targets：Individual（event_users）与 Team（event_teams）主体枚举、
//!     hidden gamebox 跳过、无参与者空集
//!   - patch_event 修改 AWDP 事件 start_time：end_time 派生同步（start+break+fix）、
//!     pending run next_action_at 跟随；已 Break 的 run 不受影响

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};
use uuid::Uuid;

use floatctf::entity::{
    awdp_events, events,
    sea_orm_active_enums::{AwdpPhase, EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::awdp::domain::AwdpConfig;
use floatctf::modules::event::awdp::repo::{event_gamebox_repo, run_repo};
use floatctf::modules::event::awdp::service::event_service;
use floatctf::modules::event::common::application::admin_service::{
    self as common_admin, PatchEventRequest,
};

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
        .filter(events::Column::Title.like("awdp-it-as-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = events::Entity::delete_by_id(row.id).exec(db).await;
    }
}

async fn create_event(
    db: &sea_orm::DatabaseConnection,
    mode: ParticipantMode,
    title: &str,
) -> events::Model {
    let base = chrono::Utc::now();
    events::ActiveModel {
        is_virtual: Set(false),
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(mode),
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

async fn seed_user(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
    let now = chrono::Utc::now().into();
    let id = Uuid::new_v4();
    floatctf::entity::users::ActiveModel {
        id: Set(id),
        username: Set(format!("u-{tag}-{}", &id.to_string()[..8])),
        nickname: Set(format!("nick-{tag}-{}", &id.to_string()[..8])),
        password: Set("x".into()),
        email: Set(format!("u-{tag}-{}@it.example", &id.to_string()[..8])),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    id
}

async fn seed_gamebox(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
    let now = chrono::Utc::now().into();
    let gb_id = Uuid::new_v4();
    floatctf::entity::gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(format!("awdp-gb-{tag}")),
        safe_name: Set(format!(
            "awdp-it-gb-{tag}-{}",
            &Uuid::new_v4().to_string()[..8]
        )),
        category: Set("web".into()),
        description: Set(String::new()),
        hidden: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        version: Set(Some("1.0.0".into())),
        source_toml: Set(None),
        spec_json: Set(Some(serde_json::json!({}))),
        spec_digest: Set(Some("spec".into())),
        package_digest: Set(Some("pkg".into())),
        image_ref: Set(None),
        image_id: Set(None),
        image_repo_digest: Set(None),
        username: Set(None),
        recommended_cpu_millis: Set(1000),
        recommended_memory_bytes: Set(512 * 1024 * 1024),
        recommended_pids_limit: Set(100),
        healthchecks_json: Set(Some(serde_json::json!([]))),
        judge_script_name: Set(None),
        judge_script_content: Set(None),
        judge_args_json: Set(None),
        judge_timeout_secs: Set(None),
        judge_retry_interval_secs: Set(None),
        build_status: Set(Some("ready".into())),
        build_error: Set(None),
        awdp_source_code_dir: Set(Some("/var/www/html".into())),
        awdp_exploit_script_name: Set(Some("exploit.py".into())),
        awdp_exploit_script_content: Set(Some("x".into())),
        awdp_source_artifact_key: Set(Some(format!("gameboxes/{gb_id}/awdp/pkg/source.tar.gz"))),
        awdp_source_artifact_digest: Set(Some("deadbeef".into())),
    }
    .insert(db)
    .await
    .unwrap();
    gb_id
}

async fn register_user(db: &sea_orm::DatabaseConnection, event_id: Uuid, user_id: Uuid) {
    floatctf::entity::event_users::ActiveModel {
        event_id: Set(event_id),
        user_id: Set(user_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

async fn create_pending_run(db: &sea_orm::DatabaseConnection, event_id: Uuid) -> Uuid {
    let run = run_repo::create_competition_run(db, event_id, &AwdpConfig::default())
        .await
        .unwrap();
    run.id
}

#[tokio::test]
async fn auto_start_targets_individual_enumerates_users_x_gameboxes() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, ParticipantMode::Individual, "awdp-it-as-ind").await;
    let u1 = seed_user(&db, "a1").await;
    let u2 = seed_user(&db, "a2").await;
    register_user(&db, event.id, u1).await;
    register_user(&db, event.id, u2).await;

    let gb1 = seed_gamebox(&db, "g1").await;
    let gb2 = seed_gamebox(&db, "g2").await;
    event_gamebox_repo::attach_gamebox(&db, event.id, gb1, false)
        .await
        .unwrap();
    // hidden gamebox 不应出现在目标里
    event_gamebox_repo::attach_gamebox(&db, event.id, gb2, true)
        .await
        .unwrap();

    let run_id = create_pending_run(&db, event.id).await;
    let targets = event_service::auto_start_targets(&db, run_id)
        .await
        .unwrap();

    assert_eq!(targets.len(), 2, "2 users x 1 visible gamebox");
    let gb1_seen = targets
        .iter()
        .filter(|(_, gb)| *gb == gb1)
        .collect::<Vec<_>>();
    assert_eq!(gb1_seen.len(), 2);
    let gb2_seen = targets.iter().filter(|(_, gb)| *gb == gb2).count();
    assert_eq!(gb2_seen, 0, "hidden gamebox excluded");

    cleanup(&db).await;
}

#[tokio::test]
async fn auto_start_targets_team_enumerates_teams_x_gameboxes() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, ParticipantMode::Team, "awdp-it-as-team").await;
    let now = chrono::Utc::now().into();
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    for (tid, name) in [(t1, "T1"), (t2, "T2")] {
        floatctf::entity::event_teams::ActiveModel {
            id: Set(tid),
            event_id: Set(event.id),
            name: Set(name.into()),
            description: Set(None),
            points: Set(0.0),
            created_at: Set(now),
            updated_at: Set(now),
            banned: Set(false),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
    }

    let gb1 = seed_gamebox(&db, "tg1").await;
    event_gamebox_repo::attach_gamebox(&db, event.id, gb1, false)
        .await
        .unwrap();

    let run_id = create_pending_run(&db, event.id).await;
    let targets = event_service::auto_start_targets(&db, run_id)
        .await
        .unwrap();

    assert_eq!(targets.len(), 2, "2 teams x 1 gamebox");
    assert!(
        targets
            .iter()
            .any(|(s, gb)| s.team_id == Some(t1) && *gb == gb1)
    );
    assert!(
        targets
            .iter()
            .any(|(s, gb)| s.team_id == Some(t2) && *gb == gb1)
    );

    cleanup(&db).await;
}

#[tokio::test]
async fn auto_start_targets_empty_when_no_participants() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, ParticipantMode::Individual, "awdp-it-as-empty").await;
    let gb1 = seed_gamebox(&db, "eg1").await;
    event_gamebox_repo::attach_gamebox(&db, event.id, gb1, false)
        .await
        .unwrap();

    let run_id = create_pending_run(&db, event.id).await;
    let targets = event_service::auto_start_targets(&db, run_id)
        .await
        .unwrap();
    assert!(targets.is_empty(), "no registered users → no targets");

    cleanup(&db).await;
}

#[tokio::test]
async fn patch_event_awdp_time_change_keeps_derived_end_time_and_pending_cursor() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, ParticipantMode::Individual, "awdp-it-as-patch").await;
    // awdp_events 配置（默认 break 1h + fix 1h → end = start + 2h）
    awdp_events::ActiveModel {
        event_id: Set(event.id),
        break_duration_secs: Set(3600),
        fix_duration_secs: Set(3600),
        fix_round_interval_secs: Set(600),
        break_score: Set(1000),
        fix_round_score: Set(150),
        configuration_generation: Set(0),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // pending run（start_time 尚未到点）
    let run_id = create_pending_run(&db, event.id).await;
    let run = run_repo::require_by_id(&db, run_id).await.unwrap();
    assert_eq!(run.phase, AwdpPhase::Pending);
    let original_cursor = run.next_action_at.unwrap();

    // 把 start_time 往后推 1h，end_time 应派生同步为 start + 2h
    let new_start = event.start_time + chrono::Duration::hours(1);
    let txn = db.begin().await.unwrap();
    let updated = common_admin::patch_event(
        &txn,
        event.id,
        PatchEventRequest {
            title: None,
            description: None,
            hidden: None,
            allow_join: None,
            rules: None,
            flag_prefix: None,
            start_time: Some(new_start),
            end_time: None,
        },
    )
    .await
    .unwrap();

    let derived_end = new_start + chrono::Duration::seconds(7200);
    assert_eq!(
        updated.end_time,
        Some(derived_end),
        "AWDP end_time 派生同步 start + break + fix"
    );
    let run2 = run_repo::require_by_id(&txn, run_id).await.unwrap();
    assert_eq!(
        run2.next_action_at.unwrap().with_timezone(&chrono::Utc),
        new_start.with_timezone(&chrono::Utc),
        "pending run next_action_at 跟随新 start_time"
    );
    assert_ne!(
        run2.next_action_at.unwrap().with_timezone(&chrono::Utc),
        original_cursor.with_timezone(&chrono::Utc)
    );

    txn.rollback().await.ok();
    cleanup(&db).await;
}
