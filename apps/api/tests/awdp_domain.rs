//! AWDP 核心领域 DB 集成测试（DB-gated）。
//!
//! 覆盖（plan §47 的 EventMode / config 子集，run 中心化后）：
//!   - EventMode 组合：awdp practice individual / competition individual / competition team
//!     通过；awdp practice team 失败（DB CHECK + Rust validate 双保险）
//!   - awdp_events：ensure 默认配置 / 配置校验（divisibility）/ 无 active run 可改 /
//!     active run 后冻结 / expected_updated_at 乐观锁 / events.end_time 回写
//!   - awdp_runs 阶段迁移 CAS（practice run pending→break→fix→ended + 非法迁移）

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    events,
    sea_orm_active_enums::{AwdpPhase, EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::awdp::domain::{AwdpConfig, AwdpConfigPatch};
use floatctf::modules::event::awdp::repo::{event_repo, run_repo};
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
        is_virtual: Set(mode.is_practice()),
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
        is_virtual: Set(true),
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
// awdp_events 配置（纯配置；运行态在 awdp_runs）
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

    // ensure 默认配置（awdp_events 不再有 phase/next_action_at；plan §76 defaults 全断言）。
    let awdp = event_repo::ensure_by_event_id(&db, ev.id, &AwdpConfig::default())
        .await
        .expect("ensure awdp event");
    assert_eq!(awdp.break_duration_secs, 3600);
    assert_eq!(awdp.fix_duration_secs, 3600);
    assert_eq!(awdp.fix_round_interval_secs, 600);
    assert_eq!(awdp.break_score, 1000);
    assert_eq!(awdp.fix_round_score, 150);
    assert_eq!(awdp.configuration_generation, 1);

    // 未启动（无 active run）可改（乐观锁必填）。
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

    // 零值拒绝（plan §76）。
    let cur = event_repo::require_by_event_id(&db, ev.id).await.unwrap();
    let zero = AwdpConfigPatch {
        expected_updated_at: Some(cur.updated_at.into()),
        break_duration_secs: Some(0),
        ..Default::default()
    };
    let err = event_repo::update_config(&db, ev.id, zero)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("> 0"), "{err}");
    let zero_score = AwdpConfigPatch {
        expected_updated_at: Some(cur.updated_at.into()),
        break_score: Some(-5),
        ..Default::default()
    };
    let err = event_repo::update_config(&db, ev.id, zero_score)
        .await
        .unwrap_err();
    assert!(err.to_string().contains(">= 0"), "{err}");

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

    // 事件启动（创建 active run）后配置冻结。
    let run = run_repo::create_competition_run(
        &db,
        ev.id,
        &AwdpConfig {
            break_duration_secs: 1800,
            fix_duration_secs: 1800,
            fix_round_interval_secs: 600,
            break_score: 2000,
            fix_round_score: 300,
        },
    )
    .await
    .expect("create competition run");
    run_repo::transition_phase(
        &db,
        run.id,
        AwdpPhase::Pending,
        AwdpPhase::Break,
        run_repo::PhaseTransitionPatch {
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

// ────────────────────────────────────────────────────────────────────────────
// 配置快照语义（plan §76：active Run snapshot / new Run gets new config）
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn run_snapshot_captures_config_immune_to_later_changes() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let mode = EventMode::awdp_individual_competition();
    let ev = create_event(&db, &mode, "awdp-it-snapshot", 60, 3600).await;
    let awdp = event_repo::ensure_by_event_id(&db, ev.id, &AwdpConfig::default())
        .await
        .expect("ensure");

    // 启动前把配置改为自定义（break 1800 / fix 1800 / interval 600 / 2000+300）。
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
        .expect("update config");
    assert_eq!(updated.configuration_generation, 2);

    // 1. 用该配置创建 competition run → 快照 5 列 = 创建时配置（plan §76 snapshot）。
    let run = run_repo::create_competition_run(
        &db,
        ev.id,
        &AwdpConfig {
            break_duration_secs: 1800,
            fix_duration_secs: 1800,
            fix_round_interval_secs: 600,
            break_score: 2000,
            fix_round_score: 300,
        },
    )
    .await
    .expect("create run");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(row.break_duration_secs, 1800);
    assert_eq!(row.fix_duration_secs, 1800);
    assert_eq!(row.fix_round_interval_secs, 600);
    assert_eq!(row.break_score, 2000);
    assert_eq!(row.fix_round_score, 300);
    assert_eq!(row.total_rounds, 3, "1800/600 = 3 rounds");

    // 2. active run 期间配置冻结：admin 修改被拒（plan §76 已锁定）。
    let cur = event_repo::require_by_event_id(&db, ev.id).await.unwrap();
    let frozen = AwdpConfigPatch {
        expected_updated_at: Some(cur.updated_at.into()),
        break_duration_secs: Some(7200),
        ..Default::default()
    };
    let err = event_repo::update_config(&db, ev.id, frozen)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("locked"), "{err}");

    // 3. 即使 awdp_events 行被直接改掉（绕过锁，模拟外部写入/数据漂移），
    //    已启动 run 的 snapshot 不受影响（plan §76 snapshot 语义）。
    use floatctf::entity::awdp_events;
    {
        let cur = event_repo::require_by_event_id(&db, ev.id).await.unwrap();
        let mut am: awdp_events::ActiveModel = cur.into();
        am.break_duration_secs = Set(9999);
        am.break_score = Set(999);
        am.updated_at = Set(chrono::Utc::now().into());
        am.update(&db).await.unwrap();
    }
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(row.break_duration_secs, 1800, "run snapshot 不随配置行变化");
    assert_eq!(row.break_score, 2000, "run snapshot 不随配置行变化");

    // 4. run 结束后配置重新可改。
    let now = chrono::Utc::now();
    run_repo::transition_phase(
        &db,
        run.id,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Pending,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break,
        run_repo::PhaseTransitionPatch {
            started_at: Some(now),
            break_ends_at: Some(now + chrono::Duration::seconds(1800)),
            next_action_at: Some(now + chrono::Duration::seconds(1800)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    run_repo::transition_phase(
        &db,
        run.id,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Fix,
        Default::default(),
    )
    .await
    .unwrap();
    run_repo::transition_phase(
        &db,
        run.id,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Fix,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Ended,
        Default::default(),
    )
    .await
    .unwrap();
    let cur = event_repo::require_by_event_id(&db, ev.id).await.unwrap();
    let reopen = AwdpConfigPatch {
        expected_updated_at: Some(cur.updated_at.into()),
        fix_duration_secs: Some(1200),
        fix_round_interval_secs: Some(600),
        ..Default::default()
    };
    event_repo::update_config(&db, ev.id, reopen)
        .await
        .expect("ended 后配置可改");

    // 5. 新 run 用新配置（plan §76 new Run gets new config；practice run 直接注入配置）。
    let user_id = seed_user(&db, "snap").await;
    let gb_id = seed_gamebox(&db, "snap").await;
    let new_run = run_repo::create_practice_run(
        &db,
        gb_id,
        user_id,
        &AwdpConfig {
            break_duration_secs: 600,
            fix_duration_secs: 1200,
            fix_round_interval_secs: 600,
            break_score: 500,
            fix_round_score: 100,
        },
    )
    .await
    .expect("new practice run");
    let new_row = run_repo::require_by_id(&db, new_run.id).await.unwrap();
    assert_eq!(new_row.break_duration_secs, 600);
    assert_eq!(new_row.fix_duration_secs, 1200);
    assert_eq!(new_row.fix_round_score, 100);
    assert_eq!(new_row.total_rounds, 2);

    cleanup(&db).await;
}

// ────────────────────────────────────────────────────────────────────────────
// awdp_runs 阶段迁移 CAS
// ────────────────────────────────────────────────────────────────────────────

async fn seed_user(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    let now = chrono::Utc::now().into();
    let id = Uuid::new_v4();
    floatctf::entity::users::ActiveModel {
        id: Set(id),
        username: Set(format!("u-{tag}-{}", &id.to_string()[..8])),
        nickname: Set(format!("u-{tag}-{}", &id.to_string()[..8])),
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
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
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
        awdp_source_artifact_key: Set(Some(format!("gameboxes/{gb_id}/awdp/pkg/source.zip"))),
        awdp_source_artifact_digest: Set(Some("deadbeef".into())),
    }
    .insert(db)
    .await
    .unwrap();
    gb_id
}

#[tokio::test]
async fn phase_transitions_are_cas_guarded() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let user_id = seed_user(&db, "phase").await;
    let gb_id = seed_gamebox(&db, "phase").await;
    let run = run_repo::create_practice_run(&db, gb_id, user_id, &AwdpConfig::default())
        .await
        .expect("create practice run");

    // practice run 创建即 Break，但**冻结**（next_action_at=None，未点「开始」前 tick 不推进）。
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(row.phase, AwdpPhase::Break, "practice run 创建即 Break");
    assert!(row.started_at.is_some());
    assert!(row.break_ends_at.is_some());
    assert!(
        row.next_action_at.is_none(),
        "练习 run 创建即冻结（等玩家点开始）"
    );

    // 非法迁移：pending → fix 直接拒绝（虽然当前是 break，但非法链仍被拒绝）。
    let err = run_repo::transition_phase(
        &db,
        run.id,
        AwdpPhase::Break,
        AwdpPhase::Ended,
        Default::default(),
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("Invalid AWDP phase transition"),
        "{err}"
    );

    // CAS：期望阶段不符 → Conflict。
    let err = run_repo::transition_phase(
        &db,
        run.id,
        AwdpPhase::Fix,
        AwdpPhase::Ended,
        Default::default(),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("concurrent"), "{err}");

    // 合法链 break→fix→ended。
    run_repo::transition_phase(
        &db,
        run.id,
        AwdpPhase::Break,
        AwdpPhase::Fix,
        Default::default(),
    )
    .await
    .unwrap();
    run_repo::transition_phase(
        &db,
        run.id,
        AwdpPhase::Fix,
        AwdpPhase::Ended,
        Default::default(),
    )
    .await
    .unwrap();

    let final_row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(final_row.phase, AwdpPhase::Ended);

    // ended 后不能回退。
    let err = run_repo::transition_phase(
        &db,
        run.id,
        AwdpPhase::Ended,
        AwdpPhase::Break,
        Default::default(),
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("Invalid AWDP phase transition"),
        "{err}"
    );

    // 同 user+gamebox 再建 practice run：active unique 冲突（ended 的不算 active）。
    let run2 = run_repo::create_practice_run(&db, gb_id, user_id, &AwdpConfig::default())
        .await
        .expect("ended 后可再训练（新 run）");
    let row2 = run_repo::require_by_id(&db, run2.id).await.unwrap();
    assert_eq!(row2.phase, AwdpPhase::Break);
    assert_ne!(run2.id, run.id, "train again 创建新 run");

    // 同 user+gamebox 已有 active run → Conflict。
    let err = run_repo::create_practice_run(&db, gb_id, user_id, &AwdpConfig::default())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("进行中"), "{err}");

    // 冗余 awdp_events 表仍为纯配置（无运行态列可读）。
    let ev_mode = EventMode::awdp_individual_competition();
    let ev = create_event(&db, &ev_mode, "awdp-it-phase-ev", 60, 3600).await;
    let awdp = event_repo::ensure_by_event_id(&db, ev.id, &AwdpConfig::default())
        .await
        .unwrap();
    let _ = awdp;
    // 直接查表确认没有 phase/next_action_at 列（生成的 Model 不含这些字段即编译期保证）。

    cleanup(&db).await;
}
