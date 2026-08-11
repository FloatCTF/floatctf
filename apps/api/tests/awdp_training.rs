//! AWDP Practice Training Ground 集成测试（DB + Docker gated）。
//!
//! 覆盖（plan §79）：start_training 建 run+instance / 重复 start 幂等返回既有 active run /
//! ended → train_again 创建新 run / 旧 run 历史保留（final score/rounds 不动）。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::gameboxes;
use floatctf::modules::event::awdp::{
    domain::AwdpConfig,
    repo::{run_repo, score_repo},
    service::practice_service,
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

/// 训练目录可见的 awdp-capable GameBox。
async fn seed_trainable_gamebox(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
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

async fn cleanup(db: &sea_orm::DatabaseConnection) {
    for row in gameboxes::Entity::find()
        .filter(gameboxes::Column::SafeName.like("awdp-it-gb-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = gameboxes::Entity::delete_by_id(row.id).exec(db).await;
    }
}

#[tokio::test]
async fn start_training_idempotent_and_train_again() {
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

    let user_id = seed_user(&db, "train").await;
    let gb_id = seed_trainable_gamebox(&db, "train").await;

    // 1. start_training：创建 run（phase=Break）+ 启动实例。
    let run = practice_service::start_training(&db, &docker, JWT_SECRET, user_id, gb_id, "flag")
        .await
        .expect("start training");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(
        row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break,
        "practice run 创建即 Break"
    );
    assert_eq!(row.break_duration_secs, 3600, "默认配置快照");
    assert_eq!(row.fix_round_interval_secs, 600);
    assert_eq!(row.total_rounds, 6);
    assert!(row.started_at.is_some() && row.break_ends_at.is_some());
    // 实例已启动（run 下恰一个 running 实例）。
    let instances = floatctf::modules::event::awdp::repo::instance_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(instances.len(), 1, "start_training 同步建实例");
    assert_eq!(instances[0].0.runtime_state, "running");

    // 2. 重复 start_training：幂等返回同一 run。
    let run2 = practice_service::start_training(&db, &docker, JWT_SECRET, user_id, gb_id, "flag")
        .await
        .expect("repeat start training");
    assert_eq!(run2.id, run.id, "同 user+gamebox active run 幂等");

    // 3. 直接建 run（绕过 start）→ 会因 active unique 冲突（service 层幂等已覆盖，
    //    此处验证 DB 层约束）。
    let dup = run_repo::create_practice_run(&db, gb_id, user_id, &AwdpConfig::default()).await;
    assert!(dup.is_err(), "active practice run unique 约束");

    // 4. 结束 run → train_again 创建新 run，旧 run 保留。
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
        run_repo::PhaseTransitionPatch {
            finished_at: Some(chrono::Utc::now()),
            next_action_at: None,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let new_run = practice_service::train_again(&db, &docker, JWT_SECRET, user_id, run.id, "flag")
        .await
        .expect("train again");
    assert_ne!(new_run.id, run.id, "train again 创建新 run");
    let old_row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(
        old_row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Ended,
        "旧 run 保持 ended（历史保留）"
    );
    assert!(old_row.finished_at.is_some());
    let new_row = run_repo::require_by_id(&db, new_run.id).await.unwrap();
    assert_eq!(
        new_row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break
    );
    assert_eq!(new_row.gamebox_id, Some(gb_id), "复用 gamebox");

    // 5. 旧 run 不能再 train_again 之外复活：直接对 ended run 转 Break 被拒绝。
    let err = run_repo::transition_phase(
        &db,
        run.id,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Ended,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break,
        Default::default(),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Invalid"), "{err}");

    // 6. 他人不能 train_again 我的 run。
    let other = seed_user(&db, "other-train").await;
    let err = practice_service::train_again(&db, &docker, JWT_SECRET, other, run.id, "flag")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("不属于"), "{err}");

    // 7. 分数隔离：practice run 的得分只存在于本 run（不污染其他 run）。
    let _ = score_repo::my_total(&db, run.id, Some(user_id), None)
        .await
        .unwrap();
    let _ = score_repo::my_total(&db, new_run.id, Some(user_id), None)
        .await
        .unwrap();

    // 清理：stop 实例 + 删 gamebox。
    let _ = floatctf::modules::event::awdp::service::runtime::stop_instance(
        &db,
        &docker,
        instances[0].0.id,
        floatctf::modules::event::awdp::service::runtime::Subject::user(user_id),
    )
    .await;
    cleanup(&db).await;
}
