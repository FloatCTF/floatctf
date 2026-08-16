//! 练习模式计分（check 失败 -50）与 ALL Check 集成测试（DB + Docker gated）。
//!
//! 覆盖（2026-08-14 需求）：
//!   - 练习 NO_PATCH / VULNERABLE → -50（幂等账本，score_type='fix' 负 delta）；
//!   - 练习 PATCHED → +fix_round_score（+150）；
//!   - 练习实例容器命名 `AWDPP-{user8}-{gamebox8}-{run8}`；
//!   - ALL Check 成功 → 当前轮起剩余回合全部 +150 + 实例停止 + run 直接 Ended；
//!   - ALL Check 失败（NO_PATCH）→ 不落账不扣分不写评估，run 仍 Fix、等官方 check；
//!   - 手动 Test Check（练习）追加 exploit 判定：成功 → Vulnerable（不计分）。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use floatctf::entity::{
    awdp_fix_rounds, gameboxes,
    sea_orm_active_enums::{AwdpEvaluationStatus, AwdpPhase},
};
use floatctf::modules::event::awdp::{
    repo::{evaluation_repo, run_repo, score_repo},
    service::{evaluation, runtime, tick_service},
};

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const IMAGE_REF: &str = "floatctf/gameboxes/test-g:1.0.3";
const IMAGE_ID: &str = "sha256:e8e04fcb779cfbfb64980f5c2c1b29ad507f3a6760e38cb0126335ea7893e70b";
const JWT_SECRET: &[u8] = b"test-platform-secret-0123456789abcdef";

const JUDGE_ALWAYS_PASS: &str = r#"import json, sys
print(json.dumps([{"gamebox_ip": ip, "success": True} for ip in sys.argv[1:]], ensure_ascii=False))
sys.exit(0)
"#;
const EXPLOIT_ALWAYS_SUCCESS: &str = r#"import json, sys
print(json.dumps([{"gamebox_ip": ip, "success": True} for ip in sys.argv[1:]], ensure_ascii=False))
sys.exit(0)
"#;
const EXPLOIT_ALWAYS_FAIL: &str = r#"import json, sys
print(json.dumps([{"gamebox_ip": ip, "success": False, "error": "no flag"} for ip in sys.argv[1:]], ensure_ascii=False))
sys.exit(0)
"#;

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
    bollard::Docker::connect_with_local_defaults().ok()
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

/// 可训练 GameBox（health PASS + 自定义 judge/exploit）。
async fn seed_trainable_gamebox(
    db: &sea_orm::DatabaseConnection,
    tag: &str,
    judge_content: &str,
    exploit_content: &str,
) -> Uuid {
    let now = chrono::Utc::now().into();
    let gb_id = Uuid::new_v4();
    gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(format!("awdp-train-{tag}")),
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
        judge_script_name: Set(None),
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
    gb_id
}

async fn remove_judge_container() {
    let Ok(docker) = bollard::Docker::connect_with_local_defaults() else {
        return;
    };
    let rt = fcmc::DockerContainerRuntime::new(docker);
    let _ = fcmc::ContainerRuntime::stop_and_remove(
        &rt,
        floatctf::modules::event::awdp::domain::judge::PRACTICE_JUDGE_CONTAINER_NAME,
        fcmc::IMMEDIATE_STOP_TIMEOUT,
    )
    .await;
}

async fn cleanup(db: &sea_orm::DatabaseConnection) {
    remove_judge_container().await;
    for row in gameboxes::Entity::find()
        .filter(gameboxes::Column::SafeName.like("awdp-it-gb-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = floatctf::entity::awdp_event_gameboxes::Entity::delete_many()
            .filter(floatctf::entity::awdp_event_gameboxes::Column::GameboxId.eq(row.id))
            .exec(db)
            .await;
        let _ = gameboxes::Entity::delete_by_id(row.id).exec(db).await;
    }
    // 清理本文件创建的 practice run（残留会占用 active-run 唯一约束）。
    let _ = floatctf::entity::awdp_runs::Entity::delete_many()
        .filter(floatctf::entity::awdp_runs::Column::OwnerUserId.is_not_null())
        .exec(db)
        .await;
}

fn awdp_config() -> floatctf::core::config::AwdpStaticConfig {
    floatctf::core::config::AwdpStaticConfig {
        practice_judgeserver_image: "floatctf/infra/awdp-judgeserver:latest".to_string(),
        practice_network_subnet: "10.42.2.0/23".to_string(),
        practice_judge_ip: "10.42.2.2".to_string(),
        network_pool: "10.43.0.0/16".to_string(),
        event_netmask: 24,
        practice_judge_data_host: "judge-server".to_string(),
        platform_internal_url: "http://host.docker.internal:9090".to_string(),
        eval_lease_duration_secs: 120,
        eval_max_attempts: 3,
    }
}

/// 构造 PatchPayload（patch.sh exit 0 空操作）。
fn patch_payload() -> floatctf::modules::event::awdp::service::patch_service::PatchPayload {
    floatctf::modules::event::awdp::service::patch_service::PatchPayload {
        script: "#!/bin/sh\nexit 0\n".to_string(),
        archive_sha256: "a".repeat(64),
        files: vec![],
    }
}

/// 打开指定回合窗口（其余回合推到 1 小时后），并更新 run.current_round。
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
    am.current_round = Set(sequence);
    am.updated_at = Set(now.into());
    am.update(db).await.unwrap();
}

/// 把已 applied 的 patch 提交时间回拨（使其 ≤ round cutoff，本轮 eligible）。
async fn backdate_patch_applied(
    db: &sea_orm::DatabaseConnection,
    instance_id: Uuid,
    seconds_ago: i64,
) {
    let row = floatctf::entity::awdp_patch_submissions::Entity::find()
        .filter(floatctf::entity::awdp_patch_submissions::Column::InstanceId.eq(instance_id))
        .filter(floatctf::entity::awdp_patch_submissions::Column::Status.eq("applied"))
        .order_by_desc(floatctf::entity::awdp_patch_submissions::Column::AppliedAt)
        .one(db)
        .await
        .unwrap()
        .expect("applied patch row");
    let mut am: floatctf::entity::awdp_patch_submissions::ActiveModel = row.into();
    am.applied_at = Set(Some(
        (chrono::Utc::now() - chrono::Duration::seconds(seconds_ago)).into(),
    ));
    am.update(db).await.unwrap();
}

async fn expire_round(db: &sea_orm::DatabaseConnection, run_id: Uuid, sequence: i32) {
    let now = chrono::Utc::now();
    for row in awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .all(db)
        .await
        .unwrap()
    {
        if row.sequence == sequence {
            let mut am: awdp_fix_rounds::ActiveModel = row.clone().into();
            am.cutoff_at = Set((now - chrono::Duration::minutes(1)).into());
            am.updated_at = Set(now.into());
            am.update(db).await.unwrap();
        }
    }
    // tick 以 awdp_runs.next_action_at 领取 run。
    let row = run_repo::require_by_id(db, run_id).await.unwrap();
    let mut am: floatctf::entity::awdp_runs::ActiveModel = row.into();
    am.next_action_at = Set(Some((now - chrono::Duration::seconds(1)).into()));
    am.updated_at = Set(now.into());
    am.update(db).await.unwrap();
}

/// 建 practice run → Launch → Fix（物化 6 回合）→ 启动实例。返回 (run_id, instance_id)。
async fn seed_practice_fix_with_instance(
    db: &sea_orm::DatabaseConnection,
    docker: &bollard::Docker,
    user_id: Uuid,
    gb_id: Uuid,
    tag: &str,
) -> (Uuid, Uuid) {
    let run = floatctf::modules::event::awdp::service::practice_service::start_training(
        db, docker, JWT_SECRET, user_id, gb_id, "flag",
    )
    .await
    .expect("start training");
    run_repo::launch_practice_run(db, run.id)
        .await
        .expect("launch");
    floatctf::modules::event::awdp::service::event_service::transition_break_to_fix(
        db, docker, JWT_SECRET, run.id,
    )
    .await
    .expect("jump to fix");
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(db, run.id)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 6, "fix 会话物化 6 个回合");

    let inst = runtime::start_instance(
        db,
        docker,
        JWT_SECRET,
        &awdp_config(),
        run.id,
        gb_id,
        runtime::Subject::user(user_id),
        "flag",
    )
    .await
    .expect("start instance");
    let _ = tag;
    (run.id, inst.instance_id)
}

/// 官方评估：expire 指定回合 → tick 物化 → worker 执行。返回该 run 该回合的评估。
async fn run_official_round(
    db: &sea_orm::DatabaseConnection,
    docker: &bollard::Docker,
    run_id: Uuid,
    sequence: i32,
) -> floatctf::entity::awdp_evaluations::Model {
    expire_round(db, run_id, sequence).await;
    tick_service::tick_once(db, docker, JWT_SECRET, &awdp_config())
        .await
        .expect("tick");
    let _ = evaluation::worker_round(db, docker, "floatctf-api-worker", 8, 120, 3)
        .await
        .expect("worker");
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(db, run_id)
        .await
        .unwrap();
    let r = rounds
        .iter()
        .find(|r| r.sequence == sequence)
        .expect("round row");
    let evals = evaluation_repo::list_for_run(db, run_id).await.unwrap();
    evals
        .into_iter()
        .find(|e| {
            e.kind == floatctf::entity::sea_orm_active_enums::AwdpEvaluationKind::Official
                && e.fix_round_id == Some(r.id)
        })
        .expect("official eval")
}

#[tokio::test]
async fn practice_failures_deduct_50_and_patched_plus_150() {
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
    cleanup(&db).await;

    // run A：exploit 恒成功 → NO_PATCH(r1) → VULNERABLE(r2)，应扣 -50 × 2。
    let user_a = seed_user(&db, "psca").await;
    let gb_a = seed_trainable_gamebox(&db, "psca", JUDGE_ALWAYS_PASS, EXPLOIT_ALWAYS_SUCCESS).await;
    let (run_a, inst_a) = seed_practice_fix_with_instance(&db, &docker, user_a, gb_a, "a").await;
    let sub_a = runtime::Subject::user(user_a);

    // 容器命名：练习实例规则风格（AWDPP-{user8}-{gamebox8}-{run8}）。
    {
        let (inst, _) =
            floatctf::modules::event::awdp::repo::instance_repo::find_by_instance_id(&db, inst_a)
                .await
                .unwrap();
        let name = inst.container_name.clone();
        assert!(
            name.starts_with("AWDPP-"),
            "practice container name: {name}"
        );
        assert_eq!(name.len(), "AWDPP-".len() + 8 + 1 + 8 + 1 + 8, "{name}");
    }

    // Round 1：无 patch → NO_PATCH -50。
    open_round(&db, run_a, 1).await;
    let e1 = run_official_round(&db, &docker, run_a, 1).await;
    assert_eq!(e1.status, AwdpEvaluationStatus::NoPatch, "NO_PATCH");
    let s1 = score_repo::my_total(&db, run_a, Some(user_a), None)
        .await
        .unwrap();
    assert_eq!(s1, -50, "NO_PATCH -50");

    // Round 2：apply patch（exploit 恒成功）→ VULNERABLE -50。
    open_round(&db, run_a, 2).await;
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_a,
        inst_a,
        &patch_payload(),
        sub_a,
    )
    .await
    .expect("patch A");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );
    backdate_patch_applied(&db, inst_a, 120).await;
    let e2 = run_official_round(&db, &docker, run_a, 2).await;
    assert_eq!(e2.status, AwdpEvaluationStatus::Vulnerable, "VULNERABLE");
    let s2 = score_repo::my_total(&db, run_a, Some(user_a), None)
        .await
        .unwrap();
    assert_eq!(s2, -100, "NO_PATCH(-50) + VULNERABLE(-50)");

    // run B：exploit 恒失败 → NO_PATCH(r1) -50 + PATCHED(r2) +150 = 100。
    let user_b = seed_user(&db, "pscb").await;
    let gb_b = seed_trainable_gamebox(&db, "pscb", JUDGE_ALWAYS_PASS, EXPLOIT_ALWAYS_FAIL).await;
    let (run_b, inst_b) = seed_practice_fix_with_instance(&db, &docker, user_b, gb_b, "b").await;
    let sub_b = runtime::Subject::user(user_b);
    open_round(&db, run_b, 1).await;
    let e1b = run_official_round(&db, &docker, run_b, 1).await;
    assert_eq!(e1b.status, AwdpEvaluationStatus::NoPatch);
    open_round(&db, run_b, 2).await;
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_b,
        inst_b,
        &patch_payload(),
        sub_b,
    )
    .await
    .expect("patch B");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );
    backdate_patch_applied(&db, inst_b, 120).await;
    let e2b = run_official_round(&db, &docker, run_b, 2).await;
    assert_eq!(e2b.status, AwdpEvaluationStatus::Patched, "PATCHED");
    let row_b = run_repo::require_by_id(&db, run_b).await.unwrap();
    let s2b = score_repo::my_total(&db, run_b, Some(user_b), None)
        .await
        .unwrap();
    assert_eq!(
        s2b,
        row_b.fix_round_score - 50,
        "PATCHED(+150) + NO_PATCH(-50)"
    );

    // 账本明细：-50 / -50 / -50 / +150（score_type='fix'，delta 为负合法）。
    let history_a = score_repo::my_history(&db, run_a, Some(user_a), None)
        .await
        .unwrap();
    let deltas_a: Vec<i64> = history_a.iter().map(|s| s.delta).collect();
    assert_eq!(deltas_a, vec![-50, -50], "{deltas_a:?}");
    let history_b = score_repo::my_history(&db, run_b, Some(user_b), None)
        .await
        .unwrap();
    let mut deltas_b: Vec<i64> = history_b.iter().map(|s| s.delta).collect();
    deltas_b.sort_unstable();
    let mut want_b = vec![-50, row_b.fix_round_score];
    want_b.sort_unstable();
    assert_eq!(deltas_b, want_b, "{deltas_b:?}");

    // 幂等：重复 worker 不重复扣分。
    let _ = evaluation::worker_round(&db, &docker, "floatctf-api-worker", 8, 120, 3)
        .await
        .expect("worker idempotent");
    let s2_again = score_repo::my_total(&db, run_a, Some(user_a), None)
        .await
        .unwrap();
    assert_eq!(s2_again, -100, "重复评估不重复扣分");

    let _ = runtime::stop_instance(&db, &docker, inst_a, sub_a).await;
    let _ = runtime::stop_instance(&db, &docker, inst_b, sub_b).await;
    cleanup(&db).await;
}

#[tokio::test]
async fn all_check_success_sweeps_remaining_rounds_and_ends_run() {
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
    cleanup(&db).await;

    let user_id = seed_user(&db, "allok").await;
    let gb_id = seed_trainable_gamebox(&db, "allok", JUDGE_ALWAYS_PASS, EXPLOIT_ALWAYS_FAIL).await;
    let (run_id, inst_id) =
        seed_practice_fix_with_instance(&db, &docker, user_id, gb_id, "ok").await;
    let sub = runtime::Subject::user(user_id);

    // Round 1 窗口内 apply patch（exploit 恒失败 → ALL Check 会判定 PATCHED）。
    open_round(&db, run_id, 1).await;
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        inst_id,
        &patch_payload(),
        sub,
    )
    .await
    .expect("patch");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );
    backdate_patch_applied(&db, inst_id, 30).await;

    let result = evaluation::all_check(&db, &docker, run_id, inst_id, sub)
        .await
        .expect("all check");
    assert_eq!(result.status, AwdpEvaluationStatus::Patched, "PATCHED");
    assert!(result.swept, "swept");
    assert_eq!(result.target_round, 1);
    assert_eq!(result.swept_rounds, 6, "round1 起共 6 轮全部计分");

    let row = run_repo::require_by_id(&db, run_id).await.unwrap();
    assert_eq!(row.phase, AwdpPhase::Ended, "比赛直接结束");
    assert!(row.finished_at.is_some());

    let total = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(total, row.fix_round_score * 6, "6 轮 × +150");
    let history = score_repo::my_history(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(history.len(), 6, "6 条 fix 账本");
    assert!(history.iter().all(|s| s.delta == row.fix_round_score));

    // 全部 6 个回合评估 = Patched（含当前轮真实判定详情）。
    let evals = evaluation_repo::list_for_run(&db, run_id).await.unwrap();
    assert_eq!(evals.len(), 6, "6 条 official 评估");
    assert!(
        evals
            .iter()
            .all(|e| e.status == AwdpEvaluationStatus::Patched),
        "{:?}",
        evals.iter().map(|e| e.status.clone()).collect::<Vec<_>>()
    );

    // 实例已停止（ALL Check 内停止）。
    let (inst, _) =
        floatctf::modules::event::awdp::repo::instance_repo::find_by_instance_id(&db, inst_id)
            .await
            .unwrap();
    assert_eq!(inst.runtime_state, "stopped", "实例已停止");

    // 已结束 → 再次 ALL Check 被拒。
    let err = evaluation::all_check(&db, &docker, run_id, inst_id, sub)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Fix 阶段"),
        "ended run 拒绝: {err}"
    );

    cleanup(&db).await;
}

#[tokio::test]
async fn all_check_failure_persists_nothing() {
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
    cleanup(&db).await;

    let user_id = seed_user(&db, "allfail").await;
    let gb_id =
        seed_trainable_gamebox(&db, "allfail", JUDGE_ALWAYS_PASS, EXPLOIT_ALWAYS_SUCCESS).await;
    let (run_id, inst_id) =
        seed_practice_fix_with_instance(&db, &docker, user_id, gb_id, "fail").await;
    let sub = runtime::Subject::user(user_id);

    // 未提交 patch → ALL Check 判定 NO_PATCH（失败）：不落账、不扣分、不写评估。
    open_round(&db, run_id, 1).await;
    let result = evaluation::all_check(&db, &docker, run_id, inst_id, sub)
        .await
        .expect("all check");
    assert_eq!(result.status, AwdpEvaluationStatus::NoPatch);
    assert!(!result.swept, "未 sweep");

    let row = run_repo::require_by_id(&db, run_id).await.unwrap();
    assert_eq!(row.phase, AwdpPhase::Fix, "run 仍 Fix，等官方 check");
    let total = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(total, 0, "不扣分");
    let evals = evaluation_repo::list_for_run(&db, run_id).await.unwrap();
    assert!(
        evals.is_empty(),
        "不写评估行（官方 check 会在 cutoff 独立物化）"
    );

    let _ = runtime::stop_instance(&db, &docker, inst_id, sub).await;
    cleanup(&db).await;
}

#[tokio::test]
async fn manual_test_check_practice_runs_exploit_and_marks_vulnerable() {
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
    cleanup(&db).await;

    let user_id = seed_user(&db, "mchk").await;
    let gb_id =
        seed_trainable_gamebox(&db, "mchk", JUDGE_ALWAYS_PASS, EXPLOIT_ALWAYS_SUCCESS).await;
    let (run_id, inst_id) =
        seed_practice_fix_with_instance(&db, &docker, user_id, gb_id, "mchk").await;
    let sub = runtime::Subject::user(user_id);

    // 先 apply patch（health/judge 均 PASS；exploit 恒成功 → Vulnerable）。
    open_round(&db, run_id, 1).await;
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        inst_id,
        &patch_payload(),
        sub,
    )
    .await
    .expect("patch");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );

    // Test Check 同步执行（worker 不领 manual）：healthcheck + judge + exploit。
    let ev = evaluation::manual_check_enqueue(&db, run_id, inst_id, sub)
        .await
        .expect("enqueue manual");
    let result = evaluation::manual_check_run_now(&db, &docker, &ev)
        .await
        .expect("manual check run now");
    assert_eq!(
        result.exploit_ok,
        Some(true),
        "exploit 成功 → exploit_ok=true（仍可利用）"
    );
    assert!(
        result.exploit_detail.is_some(),
        "exploit 详情随同步结果返回: {:?}",
        result.exploit_detail
    );
    let fresh = evaluation_repo::find_by_id(&db, ev.id).await.unwrap();
    assert_eq!(
        fresh.status,
        AwdpEvaluationStatus::Vulnerable,
        "exploit 成功 → manual Vulnerable"
    );
    assert!(
        fresh.exploit_result.is_some(),
        "exploit 详情已落评估行: {:?}",
        fresh.exploit_result
    );
    // 同步路径写终态后不得持有 lease（约束：终态行 lease_token_hash 必须为空）。
    assert!(
        fresh.lease_token_hash.is_none(),
        "终态行不得持有 lease（lease_consistency_check）"
    );
    // manual 不计分。
    let total = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(total, 0, "manual 不计分");

    let _ = runtime::stop_instance(&db, &docker, inst_id, sub).await;
    cleanup(&db).await;
}
