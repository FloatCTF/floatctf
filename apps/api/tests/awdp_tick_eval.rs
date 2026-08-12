//! AWDP 回合 + 官方评估集成测试（DB + Docker gated，run 中心化）。
//!
//! 覆盖（plan §47）：NO_PATCH 短路 / exploit 成功→VULNERABLE+0 / exploit 失败→
//! PATCHED+score / 幂等（重复 tick 不重复物化、重复评估不重复加分）/ 最终回合→Ended /
//! tick 自动启动（start_time 到点 → 建 pending run → Break）。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    awdp_fix_rounds, events, gameboxes,
    sea_orm_active_enums::{
        AwdpEvaluationStatus, AwdpPhase, EventFamily, EventPurpose, ParticipantMode,
    },
};
use floatctf::modules::event::awdp::{
    domain::AwdpConfig,
    repo::{evaluation_repo, event_gamebox_repo, event_repo, run_repo, score_repo},
    service::{
        evaluation,
        runtime::{self, Subject},
        tick_service,
    },
};

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const IMAGE_REF: &str = "floatctf/gameboxes/test-g:1.0.3";
const IMAGE_ID: &str = "sha256:e8e04fcb779cfbfb64980f5c2c1b29ad507f3a6760e38cb0126335ea7893e70b";
const JWT_SECRET: &[u8] = b"test-platform-secret-0123456789abcdef";

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

fn docker_or_skip() -> Option<bollard::Docker> {
    let docker = bollard::Docker::connect_with_local_defaults().ok()?;
    Some(docker)
}

/// 清掉之前失败运行遗留的 awdp 测试数据（events 级联 + gameboxes 前缀）。
async fn cleanup_leftovers(db: &sea_orm::DatabaseConnection) {
    for row in events::Entity::find()
        .filter(events::Column::Title.like("awdp-it-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = events::Entity::delete_by_id(row.id).exec(db).await;
    }
    for row in gameboxes::Entity::find()
        .filter(gameboxes::Column::SafeName.like("awdp-it-gb-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = gameboxes::Entity::delete_by_id(row.id).exec(db).await;
    }
}

async fn seed_user(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
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

/// 建已开始（Break）的 competition run + 完整 [awdp] GameBox（自定义 exploit 语义）。
async fn seed_event_and_gamebox(
    db: &sea_orm::DatabaseConnection,
    tag: &str,
    exploit_content: &str,
) -> (Uuid, Uuid, Uuid) {
    seed_event_and_gamebox_with_judge(
        db,
        tag,
        exploit_content,
        r#"import json, sys
print(json.dumps([{"gamebox_ip": ip, "success": True} for ip in sys.argv[1:]], ensure_ascii=False))
sys.exit(0)
"#,
    )
    .await
}

/// 与 `seed_event_and_gamebox` 相同，但 judge 脚本内容可自定义（§87 分支需要）。
async fn seed_event_and_gamebox_with_judge(
    db: &sea_orm::DatabaseConnection,
    tag: &str,
    exploit_content: &str,
    judge_content: &str,
) -> (Uuid, Uuid, Uuid) {
    let base = chrono::Utc::now();
    let event = events::ActiveModel {
        is_virtual: Set(false),
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(None),
        title: Set(format!("awdp-it-{tag}")),
        description: Set(None),
        start_time: Set((base - chrono::Duration::hours(1)).into()),
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
    event_repo::ensure_by_event_id(db, event.id, &AwdpConfig::default())
        .await
        .unwrap();
    // Event start：建 run + 推进到 Break（break_ends 已过期 → tick 可转 Fix）。
    let run = run_repo::create_competition_run(db, event.id, &AwdpConfig::default())
        .await
        .unwrap();
    let now = chrono::Utc::now();
    run_repo::transition_phase(
        db,
        run.id,
        AwdpPhase::Pending,
        AwdpPhase::Break,
        run_repo::PhaseTransitionPatch {
            started_at: Some(now),
            break_ends_at: Some(now - chrono::Duration::minutes(1)),
            next_action_at: Some(now - chrono::Duration::minutes(1)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let now2 = chrono::Utc::now().into();
    let gb_id = Uuid::new_v4();
    let gb = gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(format!("awdp-gb-{tag}")),
        safe_name: Set(format!(
            "awdp-it-gb-{tag}-{}",
            &Uuid::new_v4().to_string()[..8]
        )),
        category: Set("web".into()),
        description: Set(String::new()),
        hidden: Set(false),
        created_at: Set(now2),
        updated_at: Set(now2),
        version: Set(Some("1.0.3".into())),
        source_toml: Set(None),
        spec_json: Set(Some(serde_json::json!({}))),
        spec_digest: Set(Some("spec".into())),
        package_digest: Set(Some("pkg".into())),
        image_ref: Set(Some(IMAGE_REF.into())),
        image_id: Set(Some(IMAGE_ID.into())),
        image_repo_digest: Set(None),
        username: Set(Some("floatctf".into())),
        recommended_cpu_millis: Set(1000),
        recommended_memory_bytes: Set(512 * 1024 * 1024),
        recommended_pids_limit: Set(100),
        healthchecks_json: Set(Some(serde_json::json!([
            {"type": "http", "port": 80, "path": "/", "expected_status": 200}
        ]))),
        judge_script_name: Set(Some("check.py".into())),
        judge_script_content: Set(Some(judge_content.to_string())),
        judge_args_json: Set(None),
        judge_timeout_secs: Set(None),
        judge_retry_interval_secs: Set(None),
        build_status: Set(Some("ready".into())),
        build_error: Set(None),
        awdp_source_code_dir: Set(Some("/var/www/html".into())),
        awdp_exploit_script_name: Set(Some("exploit.py".into())),
        awdp_exploit_script_content: Set(Some(exploit_content.to_string())),
        awdp_source_artifact_key: Set(Some(format!("gameboxes/{gb_id}/awdp/pkg/source.zip"))),
        awdp_source_artifact_digest: Set(Some("deadbeef".into())),
    }
    .insert(db)
    .await
    .unwrap();
    event_gamebox_repo::attach_gamebox(db, event.id, gb.id, false)
        .await
        .expect("attach");
    (event.id, run.id, gb.id)
}

const EXPLOIT_ALWAYS_SUCCESS: &str = r#"import json, sys
print(json.dumps([{"gamebox_ip": ip, "success": True, "flag": "flag{got-it}"} for ip in sys.argv[1:]], ensure_ascii=False))
sys.exit(0)
"#;
const EXPLOIT_ALWAYS_FAIL: &str = r#"import json, sys
print(json.dumps([{"gamebox_ip": ip, "success": False, "error": "attack blocked"} for ip in sys.argv[1:]], ensure_ascii=False))
sys.exit(1)
"#;

/// 打开指定 sequence 的 round 窗口（starts 过去 / cutoff 未来），run next_action_at 指向其 cutoff。
/// 用于 patch 提交前的 open round 窗口。
async fn open_round(db: &sea_orm::DatabaseConnection, run_id: Uuid, sequence: i32) {
    let now = chrono::Utc::now();
    for row in awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .all(db)
        .await
        .unwrap()
    {
        let mut am: awdp_fix_rounds::ActiveModel = row.clone().into();
        if row.sequence == sequence {
            am.starts_at = Set((now - chrono::Duration::minutes(1)).into());
            am.cutoff_at = Set((now + chrono::Duration::minutes(10)).into());
        } else {
            am.starts_at = Set((now + chrono::Duration::hours(1)).into());
            am.cutoff_at =
                Set((now + chrono::Duration::hours(1) + chrono::Duration::minutes(10)).into());
        }
        am.updated_at = Set(now.into());
        am.update(db).await.unwrap();
    }
    let row = run_repo::require_by_id(db, run_id).await.unwrap();
    let mut am: floatctf::entity::awdp_runs::ActiveModel = row.into();
    am.next_action_at = Set(Some((now + chrono::Duration::minutes(10)).into()));
    am.updated_at = Set(now.into());
    am.update(db).await.unwrap();
}

/// 把指定 sequence 的 round 拨到过去（其余 round 拨到未来），并让 run due。
/// 保证任意时刻只有一个 open round（patch 需要 open round）。
async fn expire_round(db: &sea_orm::DatabaseConnection, run_id: Uuid, sequence: i32) {
    let now = chrono::Utc::now();
    for row in awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .all(db)
        .await
        .unwrap()
    {
        let mut am: awdp_fix_rounds::ActiveModel = row.clone().into();
        if row.sequence == sequence {
            am.starts_at = Set((now - chrono::Duration::seconds(2)).into());
            am.cutoff_at = Set((now - chrono::Duration::seconds(1)).into());
        } else {
            am.starts_at = Set((now + chrono::Duration::hours(1)).into());
            am.cutoff_at =
                Set((now + chrono::Duration::hours(1) + chrono::Duration::minutes(10)).into());
        }
        am.updated_at = Set(now.into());
        am.update(db).await.unwrap();
    }
    // tick 以 awdp_runs.next_action_at 领取 run。
    let row = run_repo::require_by_id(db, run_id).await.unwrap();
    let mut am: floatctf::entity::awdp_runs::ActiveModel = row.into();
    am.next_action_at = Set(Some((now - chrono::Duration::seconds(1)).into()));
    am.updated_at = Set(now.into());
    am.update(db).await.unwrap();
}

#[tokio::test]
async fn tick_driven_rounds_and_scoring() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    let rt = fcmc::DockerContainerRuntime::new(docker.clone());
    if fcmc::ImageRuntime::inspect_image(&rt, IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: image {IMAGE_REF} not present");
        return;
    }
    cleanup_leftovers(&db).await;

    // 事件 A：exploit 总是成功 → VULNERABLE +0（第一轮 NO_PATCH，第二轮 VULNERABLE）。
    let (ev_a, run_a, gb_a) = seed_event_and_gamebox(&db, "fA", EXPLOIT_ALWAYS_SUCCESS).await;
    // 事件 B：exploit 总是失败 → PATCHED +score。
    let (ev_b, run_b, gb_b) = seed_event_and_gamebox(&db, "fB", EXPLOIT_ALWAYS_FAIL).await;

    let user_a = seed_user(&db, "fa").await;
    let user_b = seed_user(&db, "fb").await;
    let sub_a = Subject::user(user_a);
    let sub_b = Subject::user(user_b);
    let inst_a = runtime::start_instance(&db, &docker, JWT_SECRET, run_a, gb_a, sub_a, "flag")
        .await
        .expect("start A");
    let inst_b = runtime::start_instance(&db, &docker, JWT_SECRET, run_b, gb_b, sub_b, "flag")
        .await
        .expect("start B");

    // 1. tick：Break 到期 → Fix + rounds 物化（run 维度）。
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick");
    let row_a = run_repo::require_by_id(&db, run_a).await.unwrap();
    assert_eq!(row_a.phase, AwdpPhase::Fix);
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run_a)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 6, "3600/600 = 6 rounds");
    assert_eq!(rounds[0].sequence, 1);

    // 2. Round 1 cutoff（拨到过去）→ tick 物化 → worker：NO_PATCH（无 APPLIED patch）。
    expire_round(&db, run_a, 1).await;
    expire_round(&db, run_b, 1).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r1");
    let n1 = evaluation::worker_round(&db, &docker, 4)
        .await
        .expect("worker r1");
    assert!(n1 >= 2, "processed {n1}");

    let evals_a = evaluation_repo::list_for_run(&db, run_a).await.unwrap();
    let e1 = evals_a
        .iter()
        .find(|e| e.kind == floatctf::entity::sea_orm_active_enums::AwdpEvaluationKind::Official)
        .unwrap();
    assert_eq!(
        e1.status,
        AwdpEvaluationStatus::NoPatch,
        "round1 no patch → NO_PATCH"
    );
    let score_a = score_repo::my_total(&db, run_a, Some(user_a), None)
        .await
        .unwrap();
    assert_eq!(score_a, 0, "NO_PATCH +0");

    // 3. Round 2：先打开 round 2 窗口，再应用 patch（事件 A 需要真实 APPLIED 才 eligible）。
    open_round(&db, run_a, 2).await;
    open_round(&db, run_b, 2).await;
    let patch_script = "#!/bin/sh\nexit 0\n";
    // 事件 B：apply patch
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_b,
        inst_b.instance_id,
        patch_script,
        sub_b,
    )
    .await
    .expect("patch B");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );
    // 事件 A：apply patch（虽然 exploit 会成功，但需要 APPLIED 才评估）
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_a,
        inst_a.instance_id,
        patch_script,
        sub_a,
    )
    .await
    .expect("patch A");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );

    // Round 2 cutoff 到期 → 物化（幂等：重复 tick 不重复创建）。
    expire_round(&db, run_a, 2).await;
    expire_round(&db, run_b, 2).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r2a");
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r2b (idempotent)");
    let n2 = evaluation::worker_round(&db, &docker, 4)
        .await
        .expect("worker r2");
    assert!(n2 >= 2, "processed {n2}");

    // §85：duplicate tick 不得重复物化回合（仍是 6 个）。
    let rounds_a = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run_a)
        .await
        .unwrap();
    assert_eq!(rounds_a.len(), 6, "duplicate tick 不得创建重复 round");

    // 事件 A：exploit 成功 → VULNERABLE +0。
    let evals_a = evaluation_repo::list_for_run(&db, run_a).await.unwrap();
    let r2_a = evals_a
        .iter()
        .filter(|e| e.status != AwdpEvaluationStatus::NoPatch)
        .min_by_key(|e| e.created_at)
        .unwrap();
    assert_eq!(
        r2_a.status,
        AwdpEvaluationStatus::Vulnerable,
        "exploit success → VULNERABLE"
    );
    let score_a = score_repo::my_total(&db, run_a, Some(user_a), None)
        .await
        .unwrap();
    assert_eq!(score_a, 0, "VULNERABLE +0");

    // 事件 B：exploit 失败 → PATCHED +fix_round_score。
    let evals_b = evaluation_repo::list_for_run(&db, run_b).await.unwrap();
    let r2_b = evals_b
        .iter()
        .filter(|e| e.status != AwdpEvaluationStatus::NoPatch)
        .min_by_key(|e| e.created_at)
        .unwrap();
    assert_eq!(
        r2_b.status,
        AwdpEvaluationStatus::Patched,
        "exploit fail → PATCHED"
    );
    let row_b = run_repo::require_by_id(&db, run_b).await.unwrap();
    let score_b = score_repo::my_total(&db, run_b, Some(user_b), None)
        .await
        .unwrap();
    assert_eq!(score_b, row_b.fix_round_score, "PATCHED +fix_round_score");

    // 4. 幂等：重复 worker 不重复加分（同一评估已终态，无 pending 可领）。
    let n3 = evaluation::worker_round(&db, &docker, 4)
        .await
        .expect("worker idempotent");
    assert_eq!(n3, 0, "no pending evaluations left");
    let score_b2 = score_repo::my_total(&db, run_b, Some(user_b), None)
        .await
        .unwrap();
    assert_eq!(score_b2, row_b.fix_round_score, "重复评估不重复加分");

    // 5. 依次推进剩余回合（3..=6），最后一轮 → Ended。
    for seq in 3..=6 {
        expire_round(&db, run_a, seq).await;
        expire_round(&db, run_b, seq).await;
        tick_service::tick_once(&db, &docker, JWT_SECRET)
            .await
            .expect("tick seq");
        loop {
            let n = evaluation::worker_round(&db, &docker, 8)
                .await
                .expect("worker drain");
            if n == 0 {
                break;
            }
        }
    }
    let row_a = run_repo::require_by_id(&db, run_a).await.unwrap();
    assert_eq!(row_a.phase, AwdpPhase::Ended, "final round → Ended");
    assert!(row_a.finished_at.is_some());

    // cleanup
    let _ = events::Entity::delete_by_id(ev_a).exec(&db).await;
    let _ = events::Entity::delete_by_id(ev_b).exec(&db).await;
    let _ = runtime::stop_instance(&db, &docker, inst_a.instance_id, sub_a).await;
    let _ = runtime::stop_instance(&db, &docker, inst_b.instance_id, sub_b).await;
}

#[tokio::test]
async fn tick_starts_pending_event_at_start_time() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    cleanup_leftovers(&db).await;

    // 事件 start_time 在过去、未手动开始 → tick 自动建 run 并进入 Break。
    let base = chrono::Utc::now();
    let event = events::ActiveModel {
        is_virtual: Set(false),
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(None),
        title: Set(format!("awdp-it-auto-start")),
        description: Set(None),
        start_time: Set((base - chrono::Duration::minutes(10)).into()),
        hidden: Set(true),
        allow_join: Set(false),
        rules: Set(String::new()),
        flag_prefix: Set(None),
        end_time: Set(Some((base + chrono::Duration::hours(2)).into())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    event_repo::ensure_by_event_id(&db, event.id, &AwdpConfig::default())
        .await
        .unwrap();
    // 尚无 run（tick 自动排期）。
    assert!(
        run_repo::find_active_competition_for_event(&db, event.id)
            .await
            .unwrap()
            .is_none(),
        "未开始事件无 run"
    );

    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick");
    let run = run_repo::find_active_competition_for_event(&db, event.id)
        .await
        .unwrap()
        .expect("auto-created run");
    assert_eq!(run.phase, AwdpPhase::Break, "pending + start due → Break");
    assert!(run.break_ends_at.is_some());

    let _ = events::Entity::delete_by_id(event.id).exec(&db).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §86 Patch eligibility：failed patch / R2 无新 patch → NO_PATCH
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn patch_eligibility_requires_current_round_patch() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    let rt = fcmc::DockerContainerRuntime::new(docker.clone());
    if fcmc::ImageRuntime::inspect_image(&rt, IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: image {IMAGE_REF} not present");
        return;
    }
    cleanup_leftovers(&db).await;

    // exploit 恒失败 → 有合法 APPLIED patch 时 PATCHED +score。
    let (event_id, run_id, gb_id) = seed_event_and_gamebox(&db, "elig", EXPLOIT_ALWAYS_FAIL).await;
    let user_id = seed_user(&db, "elig").await;
    let sub = Subject::user(user_id);
    let inst = runtime::start_instance(&db, &docker, JWT_SECRET, run_id, gb_id, sub, "flag")
        .await
        .expect("start");

    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick to fix");
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run_id)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 6);

    // Round 1：apply 一个失败 patch → 不算 APPLIED → NO_PATCH（§86 failed patch）。
    open_round(&db, run_id, 1).await;
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        inst.instance_id,
        "#!/bin/sh\necho boom >&2\nexit 1\n",
        sub,
    )
    .await
    .expect("failing patch");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Failed
    );
    expire_round(&db, run_id, 1).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r1");
    let n = evaluation::worker_round(&db, &docker, 4)
        .await
        .expect("worker");
    assert_eq!(n, 1, "processed 1 eval");
    let evals = evaluation_repo::list_for_run(&db, run_id).await.unwrap();
    let e1 = evals
        .iter()
        .find(|e| {
            e.kind == floatctf::entity::sea_orm_active_enums::AwdpEvaluationKind::Official
                && e.fix_round_id == Some(rounds[0].id)
        })
        .expect("round1 eval");
    assert_eq!(
        e1.status,
        AwdpEvaluationStatus::NoPatch,
        "failed patch → 不算 APPLIED → NO_PATCH"
    );
    let score = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(score, 0, "round1 +0");

    // Round 2：成功 patch → APPLIED → eligible → PATCHED +score（§86 applied）。
    open_round(&db, run_id, 2).await;
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        inst.instance_id,
        "#!/bin/sh\nexit 0\n",
        sub,
    )
    .await
    .expect("good patch");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );
    expire_round(&db, run_id, 2).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r2");
    evaluation::worker_round(&db, &docker, 4)
        .await
        .expect("worker r2");
    let evals = evaluation_repo::list_for_run(&db, run_id).await.unwrap();
    let e2 = evals
        .iter()
        .find(|e| e.fix_round_id == Some(rounds[1].id))
        .expect("round2 eval");
    assert_eq!(
        e2.status,
        AwdpEvaluationStatus::Patched,
        "applied → eligible"
    );
    let run_row = run_repo::require_by_id(&db, run_id).await.unwrap();
    let score = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(score, run_row.fix_round_score, "PATCHED +fix_round_score");

    // Round 3：无新 patch（即便 R2 patch 仍在容器里生效）→ NO_PATCH（§86）。
    open_round(&db, run_id, 3).await;
    expire_round(&db, run_id, 3).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r3");
    evaluation::worker_round(&db, &docker, 4)
        .await
        .expect("worker r3");
    let evals = evaluation_repo::list_for_run(&db, run_id).await.unwrap();
    let e3 = evals
        .iter()
        .find(|e| e.fix_round_id == Some(rounds[2].id))
        .expect("round3 eval");
    assert_eq!(
        e3.status,
        AwdpEvaluationStatus::NoPatch,
        "R2 无新 patch（即使 R1/R2 patch 仍有效）→ NO_PATCH"
    );
    let score3 = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(score3, run_row.fix_round_score, "round3 不加分");

    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = runtime::stop_instance(&db, &docker, inst.instance_id, sub).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §87 Official evaluation：SERVICE_DOWN / FUNCTIONAL_BROKEN 分支
// ────────────────────────────────────────────────────────────────────────────

const JUDGE_ALWAYS_FAIL: &str = r#"import json, sys
print(json.dumps([{"gamebox_ip": ip, "success": False, "error": "internal error"} for ip in sys.argv[1:]], ensure_ascii=False))
sys.exit(1)
"#;

#[tokio::test]
async fn official_eval_service_down_and_functional_broken() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    let rt = fcmc::DockerContainerRuntime::new(docker.clone());
    if fcmc::ImageRuntime::inspect_image(&rt, IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: image {IMAGE_REF} not present");
        return;
    }
    cleanup_leftovers(&db).await;

    // 事件 A：health PASS + judge FAIL → FUNCTIONAL_BROKEN。
    let (ev_a, run_a, gb_a) =
        seed_event_and_gamebox_with_judge(&db, "fbk", EXPLOIT_ALWAYS_FAIL, JUDGE_ALWAYS_FAIL).await;
    let user_a = seed_user(&db, "fbk").await;
    let sub_a = Subject::user(user_a);
    let inst_a = runtime::start_instance(&db, &docker, JWT_SECRET, run_a, gb_a, sub_a, "flag")
        .await
        .expect("start A");

    // 事件 B：容器停止 → health FAIL → SERVICE_DOWN。
    let (ev_b, run_b, gb_b) = seed_event_and_gamebox(&db, "svd", EXPLOIT_ALWAYS_FAIL).await;
    let user_b = seed_user(&db, "svd").await;
    let sub_b = Subject::user(user_b);
    let inst_b = runtime::start_instance(&db, &docker, JWT_SECRET, run_b, gb_b, sub_b, "flag")
        .await
        .expect("start B");

    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick to fix");
    let rounds_a = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run_a)
        .await
        .unwrap();

    // 两个 run 都在 Round 1 先 apply patch（避免 NO_PATCH 短路，直达 health/judge 分支）。
    open_round(&db, run_a, 1).await;
    open_round(&db, run_b, 1).await;
    let _ = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_a,
        inst_a.instance_id,
        "#!/bin/sh\nexit 0\n",
        sub_a,
    )
    .await
    .expect("patch A");
    let _ = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_b,
        inst_b.instance_id,
        "#!/bin/sh\nexit 0\n",
        sub_b,
    )
    .await
    .expect("patch B");

    // 事件 B：停止容器（宿主端口映射消失 → 探针失败）。
    runtime::stop_instance(&db, &docker, inst_b.instance_id, sub_b)
        .await
        .expect("stop B container");

    expire_round(&db, run_a, 1).await;
    expire_round(&db, run_b, 1).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r1");
    let n = evaluation::worker_round(&db, &docker, 4)
        .await
        .expect("worker");
    assert_eq!(n, 2, "processed 2 evals");

    let evals_a = evaluation_repo::list_for_run(&db, run_a).await.unwrap();
    let e_a = evals_a
        .iter()
        .find(|e| e.fix_round_id == Some(rounds_a[0].id))
        .expect("eval A");
    assert_eq!(
        e_a.status,
        AwdpEvaluationStatus::FunctionalBroken,
        "health PASS + judge FAIL → FUNCTIONAL_BROKEN"
    );
    assert!(e_a.exploit_result.is_none(), "judge FAIL 不跑 exploit");

    let evals_b = evaluation_repo::list_for_run(&db, run_b).await.unwrap();
    let e_b = evals_b
        .iter()
        .filter(|e| e.kind == floatctf::entity::sea_orm_active_enums::AwdpEvaluationKind::Official)
        .next()
        .expect("eval B");
    assert_eq!(
        e_b.status,
        AwdpEvaluationStatus::ServiceDown,
        "health FAIL → SERVICE_DOWN"
    );

    // 两个分支都不加分。
    assert_eq!(
        score_repo::my_total(&db, run_a, Some(user_a), None)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        score_repo::my_total(&db, run_b, Some(user_b), None)
            .await
            .unwrap(),
        0
    );

    let _ = events::Entity::delete_by_id(ev_a).exec(&db).await;
    let _ = events::Entity::delete_by_id(ev_b).exec(&db).await;
    let _ = runtime::stop_instance(&db, &docker, inst_a.instance_id, sub_a).await;
    let _ = runtime::stop_instance(&db, &docker, inst_b.instance_id, sub_b).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §练习模式：提前 Check —— 一次 check 成功 → 从该轮起全部回合自动计分
// ────────────────────────────────────────────────────────────────────────────

/// 建练习 gamebox（不挂事件；practice run 经 create_practice_run 落 AWDPlusPractice）。
async fn seed_practice_gamebox(
    db: &sea_orm::DatabaseConnection,
    tag: &str,
    exploit_content: &str,
) -> Uuid {
    let now = chrono::Utc::now().into();
    let gb_id = Uuid::new_v4();
    gameboxes::ActiveModel {
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
        version: Set(Some("1.0.3".into())),
        source_toml: Set(None),
        spec_json: Set(Some(serde_json::json!({}))),
        spec_digest: Set(Some("spec".into())),
        package_digest: Set(Some("pkg".into())),
        image_ref: Set(Some(IMAGE_REF.into())),
        image_id: Set(Some(IMAGE_ID.into())),
        image_repo_digest: Set(None),
        username: Set(Some("floatctf".into())),
        recommended_cpu_millis: Set(1000),
        recommended_memory_bytes: Set(512 * 1024 * 1024),
        recommended_pids_limit: Set(100),
        healthchecks_json: Set(Some(serde_json::json!([
            {"type": "http", "port": 80, "path": "/", "expected_status": 200}
        ]))),
        judge_script_name: Set(Some("check.py".into())),
        judge_script_content: Set(Some(
            r#"import json, sys
print(json.dumps([{"gamebox_ip": ip, "success": True} for ip in sys.argv[1:]], ensure_ascii=False))
sys.exit(0)
"#
            .to_string(),
        )),
        judge_args_json: Set(None),
        judge_timeout_secs: Set(None),
        judge_retry_interval_secs: Set(None),
        build_status: Set(Some("ready".into())),
        build_error: Set(None),
        awdp_source_code_dir: Set(Some("/var/www/html".into())),
        awdp_exploit_script_name: Set(Some("exploit.py".into())),
        awdp_exploit_script_content: Set(Some(exploit_content.to_string())),
        awdp_source_artifact_key: Set(Some(format!("gameboxes/{gb_id}/awdp/pkg/source.zip"))),
        awdp_source_artifact_digest: Set(Some("deadbeef".into())),
    }
    .insert(db)
    .await
    .unwrap();
    gb_id
}

#[tokio::test]
async fn practice_early_check_sweeps_future_rounds() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    let rt = fcmc::DockerContainerRuntime::new(docker.clone());
    if fcmc::ImageRuntime::inspect_image(&rt, IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: image {IMAGE_REF} not present");
        return;
    }
    cleanup_leftovers(&db).await;

    // 练习 run：exploit 恒失败（patch 有效 → PATCHED）。
    let user_id = seed_user(&db, "echeck").await;
    let gb_id = seed_practice_gamebox(&db, "echeck", EXPLOIT_ALWAYS_FAIL).await;
    let run = run_repo::create_practice_run(&db, gb_id, user_id, &AwdpConfig::default())
        .await
        .expect("practice run");
    let sub = Subject::user(user_id);
    let inst = runtime::start_instance(&db, &docker, JWT_SECRET, run.id, gb_id, sub, "flag")
        .await
        .expect("start instance");

    // 直接进入 Fix（练习 break 未到期，跳过 tick 时间等待）。
    floatctf::modules::event::awdp::service::event_service::transition_break_to_fix(
        &db, &docker, JWT_SECRET, run.id,
    )
    .await
    .expect("jump to fix");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(row.phase, AwdpPhase::Fix);
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 6);

    // 打开 Round 1 窗口 + 应用 patch（本轮 eligible）。
    open_round(&db, run.id, 1).await;
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run.id,
        inst.instance_id,
        "#!/bin/sh\nexit 0\n",
        sub,
    )
    .await
    .expect("apply patch");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );

    // 提前 Check：PATCHED → 从 Round 1 起全部 6 轮自动计分。
    let res = evaluation::early_check(&db, &docker, run.id, inst.instance_id, sub)
        .await
        .expect("early check");
    assert_eq!(
        res.status,
        AwdpEvaluationStatus::Patched,
        "修复有效 → PATCHED"
    );
    assert!(res.swept, "触发自动计分");
    assert_eq!(res.target_round, 1);
    assert_eq!(res.swept_rounds, 6, "Round 1..=6 全部计分");

    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(row.early_patched_seq, Some(1), "run 标记提前确认");
    let score = score_repo::my_total(&db, run.id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(score, row.fix_round_score * 6, "break 未计分；fix 6 轮全得");

    // 每轮都有 Patched official 评估（UI Official History 直接可见）。
    let evals = evaluation_repo::list_for_run(&db, run.id).await.unwrap();
    let officials: Vec<_> = evals
        .iter()
        .filter(|e| e.kind == floatctf::entity::sea_orm_active_enums::AwdpEvaluationKind::Official)
        .collect();
    assert_eq!(officials.len(), 6, "6 轮各有官方评估");
    assert!(
        officials
            .iter()
            .all(|e| e.status == AwdpEvaluationStatus::Patched),
        "全部 PATCHED"
    );

    // 幂等：后续 tick（cutoff 到期）不重复加分、不新增评估。
    expire_round(&db, run.id, 2).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r2 cutoff");
    let n = evaluation::worker_round(&db, &docker, 8)
        .await
        .expect("worker after sweep");
    assert_eq!(n, 0, "sweep 后无 pending 评估可执行");
    let score2 = score_repo::my_total(&db, run.id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(score2, score, "不重复加分");
    let evals2 = evaluation_repo::list_for_run(&db, run.id).await.unwrap();
    assert_eq!(evals2.len(), 6, "不新增评估");

    // 再跑一次提前 Check：幂等（同轮复用 PATCHED，sweep 空转）。
    let res2 = evaluation::early_check(&db, &docker, run.id, inst.instance_id, sub)
        .await
        .expect("re-early check");
    assert_eq!(res2.status, AwdpEvaluationStatus::Patched);
    assert_eq!(res2.swept_rounds, 6);
    let score3 = score_repo::my_total(&db, run.id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(score3, score, "重复提前 Check 不加分");

    // 清理。
    let _ = runtime::stop_instance(&db, &docker, inst.instance_id, sub).await;
    cleanup_leftovers(&db).await;
}
