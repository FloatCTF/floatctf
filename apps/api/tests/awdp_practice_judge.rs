//! AWDP 练习 Judge 集成测试。
//!
//! 覆盖：
//! - 配置 CRUD + 结果按 callback_id 幂等（DB）
//! - resolve_judge_server_url 显式/自动推导
//! - sweep 门禁（disabled / interval 节流）（DB）
//! - record_callback 解析 + 幂等落库（DB）
//! - 真 Docker e2e：deploy judge → sweep → 回调落库（judge 镜像存在时）

use std::time::Duration;

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::core::config::AwdpStaticConfig;
use floatctf::entity::{
    events, gameboxes,
    sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::awdp::{
    domain::judge::{PRACTICE_JUDGE_CONTAINER_NAME, judge_callback_id},
    repo::practice_judge_repo,
    service::practice_judge,
};

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

const IMAGE_REF: &str = "floatctf/gameboxes/test-g:1.0.3";
const IMAGE_ID: &str = "sha256:e8e04fcb779cfbfb64980f5c2c1b29ad507f3a6760e38cb0126335ea7893e70b";
const JUDGE_IMAGE_REF: &str = "floatctf/awdp-judgeserver:latest";
const JWT_SECRET: &[u8] = b"test-platform-secret-0123456789abcdef";

/// 测试用 AWDP 静态配置（练习子网 / JudgeServer 镜像等）。
fn awdp_config() -> AwdpStaticConfig {
    AwdpStaticConfig {
        practice_judgeserver_image: JUDGE_IMAGE_REF.to_string(),
        practice_network_subnet: "10.42.2.0/24".to_string(),
        practice_judge_ip: "10.42.2.2".to_string(),
        platform_internal_url: "http://127.0.0.1:9090".to_string(),
        eval_lease_duration_secs: 120,
        eval_max_attempts: 3,
    }
}

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

async fn seed_trainable_gamebox(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
    let now = chrono::Utc::now().into();
    let gb_id = Uuid::new_v4();
    gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(format!("awdp-judge-{tag}")),
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
        judge_script_content: Set(None),
        judge_args_json: Set(None),
        judge_timeout_secs: Set(None),
        judge_retry_interval_secs: Set(None),
        build_status: Set(Some("ready".into())),
        build_error: Set(None),
        awdp_source_code_dir: Set(Some("/var/www/html".into())),
        awdp_exploit_script_name: Set(Some("exploit.py".into())),
        awdp_exploit_script_content: Set(Some("print('not really an exploit')".into())),
        awdp_source_artifact_key: Set(Some(format!("gameboxes/{gb_id}/awdp/pkg/source.zip"))),
        awdp_source_artifact_digest: Set(Some("deadbeef".into())),
    }
    .insert(db)
    .await
    .unwrap();
    gb_id
}

async fn cleanup(db: &sea_orm::DatabaseConnection) {
    // 先清练习 run（级联 instances/results）与挂载行，再删 gamebox。
    for row in floatctf::entity::awdp_runs::Entity::find()
        .filter(floatctf::entity::awdp_runs::Column::GameboxId.is_not_null())
        .all(db)
        .await
        .unwrap()
    {
        let _ = floatctf::entity::awdp_runs::Entity::delete_by_id(row.id)
            .exec(db)
            .await;
    }
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
}

/// 建一个 awdp practice 虚拟赛事（settings FK 目标）。
async fn seed_event(db: &sea_orm::DatabaseConnection) -> Uuid {
    use sea_orm::ActiveModelTrait;
    let base = chrono::Utc::now();
    let id = Uuid::new_v4();
    events::ActiveModel {
        is_virtual: Set(true),
        id: Set(id),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Practice),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(None),
        title: Set(format!("awdp-judge-it-{}", &id.to_string()[..8])),
        description: Set(None),
        start_time: Set((base - chrono::Duration::hours(1)).into()),
        hidden: Set(true),
        allow_join: Set(false),
        rules: Set(String::new()),
        flag_prefix: Set(None),
        end_time: Set(Some((base + chrono::Duration::hours(1)).into())),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    id
}

// ────────────────────────────────────────────────────────────────────────────
// DB 测试（无 Docker）
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn practice_judge_settings_crud_and_results_idempotent() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = seed_event(&db).await;

    // 1. ensure → 默认行。
    let row = practice_judge_repo::ensure_settings(&db, event_id)
        .await
        .unwrap();
    assert!(!row.enabled);
    assert_eq!(row.interval_secs, 60);
    assert_eq!(row.flag_path, "/flag.php");
    assert_eq!(row.container_status, "stopped");

    // 2. 更新配置。
    let updated = practice_judge_repo::update_settings(
        &db,
        event_id,
        &practice_judge_repo::PracticeJudgeSettingsPatch {
            enabled: Some(true),
            judge_server_url: Some("http://10.42.2.2:8082".into()),
            interval_secs: Some(30),
            flag_path: Some("/flag.txt".into()),
        },
    )
    .await
    .unwrap();
    assert!(updated.enabled);
    assert_eq!(updated.interval_secs, 30);
    assert_eq!(updated.flag_path, "/flag.txt");
    assert_eq!(updated.judge_server_url, "http://10.42.2.2:8082");

    // 3. 容器状态 + last_sweep。
    practice_judge_repo::update_container_state(&db, event_id, "running", Some("cafe123"))
        .await
        .unwrap();
    practice_judge_repo::touch_last_sweep(&db, event_id)
        .await
        .unwrap();
    let row = practice_judge_repo::get_settings(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.container_status, "running");
    assert_eq!(row.container_id.as_deref(), Some("cafe123"));
    assert!(row.last_sweep_at.is_some());

    // 4. 结果幂等：同 callback_id 重复插入只落一条（真实 run/instance/gamebox 满足 FK）。
    let user_id = seed_user(&db, "crud").await;
    let gb_id = seed_trainable_gamebox(&db, "crud").await;
    let run = floatctf::modules::event::awdp::service::practice_service::start_training(
        &db,
        &bollard::Docker::connect_with_local_defaults().unwrap(),
        JWT_SECRET,
        user_id,
        gb_id,
        "flag",
    )
    .await
    .unwrap();
    let (instance, _ext) = floatctf::modules::event::awdp::repo::instance_repo::create_instance(
        &db,
        run.id,
        gb_id,
        Some(user_id),
        None,
        &format!("awdp-crud-test-{}", &Uuid::new_v4().to_string()[..8]),
        IMAGE_REF,
    )
    .await
    .unwrap();
    let cb = judge_callback_id(Uuid::new_v4(), run.id, instance.id, "flag");
    for _ in 0..3 {
        practice_judge_repo::insert_result(
            &db,
            event_id,
            run.id,
            instance.id,
            gb_id,
            Some(user_id),
            None,
            "flag",
            "success",
            Some("flag 端点返回预期 flag"),
            &cb,
        )
        .await
        .unwrap();
    }
    let rows = practice_judge_repo::list_results(&db, event_id, 50)
        .await
        .unwrap();
    let matches = rows
        .iter()
        .filter(|r| r.callback_id.as_deref() == Some(cb.as_str()))
        .count();
    assert_eq!(matches, 1, "callback_id 幂等去重");

    // 清理。
    let _ = floatctf::entity::awdp_judge_results::Entity::delete_many()
        .filter(floatctf::entity::awdp_judge_results::Column::EventId.eq(event_id))
        .exec(&db)
        .await;
    let _ = floatctf::entity::awdp_runs::Entity::delete_by_id(run.id)
        .exec(&db)
        .await;
    cleanup(&db).await;
    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
}

#[test]
fn resolve_judge_server_url_explicit_or_derived() {
    let config = awdp_config();

    // 显式 URL 优先。
    let settings = floatctf::entity::awdp_practice_judge_settings::Model {
        event_id: Uuid::new_v4(),
        enabled: true,
        judge_server_url: "http://192.168.1.50:8082".to_string(),
        interval_secs: 60,
        flag_path: "/flag.php".to_string(),
        container_status: "running".to_string(),
        container_id: Some("id".into()),
        last_sweep_at: None,
        created_at: chrono::Utc::now().into(),
        updated_at: chrono::Utc::now().into(),
    };
    assert_eq!(
        practice_judge::resolve_judge_server_url(&settings, &config),
        "http://192.168.1.50:8082"
    );

    // 留空 → 自动推导固定 Judge IP。
    let mut auto = settings.clone();
    auto.judge_server_url = String::new();
    assert_eq!(
        practice_judge::resolve_judge_server_url(&auto, &config),
        "http://10.42.2.2:8082"
    );
}

#[tokio::test]
async fn sweep_gated_by_enabled_and_interval() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    let event_id = seed_event(&db).await;

    // disabled → 直接 noop。
    let summary = practice_judge::sweep(&db, &docker, &awdp_config(), JWT_SECRET, event_id)
        .await
        .unwrap();
    assert_eq!(summary.dispatched, 0);
    assert!(!summary.throttled);

    // 无运行中实例（先清残留）。
    cleanup(&db).await;

    // enabled + running + 无实例 → dispatched 0（无任务也刷新间隔）。
    practice_judge_repo::update_settings(
        &db,
        event_id,
        &practice_judge_repo::PracticeJudgeSettingsPatch {
            enabled: Some(true),
            interval_secs: Some(60),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    // 容器状态 running（无真实容器也行：sweep 只查 DB 记录）。
    practice_judge_repo::update_container_state(&db, event_id, "running", Some("fake"))
        .await
        .unwrap();
    let summary = practice_judge::sweep(&db, &docker, &awdp_config(), JWT_SECRET, event_id)
        .await
        .unwrap();
    assert_eq!(summary.dispatched, 0);
    assert!(
        practice_judge_repo::get_settings(&db, event_id)
            .await
            .unwrap()
            .unwrap()
            .last_sweep_at
            .is_some()
    );

    // 间隔未到 → throttled。
    let summary = practice_judge::sweep(&db, &docker, &awdp_config(), JWT_SECRET, event_id)
        .await
        .unwrap();
    assert!(summary.throttled);

    // 清理（sweep 无容器派发，无需 docker 清理）。
    let _ = floatctf::entity::awdp_practice_judge_settings::Entity::delete_by_id(event_id)
        .exec(&db)
        .await;
}

#[tokio::test]
async fn record_callback_parses_and_records() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let user_id = seed_user(&db, "cb").await;
    let gb_id = seed_trainable_gamebox(&db, "cb").await;

    // 建练习 run + 实例（不启动容器：record_callback 只读 instance 行）。
    let run = floatctf::modules::event::awdp::service::practice_service::start_training(
        &db,
        &bollard::Docker::connect_with_local_defaults().unwrap(),
        JWT_SECRET,
        user_id,
        gb_id,
        "flag",
    )
    .await
    .unwrap();
    let (instance, _ext) = floatctf::modules::event::awdp::repo::instance_repo::create_instance(
        &db,
        run.id,
        gb_id,
        Some(user_id),
        None,
        &format!("awdp-cb-test-{}", &Uuid::new_v4().to_string()[..8]),
        IMAGE_REF,
    )
    .await
    .unwrap();

    let cb = practice_judge::JudgeCallbackRequest {
        task_id: Uuid::new_v4(),
        callback_id: judge_callback_id(Uuid::new_v4(), run.id, instance.id, "exploit"),
        kind: "exploit".to_string(),
        status: "failure".to_string(),
        exit_code: Some(1),
        duration_ms: Some(42),
        stdout: Some("out".into()),
        stderr: None,
        detail: Some("exploit FAIL".into()),
    };
    // 重复回调 ×2 → 幂等 1 条。
    practice_judge::record_callback(&db, &cb).await.unwrap();
    practice_judge::record_callback(&db, &cb).await.unwrap();

    let rows = practice_judge_repo::list_results(&db, run.event_id, 50)
        .await
        .unwrap();
    let mine: Vec<_> = rows
        .iter()
        .filter(|r| r.instance_id == instance.id)
        .collect();
    assert_eq!(mine.len(), 1, "重复回调幂等");
    assert_eq!(mine[0].check_kind, "exploit");
    assert_eq!(mine[0].status, "failure");
    assert!(
        mine[0]
            .detail
            .as_deref()
            .unwrap_or("")
            .contains("exploit FAIL")
    );
    assert_eq!(mine[0].gamebox_id, gb_id);
    assert_eq!(mine[0].owner_user_id, Some(user_id));

    // 畸形 callback_id → 拒绝。
    let bad = practice_judge::JudgeCallbackRequest {
        task_id: Uuid::new_v4(),
        callback_id: "garbage".to_string(),
        kind: "exploit".to_string(),
        status: "success".to_string(),
        exit_code: None,
        duration_ms: None,
        stdout: None,
        stderr: None,
        detail: None,
    };
    assert!(practice_judge::record_callback(&db, &bad).await.is_err());

    // 清理。
    cleanup(&db).await;
    let _ = floatctf::entity::awdp_runs::Entity::delete_by_id(run.id)
        .exec(&db)
        .await;
}

// ────────────────────────────────────────────────────────────────────────────
// 真 Docker e2e（judge 镜像存在时）
// ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn practice_judge_e2e_deploy_sweep_and_callback() {
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
        || fcmc::ImageRuntime::inspect_image(&rt, JUDGE_IMAGE_REF)
            .await
            .is_err()
    {
        eprintln!("skip: judge image {JUDGE_IMAGE_REF} or gamebox {IMAGE_REF} not present");
        return;
    }
    cleanup(&db).await;
    // 停掉可能残留的 live judge（保证测试用 token 部署，幂等）。
    let _ = practice_judge::stop_judge(
        &db,
        &docker,
        floatctf::core::system_ids::EVENT_PRACTICE_AWDP,
    )
    .await;
    let event_id = floatctf::core::system_ids::EVENT_PRACTICE_AWDP;

    // 1. 练习 run + 实例（实例加入 fctf-awdp-practice 子网）。
    let user_id = seed_user(&db, "judge").await;
    let gb_id = seed_trainable_gamebox(&db, "judge").await;
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
    assert_eq!(view.runtime_state, "running");

    // 2. 部署 judge（幂等 ×2）。
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

    // 3. 开启 + 短间隔。
    practice_judge_repo::update_settings(
        &db,
        event_id,
        &practice_judge_repo::PracticeJudgeSettingsPatch {
            enabled: Some(true),
            interval_secs: Some(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    practice_judge_repo::update_container_state(&db, event_id, "running", None)
        .await
        .unwrap();
    // 清掉 last_sweep 让首次 sweep 立即放行。
    let s = practice_judge_repo::get_settings(&db, event_id)
        .await
        .unwrap()
        .unwrap();
    let mut am: floatctf::entity::awdp_practice_judge_settings::ActiveModel = s.into();
    am.last_sweep_at = Set(None);
    am.update(&db).await.unwrap();

    // 4. sweep → 应派发 exploit + flag 两个任务。
    let summary = practice_judge::sweep(&db, &docker, &awdp_config(), JWT_SECRET, event_id)
        .await
        .unwrap();
    assert_eq!(summary.dispatched, 2, "exploit + flag 各一条");

    // 5. 轮询回调落库（judge 容器内执行后回调平台——本测试无平台，回调会失败，
    //    但任务执行本身完成。此处验证 sweep 派发链路已通；回调落库由
    //    record_callback 单测覆盖）。等待并确认无异常即可。
    use fcmc::ContainerRuntime;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let judge_state = rt
        .inspect_container(PRACTICE_JUDGE_CONTAINER_NAME)
        .await
        .unwrap();
    assert!(judge_state.running, "judge container still running");

    // 6. 清理：停 judge、删 run/instance/gamebox。
    practice_judge::stop_judge(&db, &docker, event_id)
        .await
        .unwrap();
    let _ = floatctf::modules::event::awdp::service::runtime::stop_instance(
        &db,
        &docker,
        view.instance_id,
        floatctf::modules::event::awdp::service::runtime::Subject::user(user_id),
    )
    .await;
    let _ = floatctf::entity::awdp_runs::Entity::delete_by_id(run.id)
        .exec(&db)
        .await;
    cleanup(&db).await;
    // 恢复默认（enabled=false），避免影响其它测试。
    let _ = practice_judge_repo::update_settings(
        &db,
        event_id,
        &practice_judge_repo::PracticeJudgeSettingsPatch {
            enabled: Some(false),
            interval_secs: Some(60),
            ..Default::default()
        },
    )
    .await;
    let _ = practice_judge_repo::update_container_state(&db, event_id, "stopped", None).await;
}
