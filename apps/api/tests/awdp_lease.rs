//! AWDP 评估 Pull + Lease 集成测试（DB-gated，无 Docker）。
//!
//! 覆盖（plan §65）：claim 一次 / 并发 worker 不能抢同一 lease / SKIP LOCKED 分发 /
//! heartbeat 延长 lease / 错误 token 拒绝 / lease 过期重领 / 旧 worker 晚结果拒绝 /
//! attempt 递增 / max_attempts → PLATFORM_ERROR / release_or_fail 重试语义。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
};
use std::path::Path;
use uuid::Uuid;

use floatctf::entity::{
    awdp_evaluations, awdp_fix_rounds, events, gameboxes,
    sea_orm_active_enums::{
        AwdpEvaluationKind, AwdpEvaluationStatus, EventFamily, EventPurpose, ParticipantMode,
    },
};
use floatctf::modules::event::awdp::{
    domain::AwdpConfig,
    repo::{evaluation_repo, event_repo, instance_repo, run_repo},
};

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const WORKER_A: &str = "lease-worker-a";
const WORKER_B: &str = "lease-worker-b";

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
        .filter(events::Column::Title.like("awdp-it-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = events::Entity::delete_by_id(row.id).exec(db).await;
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

async fn seed_gamebox(db: &sea_orm::DatabaseConnection, tag: &str) -> gameboxes::Model {
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
        spec_json: Set(Some(serde_json::json!({}))),
        spec_digest: Set(Some("spec".into())),
        package_digest: Set(Some("pkg".into())),
        image_ref: Set(Some("img".into())),
        image_id: Set(Some("id".into())),
        recommended_cpu_millis: Set(1000),
        recommended_memory_bytes: Set(512 * 1024 * 1024),
        recommended_pids_limit: Set(100),
        healthchecks_json: Set(Some(serde_json::json!([]))),
        build_status: Set(Some("ready".into())),
        awdp_source_code_dir: Set(Some("/srv".into())),
        awdp_exploit_script_name: Set(Some("exploit.py".into())),
        awdp_exploit_script_content: Set(Some("print()".into())),
        awdp_source_artifact_key: Set(Some(format!("gameboxes/{gb_id}/awdp/pkg/source.zip"))),
        awdp_source_artifact_digest: Set(Some("deadbeef".into())),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

/// 建立完整 FK 链：event → run → gamebox → instance → round。
/// 返回 (run_id, instance_id, round_id, user_id, gamebox_id)。
async fn seed_env(db: &sea_orm::DatabaseConnection, tag: &str) -> (Uuid, Uuid, Uuid, Uuid, Uuid) {
    let user_id = seed_user(db, tag).await;
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

    let gb = seed_gamebox(db, tag).await;

    let (instance, _ext) = instance_repo::create_instance(
        db,
        run.id,
        gb.id,
        Some(user_id),
        None,
        &format!("awdp-it-{tag}-{}", &Uuid::new_v4().to_string()[..12]),
        "img",
    )
    .await
    .unwrap();

    let round = awdp_fix_rounds::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run.id),
        sequence: Set(1),
        starts_at: Set((base - chrono::Duration::minutes(5)).into()),
        cutoff_at: Set(base.into()),
        status: Set("evaluating".to_string()),
        created_at: Set(base.into()),
        updated_at: Set(base.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();

    (run.id, instance.id, round.id, user_id, gb.id)
}

async fn create_official(
    db: &sea_orm::DatabaseConnection,
    run_id: Uuid,
    instance_id: Uuid,
    round_id: Uuid,
) -> awdp_evaluations::Model {
    evaluation_repo::create_official(db, run_id, instance_id, round_id)
        .await
        .unwrap()
}

async fn eval_by_id(db: &sea_orm::DatabaseConnection, id: Uuid) -> awdp_evaluations::Model {
    evaluation_repo::find_by_id(db, id).await.unwrap()
}

/// pending 评估只能被 claim 一次；第二个 worker 领不到同一 lease。
#[tokio::test]
async fn claim_is_exclusive() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, round, _u, _gb) = seed_env(&db, "claim-excl").await;
    let ev = create_official(&db, run, inst, round).await;

    let jobs_a =
        evaluation_repo::claim_jobs(&db, WORKER_A, 10, 120, 3, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();
    assert_eq!(jobs_a.len(), 1);
    assert_eq!(jobs_a[0].evaluation.id, ev.id);
    assert_eq!(jobs_a[0].attempt, 1);
    assert!(!jobs_a[0].lease_token.is_empty(), "lease token 非空");

    // 第二个 worker 领不到（status 已 running）。
    let jobs_b =
        evaluation_repo::claim_jobs(&db, WORKER_B, 10, 120, 3, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();
    assert!(jobs_b.is_empty(), "second worker must not claim same lease");

    let fresh = eval_by_id(&db, ev.id).await;
    assert_eq!(fresh.status, AwdpEvaluationStatus::Running);
    assert_eq!(fresh.claimed_by.as_deref(), Some(WORKER_A));
    assert_eq!(fresh.attempt_count, 1);
    assert!(fresh.lease_token_hash.is_some());
    assert!(fresh.lease_expires_at.is_some());
    assert!(fresh.heartbeat_at.is_some());
    cleanup(&db).await;
}

/// SKIP LOCKED：两个 worker 并发领取分发到不同 job（2 instance × 2 round = 4 条互异评估）。
#[tokio::test]
async fn concurrent_claims_distribute_disjoint() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, round, user, _gb) = seed_env(&db, "claim-dist").await;

    // 第二个 gamebox + instance（不同 gamebox 才能建第二个 instance——partial unique）。
    let gb2 = seed_gamebox(&db, "claim-dist-2").await;
    let (inst2, _e2) = instance_repo::create_instance(
        &db,
        run,
        gb2.id,
        Some(user),
        None,
        &format!("awdp-it-dist-{}", &Uuid::new_v4().to_string()[..12]),
        "img",
    )
    .await
    .expect("second instance");
    let round2 = awdp_fix_rounds::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run),
        sequence: Set(2),
        starts_at: Set((chrono::Utc::now() - chrono::Duration::minutes(5)).into()),
        cutoff_at: Set(chrono::Utc::now().into()),
        status: Set("evaluating".to_string()),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // 4 条互异 (round, instance) 评估（2 instance × 2 round）。
    for (iid, rid) in [
        (inst, round),
        (inst, round2.id),
        (inst2.id, round),
        (inst2.id, round2.id),
    ] {
        create_official(&db, run, iid, rid).await;
    }

    let jobs_a =
        evaluation_repo::claim_jobs(&db, WORKER_A, 10, 30, 3, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();
    let jobs_b =
        evaluation_repo::claim_jobs(&db, WORKER_B, 10, 30, 3, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();

    let ids_a: Vec<Uuid> = jobs_a.iter().map(|j| j.evaluation.id).collect();
    let ids_b: Vec<Uuid> = jobs_b.iter().map(|j| j.evaluation.id).collect();
    assert!(
        ids_a.iter().all(|id| !ids_b.contains(id)),
        "A/B 领取互斥（SKIP LOCKED）"
    );
    assert_eq!(ids_a.len() + ids_b.len(), 4, "全部评估被领走");

    // 领到的 job 有 lease token，状态 running，claimed_by 为对应 worker。
    for job in jobs_a.iter().chain(jobs_b.iter()) {
        let fresh = eval_by_id(&db, job.evaluation.id).await;
        assert_eq!(fresh.status, AwdpEvaluationStatus::Running);
        assert!(fresh.claimed_by.is_some(), "claimed_by 必须非空");
        assert!(
            fresh.lease_token_hash.is_some(),
            "lease_token_hash 必须非空"
        );
    }
    cleanup(&db).await;
}

/// heartbeat：正确 token 延长 lease；错误 token 拒绝。
#[tokio::test]
async fn heartbeat_extends_lease_and_rejects_wrong_token() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, round, _u, _gb) = seed_env(&db, "hb").await;
    let ev = create_official(&db, run, inst, round).await;

    let jobs =
        evaluation_repo::claim_jobs(&db, WORKER_A, 10, 30, 3, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();
    let job = &jobs[0];
    let token = job.lease_token.clone();
    let lease_before = eval_by_id(&db, ev.id).await.lease_expires_at.unwrap();

    // 错误 token → NoLease。
    let bad = evaluation_repo::heartbeat(&db, ev.id, WORKER_A, "wrong-token", 30)
        .await
        .unwrap();
    assert_eq!(bad, evaluation_repo::HeartbeatOutcome::NoLease);
    // 错误 worker → NoLease。
    let bad_worker = evaluation_repo::heartbeat(&db, ev.id, WORKER_B, &token, 30)
        .await
        .unwrap();
    assert_eq!(bad_worker, evaluation_repo::HeartbeatOutcome::NoLease);
    // 正确 token → Ok，lease 延长。
    let ok = evaluation_repo::heartbeat(&db, ev.id, WORKER_A, &token, 30)
        .await
        .unwrap();
    assert_eq!(ok, evaluation_repo::HeartbeatOutcome::Ok);
    let lease_after = eval_by_id(&db, ev.id).await.lease_expires_at.unwrap();
    assert!(
        lease_after.with_timezone(&chrono::Utc) > lease_before.with_timezone(&chrono::Utc),
        "heartbeat 必须延长 lease"
    );
    cleanup(&db).await;
}

/// lease 过期 → 回收重领；旧 worker 晚结果拒绝；attempt 递增；新结果生效。
#[tokio::test]
async fn expired_lease_reclaimed_and_stale_result_rejected() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, round, _u, _gb) = seed_env(&db, "stale").await;
    let ev = create_official(&db, run, inst, round).await;

    // worker A claim（lease 1s）。
    let jobs_a =
        evaluation_repo::claim_jobs(&db, WORKER_A, 10, 1, 5, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();
    let job_a = &jobs_a[0];
    assert_eq!(job_a.attempt, 1);

    // 等 lease 过期 → worker B 重领（attempt=2）。
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let jobs_b =
        evaluation_repo::claim_jobs(&db, WORKER_B, 10, 30, 5, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();
    assert_eq!(jobs_b.len(), 1, "过期 lease 必须被回收重领");
    assert_eq!(jobs_b[0].evaluation.id, ev.id, "重领的是同一条评估");
    assert_eq!(jobs_b[0].attempt, 2, "attempt 递增");
    let fresh = eval_by_id(&db, ev.id).await;
    assert_eq!(fresh.attempt_count, 2);

    // 旧 worker A 晚结果（旧 token + attempt=1）→ StaleRejected。
    let stale = evaluation_repo::finish_with_lease(
        &db,
        ev.id,
        WORKER_A,
        &job_a.lease_token,
        1,
        AwdpEvaluationStatus::Patched,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(stale, evaluation_repo::FinishOutcome::StaleRejected);
    let fresh = eval_by_id(&db, ev.id).await;
    assert_eq!(
        fresh.status,
        AwdpEvaluationStatus::Running,
        "stale 结果不得覆盖"
    );

    // 新 worker B 结果 → Ok。
    let ok = evaluation_repo::finish_with_lease(
        &db,
        ev.id,
        WORKER_B,
        &jobs_b[0].lease_token,
        2,
        AwdpEvaluationStatus::Vulnerable,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(ok, evaluation_repo::FinishOutcome::Ok);
    let fresh = eval_by_id(&db, ev.id).await;
    assert_eq!(fresh.status, AwdpEvaluationStatus::Vulnerable);
    assert!(fresh.lease_token_hash.is_none(), "终态必须释放 lease");
    cleanup(&db).await;
}

/// 错误 token 的 result 拒绝。
#[tokio::test]
async fn wrong_token_result_rejected() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, round, _u, _gb) = seed_env(&db, "wrt").await;
    let ev = create_official(&db, run, inst, round).await;
    let jobs =
        evaluation_repo::claim_jobs(&db, WORKER_A, 10, 30, 3, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();

    let out = evaluation_repo::finish_with_lease(
        &db,
        ev.id,
        WORKER_A,
        "wrong-token",
        jobs[0].attempt,
        AwdpEvaluationStatus::Patched,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(out, evaluation_repo::FinishOutcome::StaleRejected);
    let fresh = eval_by_id(&db, ev.id).await;
    assert_eq!(fresh.status, AwdpEvaluationStatus::Running);
    cleanup(&db).await;
}

/// max_attempts：超过后终态 PLATFORM_ERROR，不再重领。
#[tokio::test]
async fn max_attempts_terminal_platform_error() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, round, _u, _gb) = seed_env(&db, "maxa").await;
    let ev = create_official(&db, run, inst, round).await;

    // max_attempts=3：三次 lease 过期 → 三次重领（attempt 1,2,3）→ 第四次回收时终态。
    let mut last_attempt = 0;
    for i in 1..=3 {
        let jobs =
            evaluation_repo::claim_jobs(&db, WORKER_A, 10, 1, 3, &[AwdpEvaluationKind::Official])
                .await
                .unwrap();
        assert_eq!(jobs.len(), 1, "第 {i} 次重领应成功");
        last_attempt = jobs[0].attempt;
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    }
    assert_eq!(last_attempt, 3, "attempt 递增到 3");

    // 第 4 次 claim：回收时 attempt>=max → 终态 PLATFORM_ERROR，无 job 返回。
    let jobs =
        evaluation_repo::claim_jobs(&db, WORKER_A, 10, 30, 3, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();
    assert!(jobs.is_empty());
    let fresh = eval_by_id(&db, ev.id).await;
    assert_eq!(
        fresh.status,
        AwdpEvaluationStatus::PlatformError,
        "超过 max_attempts → PLATFORM_ERROR"
    );
    assert!(fresh.finished_at.is_some());
    assert!(fresh.lease_token_hash.is_none());

    // 后续 claim 不再返回（终态）。
    let jobs2 =
        evaluation_repo::claim_jobs(&db, WORKER_A, 10, 30, 3, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();
    assert!(jobs2.is_empty());
    cleanup(&db).await;
}

/// release_or_fail：基础设施失败 → 释放重试，达 max_attempts → 终态 PLATFORM_ERROR。
#[tokio::test]
async fn release_or_fail_retries_then_terminal() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, round, _u, _gb) = seed_env(&db, "rof").await;
    let ev = create_official(&db, run, inst, round).await;

    // attempt 1 → 失败 → 释放回 pending（attempt 保留）。
    let j1 = evaluation_repo::claim_jobs(&db, WORKER_A, 10, 30, 3, &[AwdpEvaluationKind::Official])
        .await
        .unwrap();
    assert_eq!(j1[0].attempt, 1);
    let out = evaluation_repo::release_or_fail(
        &db,
        ev.id,
        WORKER_A,
        &j1[0].lease_token,
        1,
        3,
        "script timeout",
    )
    .await
    .unwrap();
    assert_eq!(out, evaluation_repo::FinishOutcome::Ok);
    let fresh = eval_by_id(&db, ev.id).await;
    assert_eq!(
        fresh.status,
        AwdpEvaluationStatus::Pending,
        "基础设施失败 → 释放重试"
    );
    assert_eq!(fresh.attempt_count, 1, "attempt_count 保留");
    assert!(
        fresh.stdout_limited.is_some(),
        "失败原因记录在 stdout_limited"
    );

    // attempt 2 → 失败 → 释放。
    let j2 = evaluation_repo::claim_jobs(&db, WORKER_A, 10, 30, 3, &[AwdpEvaluationKind::Official])
        .await
        .unwrap();
    assert_eq!(j2[0].attempt, 2);
    let out =
        evaluation_repo::release_or_fail(&db, ev.id, WORKER_A, &j2[0].lease_token, 2, 3, "boom")
            .await
            .unwrap();
    assert_eq!(out, evaluation_repo::FinishOutcome::Ok);

    // attempt 3（=max）→ 失败 → 终态 PLATFORM_ERROR。
    let j3 = evaluation_repo::claim_jobs(&db, WORKER_A, 10, 30, 3, &[AwdpEvaluationKind::Official])
        .await
        .unwrap();
    assert_eq!(j3[0].attempt, 3);
    let out =
        evaluation_repo::release_or_fail(&db, ev.id, WORKER_A, &j3[0].lease_token, 3, 3, "boom")
            .await
            .unwrap();
    assert_eq!(out, evaluation_repo::FinishOutcome::Ok);
    let fresh = eval_by_id(&db, ev.id).await;
    assert_eq!(fresh.status, AwdpEvaluationStatus::PlatformError);
    assert!(fresh.finished_at.is_some());
    cleanup(&db).await;
}

/// manual 评估：平台 worker 不领（kind 过滤）；Test Check 同步流程不被 worker 抢走。
#[tokio::test]
async fn in_process_worker_only_claims_official() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, _round, _u, _gb) = seed_env(&db, "kindf").await;
    let manual = evaluation_repo::create_manual(&db, run, inst)
        .await
        .unwrap();

    let jobs =
        evaluation_repo::claim_jobs(&db, WORKER_A, 10, 30, 3, &[AwdpEvaluationKind::Official])
            .await
            .unwrap();
    assert!(jobs.is_empty(), "official-only claim 不得领取 manual");
    let fresh = eval_by_id(&db, manual.id).await;
    assert_eq!(fresh.status, AwdpEvaluationStatus::Pending);
    cleanup(&db).await;
}

// ────────────────────────────────────────────────────────────────────────────
// Internal API 鉴权契约（新增 claim/heartbeat/result 端点必须服务身份认证）
// ────────────────────────────────────────────────────────────────────────────

/// 源码扫描断言：awdp internal 路由 handler 首参必须是 `PracticeJudgeInternalAuth`。
#[test]
fn every_awdp_internal_route_requires_internal_auth() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/modules/event/awdp/api/internal.rs");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let lines: Vec<&str> = src.lines().collect();

    let mut checked = 0usize;
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i].trim();
        let trimmed = line.trim_start_matches("#[actix_web::");
        let is_route = line.starts_with("#[post(")
            || line.starts_with("#[get(")
            || trimmed.starts_with("post(")
            || trimmed.starts_with("get(");
        if !is_route {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < lines.len() && (lines[j].trim().starts_with("///") || lines[j].trim().is_empty())
        {
            j += 1;
        }
        let sig = lines
            .get(j)
            .unwrap_or_else(|| panic!("no handler after route attr at line {}", i + 1))
            .trim();
        assert!(
            sig.starts_with("pub async fn"),
            "route at line {} must be followed by `pub async fn`, got: {sig}",
            i + 1
        );
        let mut body = String::new();
        let mut k = j;
        loop {
            body.push_str(lines[k]);
            body.push('\n');
            if lines[k].contains(") ->") || lines[k].contains(')') && lines[k].trim().ends_with(')')
            {
                break;
            }
            k += 1;
        }
        let start = body.find('(').expect("handler signature must have params");
        let body = &body[start + 1..];
        let params = body.split(')').next().unwrap_or("").trim();
        let first_param = params.split(',').next().map(|p| p.trim()).unwrap_or("");
        assert!(
            first_param.starts_with("auth: PracticeJudgeInternalAuth")
                || first_param.starts_with("_auth: PracticeJudgeInternalAuth"),
            "route at line {} (handler `{}`) first param must be `PracticeJudgeInternalAuth`, got `{first_param}`",
            i + 1,
            lines[j].trim()
        );
        checked += 1;
        i = k + 1;
    }

    assert!(
        checked >= 3,
        "expected at least 3 awdp internal routes (claim / heartbeat / result), found {checked}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// §39/§40 Round 原子物化 + 完成判定
// ────────────────────────────────────────────────────────────────────────────

/// 原子物化：单事务内 lock+CAS+物化+expected 快照；重复调用幂等；
/// complete_finished_rounds 不因 0 条评估假完成。
#[tokio::test]
async fn round_materialization_is_atomic_and_idempotent() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, _round1, user, _gb) = seed_env(&db, "rmat").await;

    // 第二个 instance（不同 gamebox）。
    let gb2 = seed_gamebox(&db, "rmat-2").await;
    let (inst2, _e2) = instance_repo::create_instance(
        &db,
        run,
        gb2.id,
        Some(user),
        None,
        &format!("awdp-it-rmat-{}", &Uuid::new_v4().to_string()[..12]),
        "img",
    )
    .await
    .expect("second instance");

    // 新建一个 pending round（cutoff 已过）。
    let round = awdp_fix_rounds::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run),
        sequence: Set(10),
        starts_at: Set((chrono::Utc::now() - chrono::Duration::minutes(5)).into()),
        cutoff_at: Set((chrono::Utc::now() - chrono::Duration::seconds(1)).into()),
        status: Set("pending".to_string()),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // 原子物化：2 个已启动实例 → 2 条评估 + expected_eval_count=2 + evaluating。
    let created = floatctf::modules::event::awdp::repo::round_repo::materialize_round_atomic(
        &db,
        run,
        round.id,
        chrono::Utc::now(),
    )
    .await
    .unwrap();
    assert_eq!(created, 2);
    let fresh_round = awdp_fix_rounds::Entity::find_by_id(round.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fresh_round.status, "evaluating");
    assert_eq!(fresh_round.expected_eval_count, Some(2));
    assert_eq!(
        awdp_evaluations::Entity::find()
            .filter(awdp_evaluations::Column::FixRoundId.eq(round.id))
            .count(&db)
            .await
            .unwrap(),
        2
    );

    // 重复物化（crash/retry 边界）→ CAS 跳过，返回 0，不重复创建。
    let again = floatctf::modules::event::awdp::repo::round_repo::materialize_round_atomic(
        &db,
        run,
        round.id,
        chrono::Utc::now(),
    )
    .await
    .unwrap();
    assert_eq!(again, 0, "重复 tick 不得重复物化");
    assert_eq!(
        awdp_evaluations::Entity::find()
            .filter(awdp_evaluations::Column::FixRoundId.eq(round.id))
            .count(&db)
            .await
            .unwrap(),
        2
    );

    // 全部评估终态前：round 不得 completed。
    let completed_before =
        floatctf::modules::event::awdp::repo::round_repo::complete_finished_rounds(&db)
            .await
            .unwrap();
    // 可能有其它测试残留 evaluating round —— 只断言本 round 未 completed。
    let round_after = awdp_fix_rounds::Entity::find_by_id(round.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        round_after.status, "evaluating",
        "有 pending 评估时不得完成"
    );
    let _ = completed_before;

    // 全部终态 → completed。
    let evals = awdp_evaluations::Entity::find()
        .filter(awdp_evaluations::Column::FixRoundId.eq(round.id))
        .all(&db)
        .await
        .unwrap();
    for ev in evals {
        evaluation_repo::finish(
            &db,
            ev.id,
            AwdpEvaluationStatus::NoPatch,
            Some("done"),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    }
    floatctf::modules::event::awdp::repo::round_repo::complete_finished_rounds(&db)
        .await
        .unwrap();
    let round_final = awdp_fix_rounds::Entity::find_by_id(round.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(round_final.status, "completed");

    cleanup(&db).await;
}

/// 0 条评估不得假完成：pending round 未物化时，complete_finished_rounds 不动它。
#[tokio::test]
async fn no_zero_eval_fake_completion() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, _inst, _round, _u, _gb) = seed_env(&db, "zeroe").await;

    // 直接置 evaluating、不物化任何评估（模拟旧 crash 窗口数据）。
    let round = awdp_fix_rounds::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run),
        sequence: Set(2),
        starts_at: Set((chrono::Utc::now() - chrono::Duration::minutes(5)).into()),
        cutoff_at: Set((chrono::Utc::now() - chrono::Duration::seconds(1)).into()),
        status: Set("evaluating".to_string()),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let n = floatctf::modules::event::awdp::repo::round_repo::complete_finished_rounds(&db)
        .await
        .unwrap();
    let _ = n;
    let fresh = awdp_fix_rounds::Entity::find_by_id(round.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fresh.status, "evaluating", "0 条评估不得假完成");
    cleanup(&db).await;
}

async fn insert_failed_cron_task(
    db: &sea_orm::DatabaseConnection,
    key: &str,
    cron_expr: Option<&str>,
    enabled: bool,
    protected: bool,
) -> Uuid {
    let now = chrono::Utc::now().into();
    let id = Uuid::new_v4();
    floatctf::entity::scheduled_tasks::ActiveModel {
        id: Set(id),
        group_id: Set(None),
        task_name: Set(format!("it-{key}")),
        description: Set(None),
        task_key: Set(key.to_string()),
        trigger_type: Set("cron".to_string()),
        status: Set("failed".to_string()),
        enabled: Set(enabled),
        protected: Set(protected),
        cron_expr: Set(cron_expr.map(str::to_string)),
        execute_at: Set(None),
        expires_at: Set(None),
        payload: Set(None),
        error_msg: Set(Some("boom".to_string())),
        last_run_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        attempt_count: Set(3),
        max_attempts: Set(3),
        timeout_secs: Set(None),
        last_error: Set(Some("boom".to_string())),
        locked_at: Set(None),
        heartbeat_at: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

async fn sched_task(
    db: &sea_orm::DatabaseConnection,
    id: Uuid,
) -> floatctf::entity::scheduled_tasks::Model {
    floatctf::entity::scheduled_tasks::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .unwrap()
}

/// 平台种子 recurring cron（protected + enabled）失败 → 重启恢复为 pending + 重算 execute_at。
#[tokio::test]
async fn failed_recurring_cron_recovers_on_restart() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let id = insert_failed_cron_task(&db, "awdp.tick", Some("*/10 * * * * *"), true, true).await;

    let recovered =
        floatctf::scheduler::engine::recover_recurring_task(&db, sched_task(&db, id).await)
            .await
            .unwrap();
    assert!(recovered, "protected+enabled cron 必须恢复");
    let task = sched_task(&db, id).await;
    assert_eq!(task.status, "pending");
    assert_eq!(task.attempt_count, 0, "attempt 状态重置");
    assert!(task.last_error.is_none(), "错误清空");
    assert!(task.error_msg.is_none());
    let next = task.execute_at.unwrap().with_timezone(&chrono::Utc);
    assert!(next > chrono::Utc::now(), "execute_at 重算到未来");

    cleanup(&db).await;
}

/// 非法 cron 表达式 / 无 cron_expr → 不恢复（保持 failed 供人工排查）。
#[tokio::test]
async fn failed_cron_with_bad_expr_not_recovered() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let bad = insert_failed_cron_task(&db, "awdp.tick", Some("not-a-cron"), true, true).await;
    let none = insert_failed_cron_task(&db, "awdp.eval.worker", None, true, true).await;

    let ok_bad =
        floatctf::scheduler::engine::recover_recurring_task(&db, sched_task(&db, bad).await)
            .await
            .unwrap();
    assert!(!ok_bad, "非法 cron 表达式不恢复");
    let ok_none =
        floatctf::scheduler::engine::recover_recurring_task(&db, sched_task(&db, none).await)
            .await
            .unwrap();
    assert!(!ok_none, "无 cron_expr 不恢复");

    assert_eq!(sched_task(&db, bad).await.status, "failed");
    assert_eq!(sched_task(&db, none).await.status, "failed");
    cleanup(&db).await;
}

/// init_and_recover 的 SQL 过滤：只选 trigger=cron + protected + enabled 的 failed 行
/// （被禁用/非 protected 的任务不复活）。
#[tokio::test]
async fn failed_recovery_filter_skips_disabled_and_unprotected() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    use floatctf::entity::scheduled_tasks;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let id_ok = insert_failed_cron_task(&db, "awdp.tick", Some("*/10 * * * * *"), true, true).await;
    let id_disabled =
        insert_failed_cron_task(&db, "awdp.eval.worker", Some("*/3 * * * * *"), false, true).await;
    let id_unprotected = insert_failed_cron_task(
        &db,
        "awdp.practice.judge",
        Some("*/30 * * * * *"),
        true,
        false,
    )
    .await;

    // 复刻 init_and_recover 的过滤查询。
    let rows = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::Status.eq("failed"))
        .filter(scheduled_tasks::Column::TriggerType.eq("cron"))
        .filter(scheduled_tasks::Column::Protected.eq(true))
        .filter(scheduled_tasks::Column::Enabled.eq(true))
        .all(&db)
        .await
        .unwrap();
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    assert!(ids.contains(&id_ok), "protected+enabled 命中");
    assert!(!ids.contains(&id_disabled), "禁用任务不复活");
    assert!(!ids.contains(&id_unprotected), "非 protected 任务不复活");

    cleanup(&db).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §43 Patch stale applying 回收 + §45 APPLIED-AT cutoff 资格
// ────────────────────────────────────────────────────────────────────────────

/// recover_stale_applying：apply_started_at 早于阈值 → failed + reason；近期的不动。
#[tokio::test]
async fn stale_applying_recovered_not_silently_applied() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, round, user, _gb) = seed_env(&db, "stale").await;

    let now = chrono::Utc::now();
    let fresh = floatctf::entity::awdp_patch_submissions::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run),
        instance_id: Set(inst),
        fix_round_id: Set(Some(round)),
        user_id: Set(Some(user)),
        team_id: Set(None),
        script_sha256: Set("sha".into()),
        script_content: Set("#!/bin/sh\nexit 0\n".into()),
        status: Set("applying".to_string()),
        submitted_at: Set(now.into()),
        apply_started_at: Set(Some(now.into())),
        ..Default::default()
    };
    fresh.insert(&db).await.unwrap();
    let mut stale = floatctf::entity::awdp_patch_submissions::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run),
        instance_id: Set(inst),
        fix_round_id: Set(Some(round)),
        user_id: Set(Some(user)),
        team_id: Set(None),
        script_sha256: Set("sha2".into()),
        script_content: Set("#!/bin/sh\nexit 1\n".into()),
        status: Set("applying".to_string()),
        submitted_at: Set((now - chrono::Duration::minutes(10)).into()),
        apply_started_at: Set(Some((now - chrono::Duration::minutes(10)).into())),
        ..Default::default()
    };
    let stale_id = stale.id.clone().unwrap();
    stale.insert(&db).await.unwrap();

    let n = floatctf::modules::event::awdp::repo::patch_repo::recover_stale_applying(
        &db,
        inst,
        now - chrono::Duration::seconds(90),
    )
    .await
    .unwrap();
    assert_eq!(n, 1, "只有超时的一条被回收");
    let row = floatctf::entity::awdp_patch_submissions::Entity::find_by_id(stale_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "failed");
    assert!(
        row.error_message
            .as_deref()
            .unwrap_or("")
            .contains("stale applying recovered"),
        "stale 回收必须带 reason"
    );
    // 近期 applying 不受影响（不被静默视为 APPLIED）。
    let applying_count = floatctf::entity::awdp_patch_submissions::Entity::find()
        .filter(floatctf::entity::awdp_patch_submissions::Column::InstanceId.eq(inst))
        .filter(floatctf::entity::awdp_patch_submissions::Column::Status.eq("applying"))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(applying_count, 1);
    cleanup(&db).await;
}

/// APPLIED-AT 资格（§45）：applied_at <= cutoff 才属于该 Turn。
#[tokio::test]
async fn patch_eligibility_uses_applied_at_cutoff() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let (run, inst, round, user, _gb) = seed_env(&db, "cutoff").await;
    let now = chrono::Utc::now();

    let mk = |id: Uuid, applied: chrono::DateTime<chrono::Utc>| {
        floatctf::entity::awdp_patch_submissions::ActiveModel {
            id: Set(id),
            run_id: Set(run),
            instance_id: Set(inst),
            fix_round_id: Set(Some(round)),
            user_id: Set(Some(user)),
            team_id: Set(None),
            script_sha256: Set("sha".into()),
            script_content: Set("#!/bin/sh\nexit 0\n".into()),
            status: Set("applied".to_string()),
            submitted_at: Set(now.into()),
            apply_started_at: Set(Some(now.into())),
            applied_at: Set(Some(applied.into())),
            ..Default::default()
        }
    };

    // cutoff 在 5 分钟前；一个 patch applied 在 cutoff 前（eligible），一个在 cutoff 后（不 eligible）。
    // 先调整 round cutoff 到过去。
    let cutoff_before = now - chrono::Duration::minutes(5);
    let r = awdp_fix_rounds::Entity::find_by_id(round)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut am: awdp_fix_rounds::ActiveModel = r.into();
    am.cutoff_at = Set(cutoff_before.into());
    am.update(&db).await.unwrap();

    mk(Uuid::new_v4(), cutoff_before - chrono::Duration::minutes(1))
        .insert(&db)
        .await
        .unwrap();
    mk(Uuid::new_v4(), cutoff_before + chrono::Duration::minutes(1))
        .insert(&db)
        .await
        .unwrap();

    let eligible =
        floatctf::modules::event::awdp::repo::patch_repo::has_applied_patch(&db, inst, round)
            .await
            .unwrap();
    assert!(eligible, "cutoff 前 APPLIED 的 patch 属于该 Turn");

    // 只留 cutoff 后的 patch → 不 eligible。
    floatctf::entity::awdp_patch_submissions::Entity::delete_many()
        .filter(floatctf::entity::awdp_patch_submissions::Column::FixRoundId.eq(round))
        .exec(&db)
        .await
        .unwrap();
    mk(Uuid::new_v4(), cutoff_before + chrono::Duration::minutes(1))
        .insert(&db)
        .await
        .unwrap();
    let eligible2 =
        floatctf::modules::event::awdp::repo::patch_repo::has_applied_patch(&db, inst, round)
            .await
            .unwrap();
    assert!(
        !eligible2,
        "cutoff 后才 APPLIED（即使提交早于 cutoff）不属该 Turn"
    );
    cleanup(&db).await;
}

// ────────────────────────────────────────────────────────────────────────────
// §41/§69 PreparingFix：阶段门禁 + reconcile → Fix
// ────────────────────────────────────────────────────────────────────────────

/// PreparingFix：flag 不可用 / patch 禁止；reconcile（无实例）→ 物化回合 + Fix。
#[tokio::test]
async fn preparing_fix_gates_and_reconcile() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    use floatctf::modules::event::awdp::service::event_service;

    // practice run（Pending → Launch → Break）。
    let user = seed_user(&db, "pfix").await;
    let gb = seed_gamebox(&db, "pfix").await;
    let run = floatctf::modules::event::awdp::service::practice_service::start_training(
        &db,
        &dummy_docker(),
        b"x",
        user,
        gb.id,
        "flag",
    )
    .await
    .unwrap();
    let _ = &run;
    // 不需要 docker：只验证阶段门禁 + reconcile 的 DB 语义。
    let run_row = floatctf::modules::event::awdp::repo::run_repo::require_by_id(&db, run.id)
        .await
        .unwrap();
    assert_eq!(
        run_row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Pending
    );
    floatctf::modules::event::awdp::repo::run_repo::launch_practice_run(&db, run.id)
        .await
        .unwrap();

    // Break → PreparingFix。
    event_service::transition_break_to_preparing_fix(&db, run.id)
        .await
        .unwrap();
    let row = floatctf::modules::event::awdp::repo::run_repo::require_by_id(&db, run.id)
        .await
        .unwrap();
    assert_eq!(
        row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::PreparingFix
    );

    // reconcile（无实例，0 reset 即全部 pristine）→ Fix + 物化回合。
    let ok = event_service::reconcile_preparing_fix(&db, &dummy_docker(), b"x", run.id)
        .await
        .unwrap();
    assert!(ok, "无实例 reconcile 立即完成");
    let row = floatctf::modules::event::awdp::repo::run_repo::require_by_id(&db, run.id)
        .await
        .unwrap();
    assert_eq!(
        row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Fix
    );
    assert!(row.fix_started_at.is_some() && row.fix_ends_at.is_some());
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 6, "reconcile 完成时物化回合");

    // 清理。
    let _ = floatctf::entity::awdp_runs::Entity::delete_by_id(run.id)
        .exec(&db)
        .await;
    cleanup(&db).await;
}

fn dummy_docker() -> bollard::Docker {
    // 未真正连接：reconcile 无实例时不触碰 docker。
    bollard::Docker::connect_with_local_defaults().expect("docker")
}

// ────────────────────────────────────────────────────────────────────────────
// §58 Competition Individual membership 授权
// ────────────────────────────────────────────────────────────────────────────

/// 未加入赛事的用户访问 competition 端点必须 Forbidden；加入后放行。
#[tokio::test]
async fn individual_membership_required_for_competition() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    use floatctf::entity::sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode};
    let base = chrono::Utc::now();
    let event = events::ActiveModel {
        is_virtual: Set(false),
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(None),
        title: Set("awdp-it-membership".to_string()),
        description: Set(None),
        start_time: Set((base - chrono::Duration::hours(1)).into()),
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
    let user_a = seed_user(&db, "member-a").await;
    let user_b = seed_user(&db, "member-b").await;

    // 未加入 → Forbidden。
    let err = floatctf::modules::event::awdp::service::authorization::require_event_participant(
        &db, event.id, user_a,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, floatctf::modules::event::awdp::AwdpError::Forbidden(_)),
        "未加入赛事必须 Forbidden: {err}"
    );

    // user_b 注册（模拟 join_event 写入 event_users）。
    floatctf::entity::event_users::ActiveModel {
        event_id: Set(event.id),
        user_id: Set(user_b),
        points: Set(0.0),
        banned: Set(false),
        joined_at: Set(chrono::Utc::now().into()),
    }
    .insert(&db)
    .await
    .unwrap();
    floatctf::modules::event::awdp::service::authorization::require_event_participant(
        &db, event.id, user_b,
    )
    .await
    .expect("已加入放行");
    let _ = user_a;

    let _ = events::Entity::delete_by_id(event.id).exec(&db).await;
    cleanup(&db).await;
}
