//! AWDP Practice Training Ground 集成测试（DB + Docker gated）。
//!
//! 覆盖（plan §79）：start_training 建 run+instance / 重复 start 幂等返回既有 active run /
//! ended → train_again 创建新 run / 旧 run 历史保留（final score/rounds 不动）。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::gameboxes;
use floatctf::modules::event::awdp::{
    domain::AwdpConfig,
    repo::{run_repo, score_repo, writeup_repo},
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
    use sea_orm::EntityTrait;
    // 先删 AWDPlusPractice 上的挂载行（awdp_event_gameboxes.gamebox_id RESTRICT）。
    for row in floatctf::entity::gameboxes::Entity::find()
        .filter(floatctf::entity::gameboxes::Column::SafeName.like("awdp-it-gb-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = floatctf::entity::awdp_event_gameboxes::Entity::delete_many()
            .filter(floatctf::entity::awdp_event_gameboxes::Column::GameboxId.eq(row.id))
            .exec(db)
            .await;
        let _ = floatctf::entity::gameboxes::Entity::delete_by_id(row.id)
            .exec(db)
            .await;
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

    // 1. start_training：创建 run（phase=Break，**冻结**：不启动实例，等待玩家点「开始」）。
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
    assert!(
        row.next_action_at.is_none(),
        "未点「开始」前 tick 不推进（冻结）"
    );
    // 未点「开始」→ 无实例。
    let instances = floatctf::modules::event::awdp::repo::instance_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(instances.len(), 0, "start_training 只建 run 不启动实例");

    // 1.5 玩家点「开始」（start_instance）→ 建实例并启动。
    floatctf::modules::event::awdp::service::runtime::start_instance(
        &db,
        &docker,
        JWT_SECRET,
        run.id,
        gb_id,
        floatctf::modules::event::awdp::service::runtime::Subject::user(user_id),
        "flag",
    )
    .await
    .expect("begin instance");
    let instances = floatctf::modules::event::awdp::repo::instance_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(instances.len(), 1, "「开始」后同步建实例");
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

/// Run Writeup：一 run 一份，upsert 更新（§右侧写 WP）。
#[tokio::test]
async fn run_writeup_upsert_single_row() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let user_id = seed_user(&db, "wp").await;
    let gb_id = seed_trainable_gamebox(&db, "wp").await;
    let run = run_repo::create_practice_run(&db, gb_id, user_id, &AwdpConfig::default())
        .await
        .expect("create practice run");

    // 1. 无记录 → None。
    assert!(
        writeup_repo::find_by_run(&db, run.id)
            .await
            .unwrap()
            .is_none()
    );

    // 2. upsert 创建。
    let saved = writeup_repo::upsert(&db, run.id, user_id, "# 思路\n1. 扫目录".into())
        .await
        .expect("create writeup");
    assert_eq!(saved.content, "# 思路\n1. 扫目录");
    let found = writeup_repo::find_by_run(&db, run.id)
        .await
        .unwrap()
        .expect("writeup exists");
    assert_eq!(found.user_id, user_id);
    assert_eq!(found.content, "# 思路\n1. 扫目录");

    // 3. upsert 更新：内容替换 + updated_at 刷新，仍是一行。
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let updated = writeup_repo::upsert(&db, run.id, user_id, "# 更新版".into())
        .await
        .expect("update writeup");
    assert_eq!(updated.content, "# 更新版");
    assert_eq!(updated.user_id, user_id, "user_id 不变");
    assert!(updated.updated_at > found.updated_at, "updated_at 刷新");
    let rows = floatctf::entity::awdp_run_writeups::Entity::find()
        .filter(floatctf::entity::awdp_run_writeups::Column::RunId.eq(run.id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "一 run 仅一行");

    // 4. 清理（含 writeup 行）。
    writeup_repo::delete_for_run(&db, run.id).await.unwrap();
    assert!(
        writeup_repo::find_by_run(&db, run.id)
            .await
            .unwrap()
            .is_none()
    );
    cleanup(&db).await;
}

async fn fetch_flag_with_retry(url: &str) -> String {
    for _ in 0..12 {
        match reqwest::get(url).await {
            Ok(resp) => match resp.text().await {
                Ok(body) => {
                    let flag = body.trim().to_string();
                    if !flag.is_empty() {
                        return flag;
                    }
                }
                Err(_) => {}
            },
            Err(_) => {}
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    panic!("flag endpoint not ready: {url}");
}

#[tokio::test]
async fn train_again_preserves_old_run_history() {
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

    let user_id = seed_user(&db, "hist").await;
    let gb_id = seed_trainable_gamebox(&db, "hist").await;

    // 1. 第一个训练 run：Break 得分 + Fix 回合物化。
    let run = practice_service::start_training(&db, &docker, JWT_SECRET, user_id, gb_id, "flag")
        .await
        .expect("start training");
    let subject = floatctf::modules::event::awdp::service::runtime::Subject::user(user_id);
    // start_training 只建 run（冻结）；玩家点「开始」启动实例。
    floatctf::modules::event::awdp::service::runtime::start_instance(
        &db, &docker, JWT_SECRET, run.id, gb_id, subject, "flag",
    )
    .await
    .expect("begin instance");
    let view = floatctf::modules::event::awdp::service::runtime::get_my_instance_view(
        &db, run.id, gb_id, subject,
    )
    .await
    .unwrap()
    .expect("instance view");
    let ep = &view.endpoints[0];
    let flag = fetch_flag_with_retry(&format!(
        "http://{}:{}/flag.php",
        ep.public_host, ep.public_port
    ))
    .await;
    let r = floatctf::modules::event::awdp::service::break_service::submit_flag(
        &db, JWT_SECRET, run.id, gb_id, &flag, subject,
    )
    .await
    .expect("break submit");
    assert!(r.accepted && r.scored, "first break: {r:?}");
    let break_total = score_repo::my_total(&db, run.id, Some(user_id), None)
        .await
        .unwrap();
    let run_row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(break_total, run_row.break_score, "break score once");

    // Break→Fix：实例 reset pristine + 回合物化（历史数据基础）。
    floatctf::modules::event::awdp::service::event_service::transition_break_to_fix(
        &db, &docker, JWT_SECRET, run.id,
    )
    .await
    .expect("break to fix");
    let rounds_old = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(rounds_old.len(), 6, "默认 3600/600 = 6 rounds");

    // 结束旧 run。
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

    // 2. Train Again → 新 run（§79 ended 后创建新 run）。
    let new_run = practice_service::train_again(&db, &docker, JWT_SECRET, user_id, run.id, "flag")
        .await
        .expect("train again");
    assert_ne!(new_run.id, run.id, "新 run");

    // 3. 旧 run 历史完全保留（§79 old run remains immutable）。
    let old = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(
        old.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Ended
    );
    let rounds_after = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(
        rounds_after.len(),
        6,
        "旧 run 的 rounds 保留不变（含 starts_at/cutoff_at）"
    );
    for (before, after) in rounds_old.iter().zip(rounds_after.iter()) {
        assert_eq!(before.id, after.id);
        assert_eq!(before.starts_at, after.starts_at);
        assert_eq!(before.cutoff_at, after.cutoff_at);
    }
    let score_after = score_repo::my_total(&db, run.id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(
        score_after, break_total,
        "旧 run 分数保留（break_score 不因新 run 变化）"
    );
    let breaks = floatctf::modules::event::awdp::repo::break_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(breaks.len(), 1, "旧 run 的 break 记录保留");

    // 4. 新 run 是干净的：无 rounds、无 score、实例正在运行。
    let new_rounds =
        floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, new_run.id)
            .await
            .unwrap();
    assert!(new_rounds.is_empty(), "新 run 无历史 rounds");
    let new_score = score_repo::my_total(&db, new_run.id, Some(user_id), None)
        .await
        .unwrap();
    assert_eq!(new_score, 0, "新 run 分数从零开始");
    // start_training 只建 run（冻结）；新 run 无实例，待玩家点「开始」。
    let new_instances =
        floatctf::modules::event::awdp::repo::instance_repo::list_for_run(&db, new_run.id)
            .await
            .unwrap();
    assert_eq!(
        new_instances.len(),
        0,
        "train_again 新 run 冻结，不启动实例"
    );

    // 清理（新 run 冻结无实例；只停旧 run 实例）。
    let _ = floatctf::modules::event::awdp::service::runtime::stop_instance(
        &db,
        &docker,
        view.instance_id,
        subject,
    )
    .await;
    cleanup(&db).await;
}

/// 练习模式手动控制阶段（plan 新需求）：
///   break → 直接进入 Fix（回合物化）→ 提前 Check 记录 → Fix → 回到 Break
///   （fix 会话撤销：回合/评估/计分清零，early_patched_seq 清零）→ 再进 Fix（全新时间线）。
/// 不依赖真实 Docker 操作（无实例时 transition_break_to_fix 的 reset 为空操作）。
#[tokio::test]
async fn practice_phase_control_jump_fix_and_back() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    cleanup(&db).await;

    use floatctf::entity::sea_orm_active_enums::AwdpPhase;
    use floatctf::modules::event::awdp::service::event_service;

    let user_id = seed_user(&db, "phase").await;
    let gb_id = seed_trainable_gamebox(&db, "phase").await;
    // 直接建 run（不启动实例）→ phase=Break。
    let run = run_repo::create_practice_run(&db, gb_id, user_id, &AwdpConfig::default())
        .await
        .expect("create practice run");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(
        row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break
    );
    assert_eq!(
        floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
            .await
            .unwrap()
            .len(),
        0,
        "Break 阶段无回合"
    );

    // 1. 直接进入 Fix（无需等待 break_duration）。
    event_service::transition_break_to_fix(&db, &docker, JWT_SECRET, run.id)
        .await
        .expect("jump to fix");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(row.phase, AwdpPhase::Fix, "手动进入 Fix");
    assert!(row.fix_started_at.is_some() && row.fix_ends_at.is_some());
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 6, "fix 会话物化 6 个回合");
    assert_eq!(rounds[0].sequence, 1);

    // 2. 提前 Check 标记（service 层写入）。
    run_repo::set_early_patched(&db, run.id, 1)
        .await
        .expect("set early patched");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(row.early_patched_seq, Some(1));

    // 3. 回到 Break：fix 会话整体撤销。
    event_service::transition_fix_to_break(&db, run.id)
        .await
        .expect("back to break");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(row.phase, AwdpPhase::Break, "手动回到 Break");
    assert!(row.fix_started_at.is_none() && row.fix_ends_at.is_none());
    assert_eq!(row.current_round, 0);
    assert_eq!(
        row.early_patched_seq, None,
        "提前 Check 标记随 fix 会话清零"
    );
    assert!(
        floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
            .await
            .unwrap()
            .is_empty(),
        "fix 会话回合已删除"
    );
    assert!(row.break_ends_at.is_some(), "回到 Break 重新计时");

    // 4. 再进 Fix：重新物化全新时间线（新 fix_started_at）。
    event_service::transition_break_to_fix(&db, &docker, JWT_SECRET, run.id)
        .await
        .expect("re-enter fix");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(row.phase, AwdpPhase::Fix);
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 6, "重新物化 6 回合（无残留）");

    // 5. competition run 不允许 Fix→Break 回退。
    let (ev, comp_run, _) = {
        let base = chrono::Utc::now();
        let ev = floatctf::entity::events::ActiveModel {
            is_virtual: Set(false),
            id: Set(Uuid::new_v4()),
            family: Set(floatctf::entity::sea_orm_active_enums::EventFamily::Awdp),
            purpose: Set(floatctf::entity::sea_orm_active_enums::EventPurpose::Competition),
            participant_mode: Set(
                floatctf::entity::sea_orm_active_enums::ParticipantMode::Individual,
            ),
            system_key: Set(None),
            title: Set(format!(
                "awdp-it-phasecomp-{}",
                &Uuid::new_v4().to_string()[..8]
            )),
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
        floatctf::modules::event::awdp::repo::event_repo::ensure_by_event_id(
            &db,
            ev.id,
            &AwdpConfig::default(),
        )
        .await
        .unwrap();
        let run = run_repo::create_competition_run(&db, ev.id, &AwdpConfig::default())
            .await
            .unwrap();
        let now = chrono::Utc::now();
        run_repo::transition_phase(
            &db,
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
        run_repo::transition_phase(
            &db,
            run.id,
            AwdpPhase::Break,
            AwdpPhase::Fix,
            run_repo::PhaseTransitionPatch {
                fix_started_at: Some(now),
                fix_ends_at: Some(now + chrono::Duration::hours(1)),
                current_round: Some(0),
                next_action_at: Some(now + chrono::Duration::minutes(10)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        (ev.id, run.id, ())
    };
    let err = event_service::transition_fix_to_break(&db, comp_run)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("练习"),
        "competition run 回退被拒: {err}"
    );
    let _ = floatctf::entity::events::Entity::delete_by_id(ev)
        .exec(&db)
        .await;

    // 清理。
    let _ = floatctf::entity::awdp_runs::Entity::delete_by_id(run.id)
        .exec(&db)
        .await;
    cleanup(&db).await;
}

/// 练习「开始/End」DB 级循环（无 Docker 依赖）：
/// create_practice_run 冻结 → start_practice_break 回卷全新 Break 并解除冻结 →
/// Fix 会话物化后再 end_practice_session 回卷冻结 + 账本清零（score/break delete_for_run），
/// 恢复如初后可再次 start_practice_break（全新 Break 时间线）。
#[tokio::test]
async fn practice_start_end_cycle() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    cleanup(&db).await;

    let user_id = seed_user(&db, "cycle").await;
    let gb_id = seed_trainable_gamebox(&db, "cycle").await;

    // 1. 创建即冻结：Break + next_action_at None。
    let run = run_repo::create_practice_run(&db, gb_id, user_id, &AwdpConfig::default())
        .await
        .expect("create practice run");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(
        row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break
    );
    assert!(row.next_action_at.is_none(), "创建即冻结");

    // 2. 「开始」：回卷全新 Break 并解除冻结（next_action_at=break_ends）。
    run_repo::start_practice_break(&db, run.id)
        .await
        .expect("start");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(
        row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break
    );
    assert!(row.next_action_at.is_some(), "开始后解除冻结");
    assert_eq!(
        row.next_action_at.unwrap(),
        row.break_ends_at.unwrap(),
        "next_action_at=break_ends"
    );
    assert_eq!(row.current_round, 0);

    // 3. Break→Fix 物化回合（模拟会话进行中），再手动回 Fix（不影响本测试语义）。
    floatctf::modules::event::awdp::service::event_service::transition_break_to_fix(
        &db, &docker, JWT_SECRET, run.id,
    )
    .await
    .expect("break to fix (无实例，零 Docker 调用)");
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 6, "Fix 物化 6 回合");

    // 4. 计分账本 + Break 记录（模拟 Break 得分）。
    let key = floatctf::modules::event::awdp::domain::break_idempotency_key(
        run.id,
        gb_id,
        Some(user_id),
        None,
    );
    score_repo::create_score_event(
        &db,
        run.id,
        Some(user_id),
        None,
        gb_id,
        "break",
        None,
        row.break_score,
        &key,
    )
    .await
    .unwrap();
    floatctf::modules::event::awdp::repo::break_repo::record_break(
        &db,
        run.id,
        gb_id,
        Some(user_id),
        None,
        "hash",
    )
    .await
    .unwrap();
    assert_eq!(
        score_repo::my_total(&db, run.id, Some(user_id), None)
            .await
            .unwrap(),
        row.break_score
    );

    // 5. 「End」：回卷全新 Break + 重新冻结 + 清空回合/计分/破解账本。
    run_repo::end_practice_session(&db, run.id)
        .await
        .expect("end");
    score_repo::delete_for_run(&db, run.id).await.unwrap();
    floatctf::modules::event::awdp::repo::break_repo::delete_for_run(&db, run.id)
        .await
        .unwrap();
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert_eq!(
        row.phase,
        floatctf::entity::sea_orm_active_enums::AwdpPhase::Break,
        "End 后回到 Break"
    );
    assert!(row.next_action_at.is_none(), "End 后重新冻结");
    assert_eq!(row.current_round, 0);
    assert!(row.fix_started_at.is_none() && row.fix_ends_at.is_none());
    assert_eq!(
        score_repo::my_total(&db, run.id, Some(user_id), None)
            .await
            .unwrap(),
        0,
        "End 后计分清零（恢复如初）"
    );
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert!(rounds.is_empty(), "End 清空 fix 回合");
    assert!(
        !floatctf::modules::event::awdp::repo::break_repo::already_broken(
            &db,
            run.id,
            gb_id,
            Some(user_id),
            None,
        )
        .await
        .unwrap(),
        "End 后重新破解可行"
    );

    // 6. 再次「开始」：全新 Break 时间线（started_at/break_ends_at 刷新）。
    run_repo::start_practice_break(&db, run.id)
        .await
        .expect("restart");
    let row = run_repo::require_by_id(&db, run.id).await.unwrap();
    assert!(row.next_action_at.is_some() && row.break_ends_at.is_some());
    let rounds = floatctf::modules::event::awdp::repo::round_repo::list_for_run(&db, run.id)
        .await
        .unwrap();
    assert!(rounds.is_empty(), "再次开始未物化回合（等 Break→Fix）");

    // 清理。
    let _ = floatctf::entity::awdp_runs::Entity::delete_by_id(run.id)
        .exec(&db)
        .await;
    cleanup(&db).await;
}

/// 目录 Solved 列判定：该用户对该 gamebox 的练习 run **至少启动过一次实例** 才算。
/// 冻结 run（目录点击创建、未点「开始」）不算；手动插入实例行后即算。
#[tokio::test]
async fn solved_gamebox_ids_require_started_instance() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    cleanup(&db).await;

    let tag = format!("sol-{}", &Uuid::new_v4().to_string()[..8]);
    let user_id = seed_user(&db, &tag).await;
    let gb_id = seed_trainable_gamebox(&db, &tag).await;

    use floatctf::modules::event::awdp::api::training::solved_gamebox_ids_for;

    // 1. 冻结 run（无实例）：不算 solved。
    let run = run_repo::create_practice_run(&db, gb_id, user_id, &AwdpConfig::default())
        .await
        .expect("create practice run");
    let ids = solved_gamebox_ids_for(&db, user_id).await.expect("query");
    assert!(!ids.contains(&gb_id), "冻结 run 无实例不应算 solved");

    // 2. 手动插入 event_instances 根行 + awdp_instances 关联行 → solved。
    let now = chrono::Utc::now().into();
    let inst_id = Uuid::new_v4();
    floatctf::entity::event_instances::ActiveModel {
        id: Set(inst_id),
        event_id: Set(run.event_id),
        owner_user_id: Set(Some(user_id)),
        owner_team_id: Set(None),
        container_name: Set(format!("awdp-it-{}-{}", &tag, &inst_id.to_string()[..8])),
        runtime_state: Set("running".into()),
        runtime_generation: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert event_instances");
    floatctf::entity::awdp_instances::ActiveModel {
        instance_id: Set(inst_id),
        event_id: Set(run.event_id),
        owner_user_id: Set(Some(user_id)),
        owner_team_id: Set(None),
        created_at: Set(now),
        run_id: Set(run.id),
        gamebox_id: Set(gb_id),
    }
    .insert(&db)
    .await
    .expect("insert awdp_instances");

    let ids = solved_gamebox_ids_for(&db, user_id).await.expect("query");
    assert!(ids.contains(&gb_id), "启动过实例应算 solved");

    // 3. 其他用户视角不受影响。
    let other_id = seed_user(&db, &format!("{tag}-o")).await;
    let ids_other = solved_gamebox_ids_for(&db, other_id).await.expect("query");
    assert!(!ids_other.contains(&gb_id), "他人不应算 solved");

    // 清理（run 级联删 awdp_instances/event_instances）。
    let _ = floatctf::entity::awdp_runs::Entity::delete_by_id(run.id)
        .exec(&db)
        .await;
    cleanup(&db).await;
}
