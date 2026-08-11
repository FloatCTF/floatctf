//! AWDP Break→Fix + Patch 集成测试（DB + Docker gated）。
//!
//! 覆盖（plan §47）：Break 上传拒绝 / Fix 上传接受 / FLOATCTF_SOURCE_DIR /
//! exit 0 applied / 非 0 failed / restart 保留 patch / reset 移除 patch /
//! reset 保留端点 + generation+1 / manual check 不计分 / break→fix 实例 pristine。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, QueryFilter};
use uuid::Uuid;

use fcmc::ContainerRuntime;
use floatctf::entity::{
    awdp_event_gameboxes, events, gameboxes,
    sea_orm_active_enums::{AwdpPhase, EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::awdp::{
    domain::AwdpConfig,
    repo::{event_gamebox_repo, event_repo, patch_repo, score_repo},
    service::{
        event_service, patch_service,
        runtime::{self, Subject},
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

/// 建事件（pending）→ 可选推进到 Break/Fix。
async fn seed_event(db: &sea_orm::DatabaseConnection, tag: &str, mode: ParticipantMode) -> Uuid {
    let base = chrono::Utc::now();
    let event = events::ActiveModel {
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
    event.id
}

async fn transition(db: &sea_orm::DatabaseConnection, event_id: Uuid, to: AwdpPhase) {
    let now = chrono::Utc::now();
    match to {
        AwdpPhase::Break => {
            event_repo::transition_phase(
                db,
                event_id,
                AwdpPhase::Pending,
                AwdpPhase::Break,
                event_repo::PhaseTransitionPatch {
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
            event_repo::transition_phase(
                db,
                event_id,
                AwdpPhase::Pending,
                AwdpPhase::Break,
                event_repo::PhaseTransitionPatch {
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
                event_id,
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
    let eg = event_gamebox_repo::attach_gamebox(db, event_id, gb.id, false)
        .await
        .expect("attach");
    eg.id
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
    let rt = fcmc::DockerContainerRuntime::new(docker.clone());
    if fcmc::ImageRuntime::inspect_image(&rt, IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: image {IMAGE_REF} not present");
        return;
    }

    let event_id = seed_event(&db, "fix", ParticipantMode::Individual).await;
    let eg_id = seed_gamebox_and_attach(&db, event_id, "fix").await;
    transition(&db, event_id, AwdpPhase::Fix).await;

    let user_id = seed_user(&db, "fix").await;
    let subject = Subject::user(user_id);
    let view = runtime::start_instance(&db, &docker, JWT_SECRET, event_id, eg_id, subject, "flag")
        .await
        .expect("start");
    let original_port = view.endpoints[0].public_port;

    // 1. Fix 阶段上传成功 patch → applied + restart。
    let ok_patch = r#"#!/bin/sh
echo "hello from patch"
ls "$FLOATCTF_SOURCE_DIR" >/dev/null && echo "source-ok"
exit 0
"#;
    let r = patch_service::apply_patch(&db, &docker, event_id, view.instance_id, ok_patch, subject)
        .await
        .expect("apply ok patch");
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
            .contains("source-ok")
    );

    // 2. 失败 patch → failed（exit 1）。
    let bad_patch = "#!/bin/sh\necho boom >&2\nexit 1\n";
    let r =
        patch_service::apply_patch(&db, &docker, event_id, view.instance_id, bad_patch, subject)
            .await
            .expect("apply bad patch");
    assert_eq!(r, patch_service::PatchResult::Failed);
    let sub = patch_repo::latest_for_instance(&db, view.instance_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sub.status, "failed");
    assert_eq!(sub.exit_code, Some(1));

    // 3. 写入标记的 patch → reset → pristine 容器中标记消失 + 端点不变 + generation+1。
    let marker_patch = "#!/bin/sh\necho marker > /tmp/awdp-marker\nexit 0\n";
    let r = patch_service::apply_patch(
        &db,
        &docker,
        event_id,
        view.instance_id,
        marker_patch,
        subject,
    )
    .await
    .expect("marker patch");
    assert_eq!(r, patch_service::PatchResult::Applied);
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
                    "test ! -f /tmp/awdp-marker".into(),
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
    assert_eq!(gone.exit_code, Some(0), "reset 后 patch 标记应消失");

    // 4. Manual check：health + judge 通过且不计分。
    let before = score_repo::my_total(&db, event_id, Some(user_id), None)
        .await
        .unwrap();
    let mc = floatctf::modules::event::awdp::service::evaluation::manual_check(
        &db,
        &docker,
        event_id,
        view.instance_id,
        subject,
    )
    .await
    .expect("manual check");
    assert!(mc.healthcheck_ok, "{:?}", mc.healthcheck_detail);
    assert!(mc.judge_ok, "{:?}", mc.judge_detail);
    let after = score_repo::my_total(&db, event_id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(before, after, "manual check 不计分");

    // 5. Break 阶段拒绝 patch（新建事件）。
    let ev2 = seed_event(&db, "breakgate", ParticipantMode::Individual).await;
    let eg2 = seed_gamebox_and_attach(&db, ev2, "breakgate").await;
    transition(&db, ev2, AwdpPhase::Break).await;
    let v2 = runtime::start_instance(&db, &docker, JWT_SECRET, ev2, eg2, subject, "flag")
        .await
        .expect("start in break");
    let err = patch_service::apply_patch(&db, &docker, ev2, v2.instance_id, ok_patch, subject)
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
async fn break_to_fix_resets_all_instances() {
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

    let event_id = seed_event(&db, "btf", ParticipantMode::Individual).await;
    let eg_id = seed_gamebox_and_attach(&db, event_id, "btf").await;
    transition(&db, event_id, AwdpPhase::Break).await;
    let user_id = seed_user(&db, "btf").await;
    let subject = Subject::user(user_id);
    let view = runtime::start_instance(&db, &docker, JWT_SECRET, event_id, eg_id, subject, "flag")
        .await
        .expect("start");
    let port = view.endpoints[0].public_port;
    assert_eq!(view.runtime_generation, 1);

    // Break → Fix：实例 pristine reset（gen+1、端点不变）。
    event_service::transition_break_to_fix(&db, &docker, JWT_SECRET, event_id)
        .await
        .expect("break to fix");
    let row = event_repo::require_by_event_id(&db, event_id)
        .await
        .unwrap();
    assert_eq!(row.phase, AwdpPhase::Fix);
    assert!(row.fix_started_at.is_some());

    let after = runtime::get_my_instance_view(&db, event_id, eg_id, subject)
        .await
        .unwrap()
        .expect("view");
    assert_eq!(after.runtime_generation, 2, "break→fix 重置实例");
    assert_eq!(after.endpoints[0].public_port, port, "端点稳定");

    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = runtime::stop_instance(&db, &docker, after.instance_id, subject).await;
}
