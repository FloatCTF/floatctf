//! AWDP 运行时 + Break 集成测试（DB + Docker + 本地 test-g 镜像 gated）。
//!
//! 覆盖（plan §47）：on-demand 启动 / healthcheck 端点发布 / 端点跨重启稳定 /
//! flag 正确得分一次 / 重复 flag 不再得分 / 错误 flag 拒绝 / Team 共享实例。
//!
//! 前置：本地镜像 `floatctf/gameboxes/test-g:1.0.3`（examples/test_g v1.0.3，
//! flag.php 提供 FLAG env）。无镜像或 DB 时跳过。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::entity::{
    awdp_event_gameboxes, events, gameboxes,
    sea_orm_active_enums::{AwdpPhase, EventFamily, EventPurpose, ParticipantMode},
};
use floatctf::modules::event::awdp::{
    domain::AwdpConfig,
    repo::{event_gamebox_repo, event_repo, score_repo},
    service::{
        break_service,
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

async fn cleanup(db: &sea_orm::DatabaseConnection, event_id: Uuid) {
    // events 级联删除 awdp 子表；gameboxes 是全局表需单独清理。
    let _ = events::Entity::delete_by_id(event_id).exec(db).await;
    for row in gameboxes::Entity::find()
        .filter(gameboxes::Column::SafeName.like("awdp-it-gb-%"))
        .all(db)
        .await
        .unwrap()
    {
        let _ = gameboxes::Entity::delete_by_id(row.id).exec(db).await;
    }
}

/// 建 AWDP competition individual 事件（Break 已开始）。
async fn seed_event_in_break(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
    let base = chrono::Utc::now();
    let event = events::ActiveModel {
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Individual),
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
    let now = chrono::Utc::now();
    event_repo::transition_phase(
        db,
        event.id,
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
    event.id
}

#[tokio::test]
async fn individual_start_flag_break_and_idempotency() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let Some(docker) = docker_or_skip() else {
        return;
    };
    // 前置：test-g 镜像存在。
    let rt = fcmc::DockerContainerRuntime::new(docker.clone());
    if fcmc::ImageRuntime::inspect_image(&rt, IMAGE_REF)
        .await
        .is_err()
    {
        eprintln!("skip: image {IMAGE_REF} not present");
        return;
    }

    let event_id = seed_event_in_break(&db, "ind").await;
    let (_gb_id, eg_id) = {
        // attach 需要走真实 repo（校验 [awdp] capability）。
        let gb_id = Uuid::new_v4();
        let now = chrono::Utc::now().into();
        let gb = gameboxes::ActiveModel {
            id: Set(gb_id),
            name: Set("awdp-gb-ind".into()),
            safe_name: Set(format!(
                "awdp-it-gb-ind-{}",
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
        .insert(&db)
        .await
        .unwrap();
        let eg = event_gamebox_repo::attach_gamebox(&db, event_id, gb.id, false)
            .await
            .expect("attach awdp gamebox");
        (gb.id, eg.id)
    };

    let user_id = seed_user(&db, "ind").await;
    let subject = Subject::user(user_id);

    // 启动实例 → 端点发布。
    let view = runtime::start_instance(&db, &docker, JWT_SECRET, event_id, eg_id, subject, "flag")
        .await
        .expect("start instance");
    assert_eq!(view.runtime_state, "running");
    assert_eq!(view.endpoints.len(), 1);
    let ep = &view.endpoints[0];
    assert_eq!(ep.protocol, "http");
    assert_eq!(ep.container_port, 80);
    assert!(ep.public_port > 0);

    // 幂等：再次 start 返回同一实例与同一端点。
    let view2 = runtime::start_instance(&db, &docker, JWT_SECRET, event_id, eg_id, subject, "flag")
        .await
        .expect("restart idempotent");
    assert_eq!(view2.instance_id, view.instance_id);
    assert_eq!(
        view2.endpoints[0].public_port, ep.public_port,
        "endpoint 稳定"
    );

    // 从真实容器取 flag（flag.php 提供 FLAG env）并提交。
    let flag = {
        let url = format!("http://{}:{}/flag.php", ep.public_host, ep.public_port);
        let body = reqwest::get(&url)
            .await
            .expect("fetch flag")
            .text()
            .await
            .unwrap();
        body.trim().to_string()
    };
    assert!(flag.starts_with("flag{"), "flag from box: {flag}");

    let r1 = break_service::submit_flag(&db, JWT_SECRET, event_id, eg_id, &flag, subject)
        .await
        .expect("submit flag");
    assert!(r1.accepted && r1.scored, "first break scores: {r1:?}");

    let r2 = break_service::submit_flag(&db, JWT_SECRET, event_id, eg_id, &flag, subject)
        .await
        .expect("duplicate submit");
    assert!(
        r2.accepted && !r2.scored && r2.already_broken,
        "dup no score: {r2:?}"
    );

    let r3 = break_service::submit_flag(&db, JWT_SECRET, event_id, eg_id, "flag{wrong}", subject)
        .await
        .expect("wrong flag");
    assert!(!r3.accepted && !r3.scored, "wrong flag rejected: {r3:?}");

    // 分数恰好一次。
    let total = score_repo::my_total(&db, event_id, Some(user_id), None)
        .await
        .unwrap();
    let awdp = event_repo::require_by_event_id(&db, event_id)
        .await
        .unwrap();
    assert_eq!(total, awdp.break_score, "break_score 恰好一次");

    // 停止后端点保留。
    runtime::stop_instance(&db, &docker, view.instance_id, subject)
        .await
        .expect("stop");
    let after = runtime::get_my_instance_view(&db, event_id, eg_id, subject)
        .await
        .unwrap()
        .expect("view after stop");
    assert_eq!(after.runtime_state, "stopped");
    assert_eq!(
        after.endpoints[0].public_port, ep.public_port,
        "stop 保留端点"
    );

    // 他人 flag 不可用（不同 subject 不同 flag → rejected）。
    let other_user = seed_user(&db, "other").await;
    let other_subject = Subject::user(other_user);
    let r_other =
        break_service::submit_flag(&db, JWT_SECRET, event_id, eg_id, &flag, other_subject)
            .await
            .expect("other subject");
    assert!(!r_other.accepted, "other subject cannot use my flag");

    cleanup(&db, event_id).await;
}

#[tokio::test]
async fn team_shares_instance_and_score() {
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

    // Team 事件（competition team）。
    let base = chrono::Utc::now();
    let event = events::ActiveModel {
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        title: Set("awdp-it-team".into()),
        description: Set(None),
        start_time: Set((base - chrono::Duration::minutes(5)).into()),
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
    event_repo::transition_phase(
        &db,
        event.id,
        AwdpPhase::Pending,
        AwdpPhase::Break,
        event_repo::PhaseTransitionPatch {
            started_at: Some(chrono::Utc::now()),
            next_action_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // team + 两个成员。
    use floatctf::entity::{event_team_members, event_teams};
    let team_id = Uuid::new_v4();
    let now = chrono::Utc::now().into();
    event_teams::ActiveModel {
        id: Set(team_id),
        event_id: Set(event.id),
        name: Set("it-team".into()),
        description: Set(None),
        points: Set(0.0),
        banned: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let uids = [seed_user(&db, "m1").await, seed_user(&db, "m2").await];
    for (i, uid) in uids.iter().enumerate() {
        event_team_members::ActiveModel {
            event_id: Set(event.id),
            team_id: Set(team_id),
            user_id: Set(*uid),
            role: Set(if i == 0 {
                floatctf::entity::sea_orm_active_enums::EventTeamMemberRole::Captain
            } else {
                floatctf::entity::sea_orm_active_enums::EventTeamMemberRole::Member
            }),
            joined_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
    }

    // gamebox attach（复用 seed 逻辑）。
    let gb_id = Uuid::new_v4();
    let gb = gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set("awdp-gb-team".into()),
        safe_name: Set(format!(
            "awdp-it-gb-team-{}",
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
    .insert(&db)
    .await
    .unwrap();
    let eg = event_gamebox_repo::attach_gamebox(&db, event.id, gb.id, false)
        .await
        .expect("attach");

    let subject = Subject::team(team_id);
    let view = runtime::start_instance(&db, &docker, JWT_SECRET, event.id, eg.id, subject, "flag")
        .await
        .expect("team instance");

    // 第二个成员看到的实例与第一个一致。
    let view2 = runtime::get_my_instance_view(&db, event.id, eg.id, Subject::team(team_id))
        .await
        .unwrap()
        .expect("team view");
    assert_eq!(view2.instance_id, view.instance_id, "team 共享实例");

    // team 提交 flag 得分。
    let ep = &view.endpoints[0];
    let flag = reqwest::get(&format!(
        "http://{}:{}/flag.php",
        ep.public_host, ep.public_port
    ))
    .await
    .unwrap()
    .text()
    .await
    .unwrap()
    .trim()
    .to_string();
    let r = break_service::submit_flag(&db, JWT_SECRET, event.id, eg.id, &flag, subject)
        .await
        .expect("team submit");
    assert!(r.accepted && r.scored, "team break: {r:?}");
    let total = score_repo::my_total(&db, event.id, None, Some(team_id))
        .await
        .unwrap();
    let awdp = event_repo::require_by_event_id(&db, event.id)
        .await
        .unwrap();
    assert_eq!(total, awdp.break_score);

    cleanup(&db, event.id).await;
}
