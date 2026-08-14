//! AWDP 比赛 Reset 次数限制（DB-gated，无 Docker）。
//!
//! 覆盖：
//!   - check_reset_allowed：比赛 Fix 阶段放行、Break 阶段拒绝、次数达上限拒绝、
//!     练习（practice run）不受限
//!   - increment_reset_count：成功 Reset 后计数递增
//!   - 练习实例 reset_count 恒 0（默认）

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    awdp_events, awdp_instances, awdp_runs, events,
    sea_orm_active_enums::{AwdpPhase, EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::awdp::domain::AwdpConfig;
use floatctf::modules::event::awdp::repo::{event_gamebox_repo, instance_repo, run_repo};
use floatctf::modules::event::awdp::service::runtime::{
    RESET_LIMIT_COMPETITION, check_reset_allowed,
};

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    let url = db_url();
    match sea_orm::Database::connect(&url).await {
        Ok(db) => Some(db),
        Err(_) => {
            eprintln!("SKIP: no postgres at {url}");
            None
        }
    }
}

async fn cleanup(db: &sea_orm::DatabaseConnection) {
    use floatctf::entity::events::Column as EC;
    let rows = events::Entity::find()
        .filter(EC::Title.like("awdp-it-rs-%"))
        .all(db)
        .await
        .unwrap();
    for row in rows {
        // event 删除会级联清理 awdp_events/awdp_runs/awdp_instances/event_instances。
        events::Entity::delete_by_id(row.id).exec(db).await.unwrap();
    }
}

async fn create_event(
    db: &sea_orm::DatabaseConnection,
    mode: ParticipantMode,
    title: &str,
) -> events::Model {
    let base = chrono::Utc::now();
    let event = events::ActiveModel {
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
    .unwrap();
    awdp_events::ActiveModel {
        event_id: Set(event.id),
        break_duration_secs: Set(3600),
        fix_duration_secs: Set(3600),
        fix_round_interval_secs: Set(600),
        break_score: Set(1000),
        fix_round_score: Set(150),
        configuration_generation: Set(1),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    event
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

/// 建 run 并推进到指定 phase（Pending→Break→Fix 用 run_repo::transition_phase）。
async fn create_run_at_phase(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    target: AwdpPhase,
) -> awdp_runs::Model {
    use floatctf::modules::event::awdp::repo::run_repo::PhaseTransitionPatch;
    let run = run_repo::create_competition_run(db, event_id, &AwdpConfig::default())
        .await
        .unwrap();
    let now = chrono::Utc::now();
    match target {
        AwdpPhase::Pending => run,
        AwdpPhase::Break => {
            run_repo::transition_phase(
                db,
                run.id,
                AwdpPhase::Pending,
                AwdpPhase::Break,
                PhaseTransitionPatch {
                    started_at: Some(now),
                    break_ends_at: Some(now + chrono::Duration::hours(1)),
                    fix_started_at: None,
                    fix_ends_at: None,
                    finished_at: None,
                    current_round: None,
                    next_action_at: Some(now + chrono::Duration::hours(1)),
                },
            )
            .await
            .unwrap();
            run_repo::require_by_id(db, run.id).await.unwrap()
        }
        AwdpPhase::Fix => {
            run_repo::transition_phase(
                db,
                run.id,
                AwdpPhase::Pending,
                AwdpPhase::Break,
                PhaseTransitionPatch {
                    started_at: Some(now),
                    break_ends_at: Some(now + chrono::Duration::minutes(30)),
                    fix_started_at: None,
                    fix_ends_at: None,
                    finished_at: None,
                    current_round: None,
                    next_action_at: Some(now + chrono::Duration::minutes(30)),
                },
            )
            .await
            .unwrap();
            run_repo::transition_phase(
                db,
                run.id,
                AwdpPhase::Break,
                AwdpPhase::PreparingFix,
                PhaseTransitionPatch {
                    started_at: None,
                    break_ends_at: None,
                    fix_started_at: None,
                    fix_ends_at: None,
                    finished_at: None,
                    current_round: None,
                    next_action_at: Some(now + chrono::Duration::minutes(30)),
                },
            )
            .await
            .unwrap();
            run_repo::transition_phase(
                db,
                run.id,
                AwdpPhase::PreparingFix,
                AwdpPhase::Fix,
                PhaseTransitionPatch {
                    started_at: None,
                    break_ends_at: None,
                    fix_started_at: Some(now),
                    fix_ends_at: Some(now + chrono::Duration::minutes(30)),
                    finished_at: None,
                    current_round: Some(1),
                    next_action_at: Some(now + chrono::Duration::minutes(30)),
                },
            )
            .await
            .unwrap();
            run_repo::require_by_id(db, run.id).await.unwrap()
        }
        other => panic!("unexpected target phase {other:?}"),
    }
}

/// 建实例（event_instances + awdp_instances），返回 awdp_instances 行。
async fn create_instance(
    db: &sea_orm::DatabaseConnection,
    run_id: Uuid,
    gamebox_id: Uuid,
    user_id: Uuid,
) -> awdp_instances::Model {
    let (_, ext) = instance_repo::create_instance(
        db,
        run_id,
        gamebox_id,
        Some(user_id),
        None,
        &format!(
            "awdp-rs-{}",
            &Uuid::new_v4().to_string().replace('-', "")[..20]
        ),
        "img:latest",
    )
    .await
    .unwrap();
    ext
}

async fn set_reset_count(db: &sea_orm::DatabaseConnection, instance_id: Uuid, n: i64) {
    let mut am: awdp_instances::ActiveModel = awdp_instances::Entity::find_by_id(instance_id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
        .into();
    am.reset_count = Set(n);
    am.update(db).await.unwrap();
}

#[tokio::test]
async fn reset_allowed_fix_phase_only_and_limit_3() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, ParticipantMode::Individual, "awdp-it-rs-fix").await;
    let u = seed_user(&db, "r1").await;
    register_user(&db, event.id, u).await;
    let gb = seed_gamebox(&db, "rg").await;
    event_gamebox_repo::attach_gamebox(&db, event.id, gb, false)
        .await
        .unwrap();

    let run = create_run_at_phase(&db, event.id, AwdpPhase::Fix).await;
    let ext = create_instance(&db, run.id, gb, u).await;

    // Fix + reset_count=0 → 放行。
    check_reset_allowed(&run, &ext).unwrap();
    // 递增到 1/2。
    set_reset_count(&db, ext.instance_id, 2).await;
    let ext2 = instance_repo::find_by_instance_id(&db, ext.instance_id)
        .await
        .unwrap()
        .1;
    check_reset_allowed(&run, &ext2).unwrap();
    // 达到上限 3 → 拒绝。
    set_reset_count(&db, ext.instance_id, RESET_LIMIT_COMPETITION).await;
    let ext3 = instance_repo::find_by_instance_id(&db, ext.instance_id)
        .await
        .unwrap()
        .1;
    let err = check_reset_allowed(&run, &ext3).unwrap_err();
    assert!(
        err.to_string().contains("已达上限"),
        "expected limit error, got: {err}"
    );

    cleanup(&db).await;
}

#[tokio::test]
async fn reset_rejected_during_break() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, ParticipantMode::Individual, "awdp-it-rs-brk").await;
    let u = seed_user(&db, "r2").await;
    register_user(&db, event.id, u).await;
    let gb = seed_gamebox(&db, "rb").await;
    event_gamebox_repo::attach_gamebox(&db, event.id, gb, false)
        .await
        .unwrap();

    let run = create_run_at_phase(&db, event.id, AwdpPhase::Break).await;
    let ext = create_instance(&db, run.id, gb, u).await;
    let err = check_reset_allowed(&run, &ext).unwrap_err();
    assert!(
        err.to_string().contains("仅 Fix 阶段"),
        "expected fix-only error, got: {err}"
    );

    cleanup(&db).await;
}

#[tokio::test]
async fn practice_reset_unrestricted() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    // practice：虚拟 event（is_virtual）挂载 gamebox，run 带 gamebox_id。
    let now = chrono::Utc::now();
    let practice_event = events::ActiveModel {
        is_virtual: Set(true),
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Practice),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(Some("AWDPlusPractice".into())),
        title: Set("awdp-it-rs-pr".into()),
        description: Set(None),
        start_time: Set((now - chrono::Duration::hours(1)).into()),
        hidden: Set(true),
        allow_join: Set(false),
        rules: Set(String::new()),
        flag_prefix: Set(None),
        end_time: Set(Some((now + chrono::Duration::hours(2)).into())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let u = seed_user(&db, "r3").await;
    let gb = seed_gamebox(&db, "rp").await;
    event_gamebox_repo::ensure_mounted(&db, practice_event.id, gb)
        .await
        .unwrap();

    // 直接创建 practice run（run.gamebox_id = Some）。
    let run = run_repo::create_practice_run(&db, gb, u, &AwdpConfig::default())
        .await
        .unwrap();
    let ext = instance_repo::create_instance(
        &db,
        run.id,
        gb,
        Some(u),
        None,
        &format!(
            "awdp-rs-p-{}",
            &Uuid::new_v4().to_string().replace('-', "")[..20]
        ),
        "img:latest",
    )
    .await
    .unwrap()
    .1;

    // 默认 reset_count = 0。
    assert_eq!(ext.reset_count, 0, "practice reset_count 默认 0");
    // Break 阶段也放行（练习不受限）。
    check_reset_allowed(&run, &ext).unwrap();

    // 递增一次后仍放行。
    instance_repo::increment_reset_count(&db, ext.instance_id)
        .await
        .unwrap();
    let ext2 = instance_repo::find_by_instance_id(&db, ext.instance_id)
        .await
        .unwrap()
        .1;
    assert_eq!(ext2.reset_count, 1);
    check_reset_allowed(&run, &ext2).unwrap();

    // practice run 删除（虚拟事件级联亦可）。
    awdp_runs::Entity::delete_by_id(run.id)
        .exec(&db)
        .await
        .unwrap();
    cleanup(&db).await;
}

#[tokio::test]
async fn increment_reset_count_steps_up() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event = create_event(&db, ParticipantMode::Individual, "awdp-it-rs-inc").await;
    let u = seed_user(&db, "r4").await;
    register_user(&db, event.id, u).await;
    let gb = seed_gamebox(&db, "ri").await;
    event_gamebox_repo::attach_gamebox(&db, event.id, gb, false)
        .await
        .unwrap();
    let run = create_run_at_phase(&db, event.id, AwdpPhase::Fix).await;
    let ext = create_instance(&db, run.id, gb, u).await;

    assert_eq!(ext.reset_count, 0);
    for expected in [1, 2, 3] {
        instance_repo::increment_reset_count(&db, ext.instance_id)
            .await
            .unwrap();
        let (_, e) = instance_repo::find_by_instance_id(&db, ext.instance_id)
            .await
            .unwrap();
        assert_eq!(e.reset_count, expected);
    }

    cleanup(&db).await;
}
