//! AWDP 积分榜聚合 DB 集成测试（DB-gated，无 Docker）。
//!
//! 覆盖：
//!   - Individual：多用户注册 + break/fix 分项聚合 + 排名 + 0 分参与者上榜
//!   - Team：多队伍聚合（team 主体） + 排名
//!   - 空事件 → 空榜

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    events, sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::awdp::domain::AwdpConfig;
use floatctf::modules::event::awdp::repo::{run_repo, score_repo};
use floatctf::modules::event::awdp::service::scoreboard;

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
        .filter(events::Column::Title.like("awdp-it-sb-%"))
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

async fn start_run(db: &sea_orm::DatabaseConnection, event_id: Uuid) -> Uuid {
    let run = run_repo::create_competition_run(db, event_id, &AwdpConfig::default())
        .await
        .unwrap();
    let now = chrono::Utc::now();
    run_repo::transition_phase(
        db,
        run.id,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Pending,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break,
        run_repo::PhaseTransitionPatch {
            started_at: Some(now),
            break_ends_at: Some(now + chrono::Duration::hours(1)),
            next_action_at: Some(now + chrono::Duration::hours(1)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    run.id
}

async fn seed_fix_round(db: &sea_orm::DatabaseConnection, run_id: Uuid, seq: i32) -> Uuid {
    let now = chrono::Utc::now();
    let id = Uuid::new_v4();
    floatctf::entity::awdp_fix_rounds::ActiveModel {
        id: Set(id),
        run_id: Set(run_id),
        sequence: Set(seq),
        starts_at: Set((now - chrono::Duration::seconds(60)).into()),
        cutoff_at: Set(now.into()),
        status: Set("completed".into()),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    id
}

async fn score(
    db: &sea_orm::DatabaseConnection,
    run_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
    gamebox_id: Uuid,
    score_type: &str,
    fix_round_id: Option<Uuid>,
    delta: i64,
    key: &str,
) {
    score_repo::create_score_event(
        db,
        run_id,
        user_id,
        team_id,
        gamebox_id,
        score_type,
        fix_round_id,
        delta,
        key,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn scoreboard_aggregates_individual_mode() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let ev = create_event(&db, ParticipantMode::Individual, "awdp-it-sb-ind").await;
    let run_id = start_run(&db, ev.id).await;
    let gb = seed_gamebox(&db, "sb").await;

    let u_a = seed_user(&db, "a").await;
    let u_b = seed_user(&db, "b").await;
    let u_c = seed_user(&db, "c").await; // 注册但 0 分
    register_user(&db, ev.id, u_a).await;
    register_user(&db, ev.id, u_b).await;
    register_user(&db, ev.id, u_c).await;

    // A：break 1000 + fix 300 + fix 150 = 1450
    let f1 = seed_fix_round(&db, run_id, 1).await;
    let f2 = seed_fix_round(&db, run_id, 2).await;
    score(&db, run_id, Some(u_a), None, gb, "break", None, 1000, "sb-a-b1").await;
    score(&db, run_id, Some(u_a), None, gb, "fix", Some(f1), 300, "sb-a-f1").await;
    score(&db, run_id, Some(u_a), None, gb, "fix", Some(f2), 150, "sb-a-f2").await;
    // B：break 500
    score(&db, run_id, Some(u_b), None, gb, "break", None, 500, "sb-b-b1").await;

    let rows = scoreboard::get_scoreboard(&db, &ev).await.unwrap();
    assert_eq!(rows.len(), 3, "3 个注册用户全上榜");

    let a = &rows[0];
    assert_eq!(a.subject_id, u_a);
    assert!(a.subject_name.starts_with("nick-a"), "{} not nick-a*", a.subject_name);
    assert_eq!(a.break_score, 1000);
    assert_eq!(a.fix_score, 450);
    assert_eq!(a.total_score, 1450);
    assert_eq!(a.rank, 1);

    let b = &rows[1];
    assert_eq!(b.subject_id, u_b);
    assert_eq!(b.break_score, 500);
    assert_eq!(b.fix_score, 0);
    assert_eq!(b.total_score, 500);
    assert_eq!(b.rank, 2);

    let c = &rows[2];
    assert_eq!(c.subject_id, u_c);
    assert_eq!(c.total_score, 0);
    assert_eq!(c.rank, 3);
}

#[tokio::test]
async fn scoreboard_aggregates_team_mode() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let ev = create_event(&db, ParticipantMode::Team, "awdp-it-sb-team").await;
    let run_id = start_run(&db, ev.id).await;
    let gb = seed_gamebox(&db, "sb").await;

    let now = chrono::Utc::now().into();
    let t1 = Uuid::new_v4();
    floatctf::entity::event_teams::ActiveModel {
        id: Set(t1),
        event_id: Set(ev.id),
        name: Set("TeamOne".into()),
        description: Set(None),
        points: Set(0.0),
        banned: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let t2 = Uuid::new_v4();
    floatctf::entity::event_teams::ActiveModel {
        id: Set(t2),
        event_id: Set(ev.id),
        name: Set("TeamTwo".into()),
        description: Set(None),
        points: Set(0.0),
        banned: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // TeamOne：break 1000 + fix 150 = 1150；TeamTwo：0 分。
    let f1 = seed_fix_round(&db, run_id, 1).await;
    score(&db, run_id, None, Some(t1), gb, "break", None, 1000, "sb-t1-b1").await;
    score(&db, run_id, None, Some(t1), gb, "fix", Some(f1), 150, "sb-t1-f1").await;

    let rows = scoreboard::get_scoreboard(&db, &ev).await.unwrap();
    assert_eq!(rows.len(), 2, "2 支队伍全上榜");

    let one = &rows[0];
    assert_eq!(one.subject_id, t1);
    assert_eq!(one.subject_name, "TeamOne");
    assert_eq!(one.break_score, 1000);
    assert_eq!(one.fix_score, 150);
    assert_eq!(one.total_score, 1150);
    assert_eq!(one.rank, 1);

    let two = &rows[1];
    assert_eq!(two.subject_id, t2);
    assert_eq!(two.subject_name, "TeamTwo");
    assert_eq!(two.total_score, 0);
    assert_eq!(two.rank, 2);
}

#[tokio::test]
async fn scoreboard_empty_event_returns_empty() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let ev = create_event(&db, ParticipantMode::Individual, "awdp-it-sb-empty").await;
    let rows = scoreboard::get_scoreboard(&db, &ev).await.unwrap();
    assert!(rows.is_empty(), "未注册用户 → 空榜");
}
