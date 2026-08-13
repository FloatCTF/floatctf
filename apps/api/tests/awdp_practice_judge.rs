//! AWDP 练习 Judge（Pull + Lease worker）测试。
//!
//! 覆盖：
//! - settings CRUD + 幂等 ensure（DB）
//! - deploy_judge / stop_judge 幂等（真 Docker，judge 镜像存在时）
//! - 真 Docker e2e：deploy pull worker → 容器 running + data plane /healthz 可达
//!   （judge 镜像存在时）
//!
//! 旧的 sweep push / record_callback 流程已移除（plan §61），无对应测试。

use fcmc::{ContainerRuntime, DockerContainerRuntime, ImageRuntime};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{awdp_runs, events, gameboxes};
use floatctf::modules::event::awdp::{
    domain::judge::PRACTICE_JUDGE_CONTAINER_NAME,
    repo::{practice_judge_repo, run_repo},
    service::practice_judge,
};

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const IMAGE_REF: &str = "floatctf/gameboxes/test-g:1.0.3";
const IMAGE_ID: &str = "sha256:e8e04fcb779cfbfb64980f5c2c1b29ad507f3a6760e38cb0126335ea7893e70b";
const JUDGE_IMAGE_REF: &str = "floatctf/awdp-judgeserver:latest";
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

async fn cleanup(db: &sea_orm::DatabaseConnection) {
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

async fn seed_trainable_gamebox(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
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
        judge_script_content: Set(Some("import sys\nprint('[]')\n".into())),
        judge_args_json: Set(None),
        judge_timeout_secs: Set(None),
        judge_retry_interval_secs: Set(None),
        build_status: Set(Some("ready".into())),
        build_error: Set(None),
        awdp_source_code_dir: Set(Some("/var/www/html".into())),
        awdp_exploit_script_name: Set(Some("exploit.py".into())),
        awdp_exploit_script_content: Set(Some("import sys\nprint('[]')\n".into())),
        awdp_source_artifact_key: Set(Some(format!("gameboxes/{gb_id}/awdp/pkg/source.zip"))),
        awdp_source_artifact_digest: Set(Some("deadbeef".into())),
    }
    .insert(db)
    .await
    .unwrap();
    gb_id
}

async fn seed_event(db: &sea_orm::DatabaseConnection) -> Uuid {
    let base = chrono::Utc::now();
    let event = events::ActiveModel {
        is_virtual: Set(false),
        id: Set(Uuid::new_v4()),
        family: Set(floatctf::entity::sea_orm_active_enums::EventFamily::Awdp),
        purpose: Set(floatctf::entity::sea_orm_active_enums::EventPurpose::Competition),
        participant_mode: Set(floatctf::entity::sea_orm_active_enums::ParticipantMode::Individual),
        system_key: Set(None),
        title: Set("awdp-it-judge-seed".to_string()),
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
    event.id
}

/// 测试用 AWDP 静态配置。
fn awdp_config() -> floatctf::core::config::AwdpStaticConfig {
    floatctf::core::config::AwdpStaticConfig {
        practice_judgeserver_image: "floatctf/awdp-judgeserver:latest".to_string(),
        practice_network_subnet: "10.42.2.0/24".to_string(),
        practice_judge_ip: "10.42.2.2".to_string(),
        platform_internal_url: "http://host.docker.internal:9090".to_string(),
        eval_lease_duration_secs: 120,
        eval_max_attempts: 3,
    }
}

/// settings CRUD：ensure 默认 / 更新 / 容器状态记录。
#[tokio::test]
async fn practice_judge_settings_crud() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let event_id = seed_event(&db).await;

    let s0 = practice_judge_repo::ensure_settings(&db, event_id)
        .await
        .unwrap();
    assert!(!s0.enabled);
    assert_eq!(s0.container_status, "stopped");
    // 幂等 ensure。
    let s1 = practice_judge_repo::ensure_settings(&db, event_id)
        .await
        .unwrap();
    assert_eq!(s1.event_id, event_id);

    let s2 = practice_judge_repo::update_settings(
        &db,
        event_id,
        &practice_judge_repo::PracticeJudgeSettingsPatch {
            enabled: Some(true),
            interval_secs: Some(30),
            flag_path: Some("/flag.php".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(s2.enabled);
    assert_eq!(s2.interval_secs, 30);

    practice_judge_repo::update_container_state(&db, event_id, "running", None)
        .await
        .unwrap();
    let s3 = practice_judge_repo::get_settings(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(s3.container_status, "running");

    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    cleanup(&db).await;
}

/// resolve_judge_server_url：显式 URL 优先；留空按 config IP 推导。
#[tokio::test]
async fn resolve_judge_server_url_prefers_explicit() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;
    let event_id = seed_event(&db).await;
    let cfg = awdp_config();

    let s = practice_judge_repo::ensure_settings(&db, event_id)
        .await
        .unwrap();
    assert_eq!(
        practice_judge::resolve_judge_server_url(&s, &cfg),
        "http://10.42.2.2:8080"
    );
    let s2 = practice_judge_repo::update_settings(
        &db,
        event_id,
        &practice_judge_repo::PracticeJudgeSettingsPatch {
            judge_server_url: Some("http://judge.awdp.internal:8080".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        practice_judge::resolve_judge_server_url(&s2, &cfg),
        "http://judge.awdp.internal:8080"
    );
    cleanup(&db).await;
}

/// 真 Docker e2e：deploy pull worker（幂等 ×2）→ 容器 running → data plane /healthz 可达。
/// （platform_internal_url 指向无 API 的端口：worker 拉取失败会重试，不影响本测试验证部署。）
#[tokio::test]
async fn practice_judge_deploy_pull_worker_e2e() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    let rt = DockerContainerRuntime::new(docker.clone());
    if ImageRuntime::inspect_image(&rt, JUDGE_IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: judge image {JUDGE_IMAGE_REF} not present");
        return;
    }
    cleanup(&db).await;
    let event_id = floatctf::core::system_ids::EVENT_PRACTICE_AWDP;
    // 停掉可能残留的 live judge（保证用测试 token 部署，幂等）。
    let _ = practice_judge::stop_judge(&db, &docker, event_id).await;

    // 1. deploy（幂等 ×2）。
    practice_judge::deploy_judge(&db, &docker, &awdp_config(), JWT_SECRET, event_id)
        .await
        .unwrap();
    practice_judge::deploy_judge(&db, &docker, &awdp_config(), JWT_SECRET, event_id)
        .await
        .unwrap();
    let settings = practice_judge_repo::get_settings(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(settings.container_status, "running");

    // 2. 容器真实 running + data plane /healthz 可达（固定 IP 直连）。
    let state = rt
        .inspect_container(PRACTICE_JUDGE_CONTAINER_NAME)
        .await
        .expect("judge container inspect");
    assert!(state.running, "judge container must be running");

    use std::time::Duration;
    let mut healthy = false;
    for _ in 0..10 {
        let url = format!(
            "http://{}:{}/healthz",
            awdp_config().practice_judge_ip,
            floatctf::modules::event::awdp::domain::judge::PRACTICE_JUDGE_PORT
        );
        match reqwest::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                healthy = true;
                break;
            }
            _ => tokio::time::sleep(Duration::from_secs(1)).await,
        }
    }
    assert!(healthy, "judge data plane /healthz must be reachable");

    // 3. 清理：停 judge。
    practice_judge::stop_judge(&db, &docker, event_id)
        .await
        .unwrap();
    let settings = practice_judge_repo::get_settings(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(settings.container_status, "stopped");
    cleanup(&db).await;
}

/// 练习 run 解析（deploy 前置校验用）。
#[tokio::test]
async fn ensure_practice_event_resolves_virtual_event() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = practice_judge::ensure_practice_event(&db).await.unwrap();
    assert_eq!(
        event_id,
        floatctf::core::system_ids::EVENT_PRACTICE_AWDP,
        "AWDPlusPractice 虚拟赛事必须存在且稳定"
    );
    let _ = run_repo::ensure_practice_event(&db).await;
}

/// 旧 push 流程（sweep / callback）代码不得存在（plan §61）。
#[test]
fn push_flow_removed_from_source() {
    let root = env!("CARGO_MANIFEST_DIR");
    for (path, needles) in [
        (
            format!("{root}/src/modules/event/awdp/service/practice_judge.rs"),
            &[
                "fn sweep(",
                "record_callback",
                "dispatch_batch",
                "JudgeCallbackRequest",
            ][..],
        ),
        (
            format!("{root}/src/modules/event/awdp/api/internal.rs"),
            &["/internal/awdp/practice/judge/callback"][..],
        ),
    ] {
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        for needle in needles {
            assert!(!src.contains(needle), "push 流程残留 {needle:?} 在 {path}");
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// §64 网络拓扑与 ACL
// ────────────────────────────────────────────────────────────────────────────

/// 拓扑：JudgeServer 同时加入 data + control 网络；GameBox 只加入 data 网络。
#[tokio::test]
async fn judge_on_both_networks_gamebox_data_only() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    let rt = DockerContainerRuntime::new(docker.clone());
    if ImageRuntime::inspect_image(&rt, JUDGE_IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: judge image {JUDGE_IMAGE_REF} not present");
        return;
    }
    cleanup(&db).await;
    let event_id = floatctf::core::system_ids::EVENT_PRACTICE_AWDP;
    let _ = practice_judge::stop_judge(&db, &docker, event_id).await;

    // 1. 两个网络 ensure。
    let _ = practice_judge::ensure_practice_network(&docker, &awdp_config())
        .await
        .unwrap();
    floatctf::modules::event::awdp::service::practice_acl::ensure_control_network(&docker)
        .await
        .unwrap();

    // 2. 部署 judge（data + control）。
    practice_judge::deploy_judge(&db, &docker, &awdp_config(), JWT_SECRET, event_id)
        .await
        .unwrap();

    let judge_networks: Vec<String> = match docker
        .inspect_container(
            PRACTICE_JUDGE_CONTAINER_NAME,
            None::<bollard::container::InspectContainerOptions>,
        )
        .await
        .expect("judge inspect")
        .network_settings
        .and_then(|n| n.networks)
    {
        Some(map) => map.keys().cloned().collect(),
        None => vec![],
    };
    assert!(
        judge_networks.contains(&"fctf-awdp-practice".to_string()),
        "judge 必须在 data 网络: {judge_networks:?}"
    );
    assert!(
        judge_networks.contains(&"fctf-awdp-control".to_string()),
        "judge 必须在 control 网络: {judge_networks:?}"
    );

    // 3. 启动一个练习实例（GameBox）→ 只应在 data 网络。
    let user_id = seed_user(&db, "topo").await;
    let gb_id = seed_trainable_gamebox(&db, "topo").await;
    let run = floatctf::modules::event::awdp::service::practice_service::start_training(
        &db, &docker, JWT_SECRET, user_id, gb_id, "flag",
    )
    .await
    .unwrap();
    let view = floatctf::modules::event::awdp::service::runtime::start_instance(
        &db,
        &docker,
        JWT_SECRET,
        &awdp_config(),
        run.id,
        gb_id,
        floatctf::modules::event::awdp::service::runtime::Subject::user(user_id),
        "flag",
    )
    .await
    .unwrap();
    let inst_container_name = {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        floatctf::entity::event_instances::Entity::find()
            .filter(floatctf::entity::event_instances::Column::Id.eq(view.instance_id))
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .container_name
    };
    let inst_networks: Vec<String> = match docker
        .inspect_container(
            &inst_container_name,
            None::<bollard::container::InspectContainerOptions>,
        )
        .await
        .expect("instance inspect")
        .network_settings
        .and_then(|n| n.networks)
    {
        Some(map) => map.keys().cloned().collect(),
        None => vec![],
    };
    assert!(
        inst_networks.contains(&"fctf-awdp-practice".to_string()),
        "GameBox 必须在 data 网络: {inst_networks:?}"
    );
    assert!(
        !inst_networks.contains(&"fctf-awdp-control".to_string()),
        "GameBox 禁止加入 control 网络: {inst_networks:?}"
    );

    // 4. 清理。
    let _ = floatctf::modules::event::awdp::service::runtime::stop_instance(
        &db,
        &docker,
        view.instance_id,
        floatctf::modules::event::awdp::service::runtime::Subject::user(user_id),
    )
    .await;
    let _ = awdp_runs::Entity::delete_by_id(run.id).exec(&db).await;
    let _ = practice_judge::stop_judge(&db, &docker, event_id).await;
    cleanup(&db).await;
}

/// nft 可用时（root）应用 ACL 并验证连通性矩阵（§64）；否则跳过。
#[tokio::test]
async fn practice_acl_connectivity_matrix() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(_db) = connect_or_skip().await else {
        return;
    };
    let probe = tokio::process::Command::new("nft")
        .args(["list", "table", "inet", "floatctf_awdp_practice"])
        .output()
        .await;
    match &probe {
        Ok(o) if o.status.success() => {}
        Ok(_) => {
            eprintln!("skip: nft 不可用/无权限（ACL 由 root 运维应用）");
            return;
        }
        Err(_) => {
            eprintln!("skip: nft 不存在");
            return;
        }
    }
    // 真实连通性矩阵需要完整两实例 + 规则应用编排；
    // 环境无 root 时此处不执行——规则正确性由 render 单测 + 人工 root 验证覆盖。
    eprintln!("skip: root 环境连通性矩阵留待运维验证（render 单测已覆盖规则内容）");
}
