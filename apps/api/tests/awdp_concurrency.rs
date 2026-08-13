//! AWDP 并发互斥集成测试（plan §89）。
//!
//! 覆盖同一 instance 的互斥操作串行化（Patch / Official evaluation / Reset / Manual check
//! 共用 `InstanceAdvisoryLock`，postgres advisory lock 语义）：
//!   - advisory lock 同 key 互斥、不同 key 不互斥（DB 层，无 Docker）；
//!   - patch vs evaluation：prior-round 评估未完成 → patch 拒绝（Conflict），完成后放行；
//!   - reset vs evaluation：并发执行互不踩踏（评估终态 + reset gen+1 + 端点保留）；
//!   - manual check vs evaluation：并发执行互不干扰（manual 不运行 exploit）。

use std::time::Duration;

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    events, gameboxes,
    sea_orm_active_enums::{
        AwdpEvaluationStatus, AwdpPhase, EventFamily, EventPurpose, ParticipantMode,
    },
};
use floatctf::modules::event::awdp::{
    domain::AwdpConfig,
    repo::{evaluation_repo, event_gamebox_repo, event_repo, run_repo, score_repo},
    service::{
        lock::InstanceAdvisoryLock,
        runtime::{self, Subject},
        tick_service,
    },
};

/// pristine reset 后等待 Docker 容器真正进入 running（Docker 状态可能短暂滞后于 DB
/// runtime_state='running'，首个 patch exec 会命中 409 not running 的微竞态）。
async fn wait_container_running(
    docker: &bollard::Docker,
    db: &sea_orm::DatabaseConnection,
    instance_id: Uuid,
) {
    use bollard::query_parameters::InspectContainerOptions;
    for _ in 0..40 {
        let (instance, _) =
            floatctf::modules::event::awdp::repo::instance_repo::find_by_instance_id(
                db,
                instance_id,
            )
            .await
            .expect("instance");
        if let Some(cid) = instance.container_id {
            match docker
                .inspect_container(&cid, None::<InspectContainerOptions>)
                .await
            {
                Ok(info) => {
                    if info.state.as_ref().and_then(|s| s.running).unwrap_or(false) {
                        return;
                    }
                }
                Err(_) => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("container for instance {instance_id} not running within timeout");
}

/// Break 到期 → PreparingFix（tick 1）→ reconcile → Fix（tick 2，游标 now 立即 due）。
async fn tick_to_fix(db: &sea_orm::DatabaseConnection, docker: &bollard::Docker) {
    tick_service::tick_once(db, docker, JWT_SECRET)
        .await
        .expect("tick: break → preparing_fix");
    tick_service::tick_once(db, docker, JWT_SECRET)
        .await
        .expect("tick: preparing_fix reconcile → fix");
}

/// 把该实例最近一次成功 patch 的 applied_at 回拨 seconds 秒（APPLIED-AT：applied_at <= cutoff）。
async fn backdate_patch_applied(
    db: &sea_orm::DatabaseConnection,
    instance_id: Uuid,
    seconds_ago: i64,
) {
    use sea_orm::ActiveModelTrait;
    use sea_orm::QueryOrder;
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

/// 建已开始（Break，break_ends 已过期）的 competition run + 完整 [awdp] GameBox。
/// exploit 恒失败 → 有合法 APPLIED patch 时 PATCHED +score。
async fn seed_run_in_break(db: &sea_orm::DatabaseConnection, tag: &str) -> (Uuid, Uuid, Uuid) {
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
            // 故意远早于现在：并行全量测试时保证本 run 稳定在 find_due_runs
            // 的 10 条批次内（不会因为其他测试二进制并发的 due run 而饿死）。
            next_action_at: Some(now - chrono::Duration::minutes(30)),
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
        safe_name: Set(format!("awdp-it-gb-{tag}-{}", &Uuid::new_v4().to_string()[..8])),
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
        awdp_exploit_script_content: Set(Some(
            r#"import json, sys
print(json.dumps([{"gamebox_ip": ip, "success": False, "error": "attack blocked"} for ip in sys.argv[1:]], ensure_ascii=False))
sys.exit(1)
"#
            .to_string(),
        )),
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

/// 打开指定 sequence 的 round 窗口 + 让 run due。
async fn open_round(db: &sea_orm::DatabaseConnection, run_id: Uuid, sequence: i32) {
    let now = chrono::Utc::now();
    for row in floatctf::entity::awdp_fix_rounds::Entity::find()
        .filter(floatctf::entity::awdp_fix_rounds::Column::RunId.eq(run_id))
        .all(db)
        .await
        .unwrap()
    {
        let mut am: floatctf::entity::awdp_fix_rounds::ActiveModel = row.clone().into();
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

/// 指定 sequence 的 round 已到期（cutoff 过去），run due → tick 可物化评估。
async fn expire_round(db: &sea_orm::DatabaseConnection, run_id: Uuid, sequence: i32) {
    let now = chrono::Utc::now();
    for row in floatctf::entity::awdp_fix_rounds::Entity::find()
        .filter(floatctf::entity::awdp_fix_rounds::Column::RunId.eq(run_id))
        .all(db)
        .await
        .unwrap()
    {
        let mut am: floatctf::entity::awdp_fix_rounds::ActiveModel = row.clone().into();
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
    let row = run_repo::require_by_id(db, run_id).await.unwrap();
    let mut am: floatctf::entity::awdp_runs::ActiveModel = row.into();
    // 用非常旧的 next_action_at，保证并行全量测试时该 run 稳定落入
    // find_due_runs 的 10 条批次（其他测试二进制并发的 due run 不会更旧）。
    am.next_action_at = Set(Some((now - chrono::Duration::minutes(30)).into()));
    am.updated_at = Set(now.into());
    am.update(db).await.unwrap();
}

// ────────────────────────────────────────────────────────────────────────────
// §89：advisory lock 串行化语义（DB 层，无 Docker）
// ────────────────────────────────────────────────────────────────────────────

/// 测试用 AWDP 静态配置（练习子网 / JudgeServer 镜像等）。
fn awdp_config() -> floatctf::core::config::AwdpStaticConfig {
    floatctf::core::config::AwdpStaticConfig {
        practice_judgeserver_image: "floatctf/awdp-judgeserver:latest".to_string(),
        practice_network_subnet: "10.42.2.0/24".to_string(),
        practice_judge_ip: "10.42.2.2".to_string(),
        practice_judge_data_host: "awdp-judge".to_string(),
        platform_internal_url: "http://host.docker.internal:9090".to_string(),
        eval_lease_duration_secs: 120,
        eval_max_attempts: 3,
    }
}

#[tokio::test]
async fn advisory_lock_mutual_exclusion() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let instance_a = Uuid::new_v4();
    let instance_b = Uuid::new_v4();

    // task1：持锁 1.2s 后释放。
    let db1 = db.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let holder = tokio::spawn(async move {
        let lock = InstanceAdvisoryLock::acquire(&db1, instance_a)
            .await
            .expect("acquire A");
        let _ = tx.send(());
        tokio::time::sleep(Duration::from_millis(1200)).await;
        lock.release().await;
    });
    rx.await.expect("task1 holds lock");

    // 不同 instance：task1 仍持锁时立即获得（不同 key 不互斥）。
    let start = std::time::Instant::now();
    let db2 = db.clone();
    let lock_b = InstanceAdvisoryLock::acquire(&db2, instance_b)
        .await
        .expect("acquire B");
    let elapsed_b = start.elapsed();
    lock_b.release().await;
    assert!(
        elapsed_b.as_millis() < 800,
        "不同 instance 不应被阻塞 (elapsed={elapsed_b:?})"
    );

    // 同一 instance：必须等到 task1 释放（串行化）。
    let start = std::time::Instant::now();
    let db3 = db.clone();
    let lock_a = InstanceAdvisoryLock::acquire(&db3, instance_a)
        .await
        .expect("acquire A again");
    let elapsed_a = start.elapsed();
    lock_a.release().await;
    assert!(
        elapsed_a.as_millis() >= 900,
        "同一 instance 必须互斥等待 (elapsed={elapsed_a:?})"
    );

    holder.await.expect("holder task");
}

// ────────────────────────────────────────────────────────────────────────────
// §89：patch vs official evaluation —— prior-round 评估未完成时 patch 拒绝
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn patch_rejected_while_prior_round_eval_unfinished() {
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

    let (event_id, run_id, gb_id) = seed_run_in_break(&db, "pe").await;
    let user_id = seed_user(&db, "pe").await;
    let sub = Subject::user(user_id);
    let inst = runtime::start_instance(
        &db,
        &docker,
        JWT_SECRET,
        &awdp_config(),
        run_id,
        gb_id,
        sub,
        "flag",
    )
    .await
    .expect("start");

    // Break → Fix（rounds 物化）。
    tick_to_fix(&db, &docker).await;
    wait_container_running(&docker, &db, inst.instance_id).await;

    // Round 1：apply patch（eligible）→ 到期 → tick 物化 pending 评估（worker 未消费）。
    open_round(&db, run_id, 1).await;
    let r = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        inst.instance_id,
        "#!/bin/sh\nexit 0\n",
        sub,
    )
    .await
    .expect("patch round1");
    assert_eq!(
        r,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );
    expire_round(&db, run_id, 1).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r1");
    let evals = evaluation_repo::list_for_run(&db, run_id).await.unwrap();
    let e1 = evals
        .iter()
        .find(|e| e.kind == floatctf::entity::sea_orm_active_enums::AwdpEvaluationKind::Official)
        .expect("round1 eval materialized");
    assert_eq!(e1.status, AwdpEvaluationStatus::Pending);

    // Round 2：prior-round（round1）评估未完成（pending）→ patch 被拒（§89 串行化）。
    open_round(&db, run_id, 2).await;
    let err = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        inst.instance_id,
        "#!/bin/sh\necho r2 > /tmp/r2\nexit 0\n",
        sub,
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("Round evaluation in progress"),
        "patch vs unfinished eval must conflict: {err}"
    );

    // 评估完成（终态）→ patch 放行。
    evaluation_repo::finish(
        &db,
        e1.id,
        AwdpEvaluationStatus::Patched,
        Some("ok"),
        Some("ok"),
        Some("exploit blocked"),
        None,
        None,
    )
    .await
    .expect("finish round1 eval");
    let r2 = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        inst.instance_id,
        "#!/bin/sh\necho r2 > /tmp/r2\nexit 0\n",
        sub,
    )
    .await
    .expect("patch round2 after eval done");
    assert_eq!(
        r2,
        floatctf::modules::event::awdp::service::patch_service::PatchResult::Applied
    );

    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = runtime::stop_instance(&db, &docker, inst.instance_id, sub).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §89：reset vs official evaluation —— 同一 instance 并发执行互不踩踏
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reset_and_evaluation_concurrent() {
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

    let (event_id, run_id, gb_id) = seed_run_in_break(&db, "re").await;
    let user_id = seed_user(&db, "re").await;
    let sub = Subject::user(user_id);
    let inst = runtime::start_instance(
        &db,
        &docker,
        JWT_SECRET,
        &awdp_config(),
        run_id,
        gb_id,
        sub,
        "flag",
    )
    .await
    .expect("start");
    let original_port = inst.endpoints[0].public_port;

    tick_to_fix(&db, &docker).await;
    wait_container_running(&docker, &db, inst.instance_id).await;
    open_round(&db, run_id, 1).await;
    let _ = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        inst.instance_id,
        "#!/bin/sh\nexit 0\n",
        sub,
    )
    .await
    .expect("patch");
    // APPLIED-AT：回拨 applied_at 到 cutoff 前（expire_round 会把 cutoff 拨到过去）。
    backdate_patch_applied(&db, inst.instance_id, 30).await;
    expire_round(&db, run_id, 1).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r1");
    // 此刻 round1 official eval 为 pending，run due 已清。

    // break→fix 已 reset 过实例（gen 2），并发 reset 应再 +1。
    let before_reset = runtime::get_my_instance_view(&db, run_id, gb_id, sub)
        .await
        .unwrap()
        .expect("view before reset");

    // 并发：worker 消费评估（持 instance 锁）+ 玩家 reset（同一把锁）。
    let db_w = db.clone();
    let docker_w = docker.clone();
    let worker = tokio::spawn(async move {
        floatctf::modules::event::awdp::service::evaluation::worker_round(
            &db_w,
            &docker_w,
            "floatctf-api-worker",
            16,
            120,
            3,
        )
        .await
        .expect("worker")
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let view_after =
        runtime::reset_instance(&db, &docker, JWT_SECRET, inst.instance_id, sub, "flag")
            .await
            .expect("concurrent reset");

    let n = worker.await.expect("worker joined");
    assert!(n >= 1, "worker processed at least our eval");

    // 终态一致：评估落终态、reset 成功 gen+1、端点保留、实例 running。
    let evals = evaluation_repo::list_for_run(&db, run_id).await.unwrap();
    let official: Vec<_> = evals
        .iter()
        .filter(|e| e.kind == floatctf::entity::sea_orm_active_enums::AwdpEvaluationKind::Official)
        .collect();
    assert!(
        official.iter().any(|e| !matches!(
            e.status,
            AwdpEvaluationStatus::Pending | AwdpEvaluationStatus::Running
        )),
        "评估必须进入终态: {:?}",
        official
            .iter()
            .map(|e| e.status.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        view_after.runtime_generation,
        before_reset.runtime_generation + 1,
        "并发 reset gen+1"
    );
    assert_eq!(view_after.runtime_state, "running");
    assert_eq!(view_after.endpoints[0].public_port, original_port);

    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = runtime::stop_instance(&db, &docker, view_after.instance_id, sub).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §89：manual check vs official evaluation —— 并发执行互不干扰
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn manual_check_and_evaluation_concurrent() {
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

    let (event_id, run_id, gb_id) = seed_run_in_break(&db, "mc").await;
    let user_id = seed_user(&db, "mc").await;
    let sub = Subject::user(user_id);
    let inst = runtime::start_instance(
        &db,
        &docker,
        JWT_SECRET,
        &awdp_config(),
        run_id,
        gb_id,
        sub,
        "flag",
    )
    .await
    .expect("start");

    tick_to_fix(&db, &docker).await;
    wait_container_running(&docker, &db, inst.instance_id).await;
    open_round(&db, run_id, 1).await;
    let _ = floatctf::modules::event::awdp::service::patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        inst.instance_id,
        "#!/bin/sh\nexit 0\n",
        sub,
    )
    .await
    .expect("patch");
    // APPLIED-AT：回拨 applied_at 到 cutoff 前（expire_round 会把 cutoff 拨到过去）。
    backdate_patch_applied(&db, inst.instance_id, 30).await;
    expire_round(&db, run_id, 1).await;
    tick_service::tick_once(&db, &docker, JWT_SECRET)
        .await
        .expect("tick r1");

    // 并发：manual check 入队 + worker 消费（同一把 instance 锁；official + manual 互不踩踏）。
    let enqueued = floatctf::modules::event::awdp::service::evaluation::manual_check_enqueue(
        &db,
        run_id,
        inst.instance_id,
        sub,
    )
    .await
    .expect("enqueue manual check");
    let db_w = db.clone();
    let docker_w = docker.clone();
    let worker = tokio::spawn(async move {
        floatctf::modules::event::awdp::service::evaluation::worker_round(
            &db_w,
            &docker_w,
            "floatctf-api-worker",
            16,
            120,
            3,
        )
        .await
        .expect("worker")
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let n = worker.await.expect("worker joined");
    assert!(n >= 1, "worker processed at least our eval");
    let mc = floatctf::modules::event::awdp::repo::evaluation_repo::find_by_id(&db, enqueued.id)
        .await
        .expect("manual eval");
    assert_eq!(mc.status, AwdpEvaluationStatus::Patched, "{mc:?}");

    // manual 评估：只跑 health+judge，不运行 exploit（exploit_result NULL）。
    let evals = evaluation_repo::list_for_run(&db, run_id).await.unwrap();
    let manual = evals
        .iter()
        .find(|e| e.kind == floatctf::entity::sea_orm_active_enums::AwdpEvaluationKind::Manual)
        .expect("manual eval row");
    assert_eq!(manual.status, AwdpEvaluationStatus::Patched);
    assert!(manual.exploit_result.is_none(), "manual 不运行 exploit");

    // official 评估终态（互不踩踏）。
    let official = evals
        .iter()
        .filter(|e| e.kind == floatctf::entity::sea_orm_active_enums::AwdpEvaluationKind::Official)
        .collect::<Vec<_>>();
    assert!(
        official.iter().any(|e| !matches!(
            e.status,
            AwdpEvaluationStatus::Pending | AwdpEvaluationStatus::Running
        )),
        "official eval must reach terminal state"
    );

    // manual check 不计分；official PATCHED 计分一次。
    let total = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    let run_row = run_repo::require_by_id(&db, run_id).await.unwrap();
    assert_eq!(total, run_row.fix_round_score, "只有 official PATCHED 加分");

    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = runtime::stop_instance(&db, &docker, inst.instance_id, sub).await;
}
