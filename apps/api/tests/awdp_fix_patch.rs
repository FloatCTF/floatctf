//! AWDP Break→Fix + Patch 集成测试（DB + Docker gated，run 中心化）。
//!
//! 覆盖（plan §47）：Break 上传拒绝 / Fix 上传接受 / FLOATCTF_SOURCE_DIR /
//! exit 0 applied / 非 0 failed / restart 保留 patch / reset 移除 patch /
//! reset 保留端点 + generation+1 / manual check 不计分 / break→fix 实例 pristine。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, QueryFilter};
use uuid::Uuid;

use fcmc::ContainerRuntime;
use floatctf::entity::{
    events, gameboxes,
    sea_orm_active_enums::{
        AwdpEvaluationKind, AwdpEvaluationStatus, AwdpPhase, EventFamily, EventPurpose,
        ParticipantMode,
    },
};
use floatctf::modules::event::awdp::{
    domain::AwdpConfig,
    repo::{evaluation_repo, event_gamebox_repo, event_repo, patch_repo, run_repo, score_repo},
    service::{
        event_service, patch_service,
        runtime::{self, Subject},
    },
};

/// 构造 PatchPayload：patch.sh（入口脚本）+ 辅助文件（路径由调用方自定）。
fn patch_payload(script: &str, files: &[(&str, &str)]) -> patch_service::PatchPayload {
    patch_service::PatchPayload {
        script: script.to_string(),
        archive_sha256: "a".repeat(64),
        files: files
            .iter()
            .map(|(p, c)| patch_service::PatchFile {
                relative_path: p.to_string(),
                content: c.as_bytes().to_vec(),
            })
            .collect(),
    }
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

/// 建 competition 事件 + active run（pending），返回 (event_id, run_id)。
async fn seed_event_and_run(
    db: &sea_orm::DatabaseConnection,
    tag: &str,
    mode: ParticipantMode,
) -> (Uuid, Uuid) {
    let base = chrono::Utc::now();
    let event = events::ActiveModel {
        is_virtual: Set(false),
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(mode),
        system_key: Set(None),
        title: Set(format!("awdp-it-{tag}")),
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
    event_repo::ensure_by_event_id(db, event.id, &AwdpConfig::default())
        .await
        .unwrap();
    let run = run_repo::create_competition_run(db, event.id, &AwdpConfig::default())
        .await
        .unwrap();
    (event.id, run.id)
}

async fn transition(db: &sea_orm::DatabaseConnection, run_id: Uuid, to: AwdpPhase) {
    let now = chrono::Utc::now();
    match to {
        AwdpPhase::Break => {
            run_repo::transition_phase(
                db,
                run_id,
                AwdpPhase::Pending,
                AwdpPhase::Break,
                run_repo::PhaseTransitionPatch {
                    started_at: Some(now),
                    break_ends_at: Some(now + chrono::Duration::hours(1)),
                    next_action_at: Some(now + chrono::Duration::hours(1)),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        }
        AwdpPhase::Fix => {
            run_repo::transition_phase(
                db,
                run_id,
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
            event_service::transition_break_to_fix(
                db,
                &docker_or_skip().unwrap(),
                JWT_SECRET,
                run_id,
            )
            .await
            .unwrap();
        }
        _ => panic!("unsupported target phase"),
    }
}

async fn seed_gamebox_and_attach(
    db: &sea_orm::DatabaseConnection,
    event_id: Uuid,
    tag: &str,
) -> Uuid {
    let now = chrono::Utc::now().into();
    let gb_id = Uuid::new_v4();
    let gb = gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(format!("awdp-gb-{tag}")),
        safe_name: Set(format!("awdp-it-gb-{tag}-{}", &Uuid::new_v4().to_string()[..8])),
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
            r#"import json, sys, urllib.request
def check(ip):
    try:
        with urllib.request.urlopen(f"http://{ip}/", timeout=5) as r:
            return {"gamebox_ip": ip, "success": r.status == 200, "error": f"HTTP {r.status}" if r.status != 200 else None}
    except Exception as e:
        return {"gamebox_ip": ip, "success": False, "error": str(e)}
ips = sys.argv[1:]
print(json.dumps([check(ip) for ip in ips], ensure_ascii=False))
sys.exit(0 if all(check(ip)["success"] for ip in ips) else 1)
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
        awdp_exploit_script_content: Set(Some("x".into())),
        awdp_source_artifact_key: Set(Some(format!("gameboxes/{gb_id}/awdp/pkg/source.zip"))),
        awdp_source_artifact_digest: Set(Some("deadbeef".into())),
    }
    .insert(db)
    .await
    .unwrap();
    event_gamebox_repo::attach_gamebox(db, event_id, gb.id, false)
        .await
        .expect("attach");
    gb.id
}

/// 测试用 AWDP 静态配置（练习子网 / JudgeServer 镜像等）。
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

/// 移除部署的 JudgeServer 容器（best-effort）：测试用 host.docker.internal env 部署的
/// judge 若残留会污染真实环境（deploy_judge 幂等但带 env 漂移自愈，见 practice_judge.rs）。
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

#[tokio::test]
async fn patch_and_reset_lifecycle() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    remove_judge_container().await;
    let rt = fcmc::DockerContainerRuntime::new(docker.clone());
    if fcmc::ImageRuntime::inspect_image(&rt, IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: image {IMAGE_REF} not present");
        return;
    }

    let (event_id, run_id) = seed_event_and_run(&db, "fix", ParticipantMode::Individual).await;
    let gb_id = seed_gamebox_and_attach(&db, event_id, "fix").await;
    transition(&db, run_id, AwdpPhase::Fix).await;

    let user_id = seed_user(&db, "fix").await;
    let subject = Subject::user(user_id);
    let view = runtime::start_instance(
        &db,
        &docker,
        JWT_SECRET,
        &awdp_config(),
        run_id,
        gb_id,
        subject,
        "flag",
    )
    .await
    .expect("start");
    let original_port = view.endpoints[0].public_port;

    // 1. Fix 阶段上传 patch.tar.gz（patch.sh 把 src/index.php 覆盖进源码目录）→ applied + restart。
    // workdir = 解压目录 /tmp/patch/，脚本内相对路径 `src/index.php` 即可引用包内文件。
    let ok_payload = patch_payload(
        r#"#!/bin/sh
cp src/index.php "$FLOATCTF_SOURCE_DIR/index.php"
echo "patched-ok"
exit 0
"#,
        &[("src/index.php", "<?php echo 'patched-by-awdp'; ?>")],
    );
    let r =
        patch_service::apply_patch(&db, &docker, run_id, view.instance_id, &ok_payload, subject)
            .await
            .expect("apply patch");
    assert_eq!(r, patch_service::PatchResult::Applied);
    let sub = patch_repo::latest_for_instance(&db, view.instance_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sub.status, "applied");
    assert_eq!(sub.exit_code, Some(0));
    assert!(
        sub.stdout_limited
            .as_deref()
            .unwrap_or("")
            .contains("patched-ok"),
        "stdout: {:?}",
        sub.stdout_limited
    );
    assert!(
        sub.script_content.contains("index.php"),
        "manifest: {:?}",
        sub.script_content
    );

    // 2. 失败 patch（exit 1）→ failed（脚本在容器内执行后非零退出）。
    let bad_payload = patch_payload("#!/bin/sh\necho boom >&2\nexit 1\n", &[]);
    let r = patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        view.instance_id,
        &bad_payload,
        subject,
    )
    .await
    .expect("apply bad patch");
    assert!(
        matches!(r, patch_service::PatchResult::Failed(_)),
        "{:?}",
        r
    );
    let sub = patch_repo::latest_for_instance(&db, view.instance_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sub.status, "failed");
    assert_eq!(sub.exit_code, Some(1));
    assert!(
        sub.stderr_limited.as_deref().unwrap_or("").contains("boom"),
        "stderr: {:?}",
        sub.stderr_limited
    );

    // 3. 覆盖 src/index.php（marker 内容）→ restart 保留 → reset → pristine 恢复 + generation+1。
    let marker_payload = patch_payload(
        r#"#!/bin/sh
cp src/index.php "$FLOATCTF_SOURCE_DIR/index.php"
exit 0
"#,
        &[("src/index.php", "<?php echo 'marker'; ?>")],
    );
    let r = patch_service::apply_patch(
        &db,
        &docker,
        run_id,
        view.instance_id,
        &marker_payload,
        subject,
    )
    .await
    .expect("marker patch");
    assert_eq!(r, patch_service::PatchResult::Applied);

    // §83：APPLIED 后同容器 restart（patch_service 内部 restart_container）→ 修改保留。
    let (inst_row, _ext) =
        floatctf::modules::event::awdp::repo::instance_repo::find_by_instance_id(
            &db,
            view.instance_id,
        )
        .await
        .unwrap();
    let kept = rt
        .exec(
            &inst_row.container_name,
            fcmc::ExecOptions {
                cmd: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "cat /var/www/html/index.php".into(),
                ],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(10),
                stdout_limit: 4096,
                stderr_limit: 4096,
                stdin: None,
            },
        )
        .await
        .expect("exec marker check");
    assert_eq!(kept.exit_code, Some(0), "cat index.php");
    assert!(
        kept.stdout.contains("marker"),
        "restart 后 patch 写入的源码必须仍在（writable layer 保留）: {}",
        kept.stdout
    );

    let view = runtime::reset_instance(&db, &docker, JWT_SECRET, view.instance_id, subject, "flag")
        .await
        .expect("reset");
    assert_eq!(view.runtime_generation, 2, "reset bumps generation");
    assert_eq!(
        view.endpoints[0].public_port, original_port,
        "reset 保留端点"
    );
    let (inst_row, _ext) =
        floatctf::modules::event::awdp::repo::instance_repo::find_by_instance_id(
            &db,
            view.instance_id,
        )
        .await
        .unwrap();
    let gone = rt
        .exec(
            &inst_row.container_name,
            fcmc::ExecOptions {
                cmd: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "cat /var/www/html/index.php".into(),
                ],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(10),
                stdout_limit: 4096,
                stderr_limit: 4096,
                stdin: None,
            },
        )
        .await
        .expect("exec marker check");
    assert_eq!(gone.exit_code, Some(0), "cat index.php after reset");
    assert!(
        !gone.stdout.contains("marker"),
        "reset 后 patch 修改应消失（恢复镜像原版）: {}",
        gone.stdout
    );

    // 4. Manual check：health + judge 通过且不计分（§84）。
    let before = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    // 同步 manual：Test Check 流程独占（worker 不领取）；health+judge+exploit 诊断。
    let enqueued = floatctf::modules::event::awdp::service::evaluation::manual_check_enqueue(
        &db,
        run_id,
        view.instance_id,
        subject,
    )
    .await
    .expect("enqueue manual check");
    let _res = floatctf::modules::event::awdp::service::evaluation::manual_check_run_now(
        &db, &docker, &enqueued,
    )
    .await
    .expect("manual check sync");
    let mc = evaluation_repo::find_by_id(&db, enqueued.id)
        .await
        .expect("eval");
    assert_eq!(
        mc.status,
        AwdpEvaluationStatus::Patched,
        "health+judge 全过 → patched"
    );
    let after = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(before, after, "manual check 不计分");

    // §84：manual 同步执行含 exploit 诊断（不计分；exploit 结果落行）。
    let evals = evaluation_repo::list_for_run(&db, run_id).await.unwrap();
    let manual = evals
        .iter()
        .find(|e| e.kind == AwdpEvaluationKind::Manual)
        .expect("manual evaluation row");
    assert_eq!(manual.status, AwdpEvaluationStatus::Patched);
    assert!(
        manual.exploit_result.is_some(),
        "manual 同步执行含 exploit 诊断"
    );
    assert!(
        manual.healthcheck_result.is_some() && manual.judge_result.is_some(),
        "manual check 结果必须落库"
    );
    assert!(manual.fix_round_id.is_none(), "manual 评估不属于任何回合");

    // 5. Break 阶段拒绝 patch（新 run 留在 Break）。
    let (ev2, run2) = seed_event_and_run(&db, "breakgate", ParticipantMode::Individual).await;
    let gb2 = seed_gamebox_and_attach(&db, ev2, "breakgate").await;
    transition(&db, run2, AwdpPhase::Break).await;
    let v2 = runtime::start_instance(
        &db,
        &docker,
        JWT_SECRET,
        &awdp_config(),
        run2,
        gb2,
        subject,
        "flag",
    )
    .await
    .expect("start in break");
    let err = patch_service::apply_patch(&db, &docker, run2, v2.instance_id, &ok_payload, subject)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Fix"), "{err}");

    // cleanup
    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = events::Entity::delete_by_id(ev2).exec(&db).await;
    runtime::stop_instance(&db, &docker, view.instance_id, subject)
        .await
        .ok();
    let _ = runtime::stop_instance(&db, &docker, v2.instance_id, subject).await;
}

#[tokio::test]
async fn manual_test_check_sync_returns_completed() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    remove_judge_container().await;
    let rt = fcmc::DockerContainerRuntime::new(docker.clone());
    if fcmc::ImageRuntime::inspect_image(&rt, IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: image {IMAGE_REF} not present");
        return;
    }

    let (event_id, run_id) = seed_event_and_run(&db, "mchksync", ParticipantMode::Individual).await;
    let gb_id = seed_gamebox_and_attach(&db, event_id, "mchksync").await;
    transition(&db, run_id, AwdpPhase::Fix).await;
    let user_id = seed_user(&db, "mchksync").await;
    let subject = Subject::user(user_id);
    let view = runtime::start_instance(
        &db,
        &docker,
        JWT_SECRET,
        &awdp_config(),
        run_id,
        gb_id,
        subject,
        "flag",
    )
    .await
    .expect("start");
    let ok_payload = patch_payload(
        r#"#!/bin/sh
cp src/index.php "$FLOATCTF_SOURCE_DIR/index.php"
echo "patched-ok"
exit 0
"#,
        &[("src/index.php", "<?php echo 'patched-by-awdp'; ?>")],
    );
    let r =
        patch_service::apply_patch(&db, &docker, run_id, view.instance_id, &ok_payload, subject)
            .await
            .expect("patch");
    assert_eq!(r, patch_service::PatchResult::Applied);

    // 同步 Test Check：HTTP 请求内直接执行（不排队），返回完成结果。
    let ev = floatctf::modules::event::awdp::service::evaluation::manual_check_enqueue(
        &db,
        run_id,
        view.instance_id,
        subject,
    )
    .await
    .expect("enqueue manual");
    let res = floatctf::modules::event::awdp::service::evaluation::manual_check_run_now(
        &db, &docker, &ev,
    )
    .await
    .expect("sync manual check");
    assert!(res.healthcheck_ok, "同步检查 healthcheck 应通过");
    assert!(res.judge_ok, "同步检查 judge 应通过");
    // BUG4：比赛 Fix 阶段 Test Check 同样执行 exploit 诊断（不计分）。
    // 本测试 gamebox 的 exploit 脚本为占位（非合法 python）→ 输出解析失败 → 视为已修复。
    assert_eq!(
        res.exploit_ok,
        Some(false),
        "比赛同步检查运行 exploit（占位脚本解析失败 → 修复成功 False）"
    );
    assert!(res.exploit_detail.is_some(), "exploit 详情随结果返回");

    // 行终态立即可见（无需 worker 消费）。
    let fresh = evaluation_repo::find_by_id(&db, ev.id).await.unwrap();
    assert_eq!(fresh.status, AwdpEvaluationStatus::Patched);
    assert!(
        fresh.exploit_result.is_some(),
        "比赛同步检查写 exploit 详情"
    );
    assert!(fresh.lease_token_hash.is_none(), "终态行不得持有 lease");
    let total = score_repo::my_total(&db, run_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(total, 0, "同步检查不计分");

    let _ = runtime::stop_instance(&db, &docker, view.instance_id, subject).await;
    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
}

#[tokio::test]
async fn break_to_fix_resets_all_instances() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    remove_judge_container().await;
    let rt = fcmc::DockerContainerRuntime::new(docker.clone());
    if fcmc::ImageRuntime::inspect_image(&rt, IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: image {IMAGE_REF} not present");
        return;
    }

    let (event_id, run_id) = seed_event_and_run(&db, "btf", ParticipantMode::Individual).await;
    let gb_id = seed_gamebox_and_attach(&db, event_id, "btf").await;
    transition(&db, run_id, AwdpPhase::Break).await;
    let user_id = seed_user(&db, "btf").await;
    let subject = Subject::user(user_id);
    let view = runtime::start_instance(
        &db,
        &docker,
        JWT_SECRET,
        &awdp_config(),
        run_id,
        gb_id,
        subject,
        "flag",
    )
    .await
    .expect("start");
    let port = view.endpoints[0].public_port;
    assert_eq!(view.runtime_generation, 1);

    // §82：Break 阶段容器 writable layer 可修改（转换前存在）。
    let (inst_row, _ext) =
        floatctf::modules::event::awdp::repo::instance_repo::find_by_instance_id(
            &db,
            view.instance_id,
        )
        .await
        .unwrap();
    let marker = rt
        .exec(
            &inst_row.container_name,
            fcmc::ExecOptions {
                cmd: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "echo break-marker > /tmp/awdp-break-marker".into(),
                ],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(10),
                stdout_limit: 4096,
                stderr_limit: 4096,
                stdin: None,
            },
        )
        .await
        .expect("write break marker");
    assert_eq!(marker.exit_code, Some(0));
    let check = rt
        .exec(
            &inst_row.container_name,
            fcmc::ExecOptions {
                cmd: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "test -f /tmp/awdp-break-marker".into(),
                ],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(10),
                stdout_limit: 4096,
                stderr_limit: 4096,
                stdin: None,
            },
        )
        .await
        .expect("check break marker");
    assert_eq!(check.exit_code, Some(0), "Break 阶段修改必须存在于容器中");

    // Break → Fix：实例 pristine reset（gen+1、端点不变、Break 可写层消失）。
    event_service::transition_break_to_fix(&db, &docker, JWT_SECRET, run_id)
        .await
        .expect("break to fix");
    let row = run_repo::require_by_id(&db, run_id).await.unwrap();
    assert_eq!(row.phase, AwdpPhase::Fix);
    assert!(row.fix_started_at.is_some());

    let after = runtime::get_my_instance_view(&db, run_id, gb_id, subject)
        .await
        .unwrap()
        .expect("view");
    assert_eq!(after.runtime_generation, 2, "break→fix 重置实例");
    assert_eq!(after.endpoints[0].public_port, port, "端点稳定");

    // §82：转换后 Break writable layer 消失（pristine reset）。
    let (inst_after, _ext2) =
        floatctf::modules::event::awdp::repo::instance_repo::find_by_instance_id(
            &db,
            after.instance_id,
        )
        .await
        .unwrap();
    assert_eq!(inst_after.id, inst_row.id, "logical instance 不变");
    let gone = rt
        .exec(
            &inst_after.container_name,
            fcmc::ExecOptions {
                cmd: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "test ! -f /tmp/awdp-break-marker".into(),
                ],
                env: vec![],
                workdir: None,
                timeout: std::time::Duration::from_secs(10),
                stdout_limit: 4096,
                stderr_limit: 4096,
                stdin: None,
            },
        )
        .await
        .expect("check marker gone");
    assert_eq!(
        gone.exit_code,
        Some(0),
        "Break→Fix 后 Break 可写层必须清除（pristine）"
    );

    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = runtime::stop_instance(&db, &docker, after.instance_id, subject).await;
}
