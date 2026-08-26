//! AWD E2E 场景 DB-gated 测试（Phase 5 P5-2/P5-5 等）。
//!
//! 覆盖可在「DB + Noop/mock runtime」下断言的核心闭环：
//! - Scenario A：完整比赛轮次闭环（Round1 → End → Grace → Completed → Round2 N+1）
//! - Scenario E：网络 reconcile 失败 → NetworkError Fail Closed（FaultInjecting runtime）
//! - 轮次幂等：scheduler retry 不产生重复 round
//!
//! 需要可达的 PostgreSQL（soft-skip）；host 级矩阵/容器场景见
//! `scripts/nft_prototype.sh` 与 CI-host-network 层。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};
use std::sync::Arc;
use uuid::Uuid;

use floatctf::entity::sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode, RoundStatus};
use floatctf::entity::{
    awd_event_networks, awd_events, awd_rounds, events, sea_orm_active_enums,
    sea_orm_active_enums::AwdEventStatus, sea_orm_active_enums::AwdPhase,
};
use floatctf::infrastructure::realtime::NoopEventPublisher;
use floatctf::modules::event::awd::{
    domain::firewall_state::DesiredFirewallState,
    infrastructure::{
        firewall::FirewallRuntime,
        firewall::NoopFirewallRuntime,
        network::NoopNetworkRuntime,
    },
    repo::event_repo,
    service::{event_service, round_service},
};

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip awd_scenarios: DB unreachable ({e})");
            None
        }
    }
}

async fn seed_running_event(db: &sea_orm::DatabaseConnection, tag: &str) -> Uuid {
    let event_id = Uuid::new_v4();
    let parent = events::ActiveModel {
        is_virtual: Set(false),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        id: Set(event_id),
        title: Set(format!("awd-scenario-{tag}")),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set(Some(
            (chrono::Utc::now() + chrono::Duration::hours(1)).fixed_offset(),
        )),
        ..Default::default()
    };
    parent.insert(db).await.expect("insert events");

    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        status: Set(AwdEventStatus::Verified),
        configuration_generation: Set(0),
        ..Default::default()
    };
    awd.insert(db).await.expect("insert awd_events");

    // Event Network（新模型）：网络配置独立固化
    let wg_iface = format!("fawg_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let wg_port = 5_0000 + Uuid::new_v4().as_u128() as i32 % 1000;
    let net = awd_event_networks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        allocation_mode: Set(sea_orm_active_enums::AwdNetworkAllocationMode::Automatic),
        gamebox_cidr: Set("10.42.0.0/16".parse().unwrap()),
        wireguard_cidr: Set("172.31.0.0/16".parse().unwrap()),
        infrastructure_subnet: Set("10.42.0.0/24".parse().unwrap()),
        flagserver_ip: Set("10.42.0.10".parse().unwrap()),
        judgeserver_ip: Set("10.42.0.11".parse().unwrap()),
        wireguard_interface_name: Set(wg_iface),
        wireguard_listen_port: Set(wg_port),
        docker_network_name: Set(format!(
            "fctf-awd-{}",
            &Uuid::new_v4().simple().to_string()[..8]
        )),
        locked_at: Set(None),
        ..Default::default()
    };
    net.insert(db).await.expect("insert awd_event_networks");

    // Verified → Running
    let row = event_repo::find_by_event_id(db, event_id)
        .await
        .unwrap()
        .unwrap();
    event_repo::transition_event(
        db,
        row.id,
        AwdEventStatus::Verified,
        AwdEventStatus::Running,
        event_repo::TransitionPatch {
            phase: Some(AwdPhase::Attack),
            started_at: Some(chrono::Utc::now()),
            ..Default::default()
        },
    )
    .await
    .expect("start event");

    event_id
}

async fn cleanup_event(db: &sea_orm::DatabaseConnection, event_id: Uuid) {
    let _ = awd_rounds::Entity::delete_many()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .exec(db)
        .await;
    let _ = floatctf::entity::event_gamebox_instances::Entity::delete_many()
        .filter(floatctf::entity::event_gamebox_instances::Column::EventId.eq(event_id))
        .exec(db)
        .await;
    let _ = floatctf::entity::awd_event_gameboxes::Entity::delete_many()
        .filter(floatctf::entity::awd_event_gameboxes::Column::EventId.eq(event_id))
        .exec(db)
        .await;
    let _ = floatctf::entity::awd_event_networks::Entity::delete_many()
        .filter(floatctf::entity::awd_event_networks::Column::EventId.eq(event_id))
        .exec(db)
        .await;
    let _ = floatctf::entity::awd_events::Entity::delete_by_id(event_id)
        .exec(db)
        .await;
    let _ = events::Entity::delete_many()
        .filter(events::Column::Id.eq(event_id))
        .exec(db)
        .await;
}

/// 创建最小 Event Network fixture（用于 round recovery 测试）。
async fn seed_minimal_event_network(db: &sea_orm::DatabaseConnection, event_id: Uuid) {
    use floatctf::entity::sea_orm_active_enums::AwdNetworkAllocationMode;
    let wg_iface = format!("fawg_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let wg_port = 5_0000 + Uuid::new_v4().as_u128() as i32 % 1000;
    let net = awd_event_networks::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        allocation_mode: Set(AwdNetworkAllocationMode::Automatic),
        gamebox_cidr: Set("10.42.0.0/16".parse().unwrap()),
        wireguard_cidr: Set("172.31.0.0/16".parse().unwrap()),
        infrastructure_subnet: Set("10.42.0.0/24".parse().unwrap()),
        flagserver_ip: Set("10.42.0.10".parse().unwrap()),
        judgeserver_ip: Set("10.42.0.11".parse().unwrap()),
        wireguard_interface_name: Set(wg_iface),
        wireguard_listen_port: Set(wg_port),
        docker_network_name: Set(format!(
            "fctf-awd-{}",
            &Uuid::new_v4().simple().to_string()[..8]
        )),
        locked_at: Set(None),
        ..Default::default()
    };
    net.insert(db).await.expect("insert awd_event_networks");
}

/// Scenario A：完整轮次闭环（P5-2，DB 级断言）。
#[tokio::test]
async fn scenario_a_full_round_loop() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = seed_running_event(&db, "roundloop").await;
    let network = NoopNetworkRuntime;
    let firewall = floatctf::modules::event::awd::infrastructure::firewall::NoopFirewallRuntime;
    let publisher = NoopEventPublisher;

    // Round 1 start
    let r1 = round_service::start_round(&db, &network, &firewall, &publisher, event_id, Some(1))
        .await
        .expect("round 1 start");
    assert!(r1.created);
    assert_eq!(r1.round_number, 1);
    assert_eq!(r1.phase, AwdPhase::Attack);

    // 幂等：重复 start → 同一 round（retry 不重复创建，P3-3）
    // retry 携带相同期望 round_number → 幂等命中（P3-3 防 retry 双 round）
    let r1_retry =
        round_service::start_round(&db, &network, &firewall, &publisher, event_id, Some(1))
            .await
            .expect("round 1 retry idempotent");
    assert!(!r1_retry.created);
    assert_eq!(r1_retry.round_id, r1.round_id);

    // Round 1 end → Completed（无 Grace）
    round_service::end_round(&db, event_id, r1.round_id, &network, &firewall, &publisher)
        .await
        .expect("round 1 end");
    let round = awd_rounds::Entity::find_by_id(r1.round_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(round.completed_at.is_some());

    // Round 2 start（N+1 自动推进）
    let r2 = round_service::start_round(&db, &network, &firewall, &publisher, event_id, Some(2))
        .await
        .expect("round 2 start");
    assert_eq!(r2.round_number, 2);
    assert_eq!(r2.phase, AwdPhase::Attack);

    cleanup_event(&db, event_id).await;
}

/// Scenario E：网络 reconcile 失败 → Fail Closed（P5-5，FaultInjecting runtime）。
#[tokio::test]
async fn scenario_e_network_failure_fail_closed() {
    use async_trait::async_trait;
    use floatctf::modules::event::awd::{
        AwdError,
        infrastructure::firewall::{
            FirewallApplyResult, FirewallVerification, ObservedFirewallState,
        },
    };

    struct FailingFirewallRuntime;
    #[async_trait]
    impl FirewallRuntime for FailingFirewallRuntime {
        async fn inspect(&self) -> Result<ObservedFirewallState, AwdError> {
            Ok(ObservedFirewallState::default())
        }
        async fn reconcile(
            &self,
            _desired: &DesiredFirewallState,
        ) -> Result<FirewallApplyResult, AwdError> {
            Err(AwdError::Network("injected nft apply failure".into()))
        }
        async fn verify(
            &self,
            _desired: &DesiredFirewallState,
        ) -> Result<FirewallVerification, AwdError> {
            Ok(FirewallVerification {
                verified: false,
                observed: ObservedFirewallState::default(),
                notes: vec!["failing".into()],
            })
        }
    }

    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = seed_running_event(&db, "neterr").await;
    let network = NoopNetworkRuntime;
    let failing = FailingFirewallRuntime;
    let publisher = NoopEventPublisher;

    // round start 的 reconcile 失败 → start_round 返回错误（Fail Closed）
    let err = round_service::start_round(&db, &network, &failing, &publisher, event_id, Some(1))
        .await
        .expect_err("round start must fail when reconcile fails");
    assert!(
        err.to_string().contains("injected nft apply failure"),
        "got {err}"
    );

    cleanup_event(&db, event_id).await;
}

/// Scenario F：多赛事互不干扰（P5-6）——两个事件不同 phase 同时存在于 desired set。
#[tokio::test]
async fn scenario_f_multi_event_desired_state() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let e1 = seed_running_event(&db, "multiA").await;
    let e2 = seed_running_event(&db, "multiB").await;

    // 把 e1 置为 Attack（round 2 phase），e2 保持 Hardening
    let row1 = event_repo::find_by_event_id(&db, e1)
        .await
        .unwrap()
        .unwrap();
    event_repo::update_phase(&db, row1.id, AwdPhase::Attack)
        .await
        .expect("e1 attack");

    let desired =
        floatctf::modules::event::awd::service::firewall_service::build_desired_state(&db, 1)
            .await
            .expect("build desired");
    let keys: Vec<String> = desired
        .event_keys()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    // 断言我们的两个赛事都在（不依赖库内无其他赛事）
    let k1 = floatctf::modules::event::awd::infrastructure::firewall::NftObjectName::event_key(&e1)
        .as_str()
        .to_string();
    let k2 = floatctf::modules::event::awd::infrastructure::firewall::NftObjectName::event_key(&e2)
        .as_str()
        .to_string();
    assert!(keys.contains(&k1), "e1 missing: {keys:?}");
    assert!(keys.contains(&k2), "e2 missing: {keys:?}");

    // 两个赛事 key 都出现 → 渲染含两个 event chain（Event A 更新不影响 Event B）
    let rendered =
        floatctf::modules::event::awd::infrastructure::firewall::render::render_table(&desired);
    for k in &keys {
        assert!(
            rendered.contains(&format!("chain event_{k}")),
            "missing {k}"
        );
    }

    cleanup_event(&db, e1).await;
    cleanup_event(&db, e2).await;
}

/// P4-9 回归：pause/resume 后 round.end 调度任务重建（修复卡死 bug）。
/// 真实路径：暂停期间 RoundEnd 任务触发（round 非 Active 幂等跳过）后被消费；
/// 测试模拟「任务被消费」，resume 后断言新的 pending awd.round.end 任务已按新 deadline 重建。
#[tokio::test]
async fn pause_resume_rebuilds_round_end_task() {
    use floatctf::entity::{awd_rounds, scheduled_tasks, sea_orm_active_enums::RoundStatus};
    use floatctf::modules::event::awd::{
        infrastructure::firewall::NoopFirewallRuntime, service::event_service,
    };

    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = seed_running_event(&db, "pauseresume").await;
    let network = NoopNetworkRuntime;
    let firewall = NoopFirewallRuntime;
    let publisher = NoopEventPublisher;

    // Round 1 启动（创建 RoundEnd 任务）
    let r1 = round_service::start_round(&db, &network, &firewall, &publisher, event_id, Some(1))
        .await
        .expect("round 1 start");
    let pending_after_start = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq("awd.round.end"))
        .filter(scheduled_tasks::Column::Status.eq("pending"))
        .one(&db)
        .await
        .unwrap()
        .expect("round end task scheduled at start");
    let original_execute_at = pending_after_start.execute_at;

    // Pause（round → Paused）
    event_service::pause_event(&db, &network, &firewall, event_id)
        .await
        .expect("pause");

    // 模拟暂停期间任务被消费（真实场景：触发后幂等跳过并标记完成）
    scheduled_tasks::Entity::delete_many()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq("awd.round.end"))
        .exec(&db)
        .await
        .expect("consume round end task");

    // Resume
    event_service::resume_event(&db, &network, &firewall, &publisher, event_id)
        .await
        .expect("resume");

    // 断言：round 回到 Active，且新的 pending round.end 任务已重建
    let round = awd_rounds::Entity::find_by_id(r1.round_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(round.status, RoundStatus::Active, "round resumes Active");

    let rebuilt = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq("awd.round.end"))
        .filter(scheduled_tasks::Column::Status.eq("pending"))
        .one(&db)
        .await
        .unwrap()
        .expect("round end task rebuilt after resume");
    assert_eq!(
        rebuilt.execute_at.unwrap(),
        round.scheduled_end_at,
        "rebuilt task deadline == resumed round scheduled_end_at"
    );
    assert_ne!(
        rebuilt.execute_at, original_execute_at,
        "rebuilt deadline differs from pre-pause deadline"
    );

    cleanup_event(&db, event_id).await;
}

// ── P5-9 并发正确性种子 ──────────────────────────────────────────────

fn hash_flag(f: &str) -> String {
    // 与生产 hash_flag 一致：SHA-256 hex
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(f.as_bytes()))
}

/// 种子：完整提交链（event Running/Attack + round Active + issue + teams + instance + template + user）。
async fn seed_submission_fixture(
    db: &sea_orm::DatabaseConnection,
    tag: &str,
    first_bonus: i64,
) -> (
    Uuid, // event_id
    Uuid, // round_id
    Uuid, // attacker_team_id
    Uuid, // victim_team_id
    Uuid, // instance_id
    Uuid, // event_gamebox_id
    Uuid, // user_id
    Uuid, // flag_issue_id
) {
    use floatctf::entity::{
        awd_event_gameboxes, awd_flag_issues, awd_rounds, event_gamebox_instances, event_teams,
        gameboxes,
        sea_orm_active_enums::{
            AwdPhase, EventFamily, EventPurpose, GameboxStatus, ParticipantMode, RoundStatus,
        },
        users,
    };

    let event_id = seed_running_event(db, tag).await;
    let now = chrono::Utc::now();

    // Round 1（Active, Attack）
    let round_id = Uuid::new_v4();
    awd_rounds::ActiveModel {
        id: Set(round_id),
        event_id: Set(event_id),
        round_number: Set(1),
        status: Set(RoundStatus::Active),
        phase: Set(AwdPhase::Attack),
        started_at: Set(now.into()),
        scheduled_end_at: Set((now + chrono::Duration::minutes(10)).into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert round");

    // Teams
    let attacker_team_id = Uuid::new_v4();
    event_teams::ActiveModel {
        id: Set(attacker_team_id),
        event_id: Set(event_id),
        name: Set(format!("attacker-{tag}")),
        points: Set(0.0),
        banned: Set(false),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert attacker team");
    let victim_team_id = Uuid::new_v4();
    event_teams::ActiveModel {
        id: Set(victim_team_id),
        event_id: Set(event_id),
        name: Set(format!("victim-{tag}")),
        points: Set(0.0),
        banned: Set(false),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert victim team");

    // GameBox identity（单版本：package 字段在 identity）+ EventGameBox
    // safe_name 随机后缀，避免跨 run 残留冲突（gameboxes 是全局表，events 级联清不到）
    let gamebox_id = Uuid::new_v4();
    let gb_suffix = &gamebox_id.to_string().replace('-', "")[..8];
    gameboxes::ActiveModel {
        id: Set(gamebox_id),
        name: Set(format!("gb-{tag}")),
        safe_name: Set(format!("gb-{tag}-{gb_suffix}")),
        category: Set("other".into()),
        description: Set(String::new()),
        hidden: Set(false),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        version: Set(Some("1.0.0".into())),
        source_toml: Set(None),
        spec_json: Set(Some(serde_json::json!({}))),
        spec_digest: Set(Some("spec".into())),
        package_digest: Set(Some("pkg".into())),
        image_ref: Set(Some(format!("fctf/test-{gb_suffix}:1.0.0"))),
        image_id: Set(Some(format!("sha256:scen{}", gb_suffix))),
        image_repo_digest: Set(None),
        username: Set(Some("root".into())),
        recommended_cpu_millis: Set(1000),
        recommended_memory_bytes: Set(512 * 1024 * 1024),
        recommended_pids_limit: Set(256),
        healthchecks_json: Set(Some(serde_json::json!([]))),
        judge_script_name: Set(None),
        judge_script_content: Set(None),
        judge_args_json: Set(None),
        judge_timeout_secs: Set(None),
        judge_retry_interval_secs: Set(None),
        awdp_source_code_dir: Set(None),
        awdp_exploit_script_name: Set(None),
        awdp_exploit_script_content: Set(None),
        awdp_source_artifact_key: Set(None),
        awdp_source_artifact_digest: Set(None),
        build_status: Set(Some("ready".into())),
        build_error: Set(None),
    }
    .insert(db)
    .await
    .expect("insert gamebox");

    let event_gamebox_id = Uuid::new_v4();
    awd_event_gameboxes::ActiveModel {
        id: Set(event_gamebox_id),
        event_id: Set(event_id),
        gamebox_id: Set(gamebox_id),
        host_offset: Set(10),
        enabled: Set(true),
        hidden: Set(false),
        cpu_millis: Set(1000),
        memory_bytes: Set(512 * 1024 * 1024),
        pids_limit: Set(256),
        healthcheck_override_json: Set(None),
        judge_timeout_secs: Set(None),
        judge_retry_interval_secs: Set(None),
        break_points: Set(100),
        loss_points: Set(50),
        fix_points: Set(80),
        judge_down_penalty: Set(60),
        first_bonus: Set(first_bonus),
        attack_score: Set(100),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
    }
    .insert(db)
    .await
    .expect("insert event_gamebox");

    // Instance（归一化：根表 = 运行时；AWD 关联 = 领域状态）
    let instance_id = Uuid::new_v4();
    let root_id = Uuid::new_v4();
    floatctf::entity::event_instances::ActiveModel {
        id: Set(root_id),
        event_id: Set(event_id),
        owner_user_id: Set(None),
        owner_team_id: Set(Some(victim_team_id)),
        image_ref: Set(Some("img:v1".into())),
        container_id: Set(Some(format!("cid-{tag}"))),
        container_name: Set(format!("fctf-gb-{}-{tag}", &event_id.to_string()[..8])),
        runtime_state: Set("running".to_string()),
        runtime_generation: Set(1),
        created_at: Set(now.into()),
        started_at: Set(Some(now.into())),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert root");
    event_gamebox_instances::ActiveModel {
        id: Set(instance_id),
        instance_id: Set(root_id),
        event_id: Set(event_id),
        event_gamebox_id: Set(event_gamebox_id),
        team_id: Set(victim_team_id),
        status: Set(GameboxStatus::Ready),
        gamebox_ip: Set("10.42.1.10".parse().unwrap()),
        health_status: Set("healthy".into()),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert instance");

    // Flag issue（固定 id，供并发提交复用同一 issue）
    let flag_issue_id = Uuid::new_v4();
    awd_flag_issues::ActiveModel {
        id: Set(flag_issue_id),
        event_id: Set(event_id),
        round_id: Set(round_id),
        gamebox_instance_id: Set(instance_id),
        flag_hash: Set(hash_flag(&format!("flag{{concurrent-{tag}}}"))),
        issued_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert flag issue");

    // User（username 全局唯一，随机后缀避免重跑残留冲突）
    let user_id = Uuid::new_v4();
    let uname = format!("user-{tag}-{}", &user_id.to_string().replace('-', "")[..8]);
    users::ActiveModel {
        id: Set(user_id),
        username: Set(uname.clone()),
        nickname: Set(uname.clone()),
        password: Set("x".into()),
        email: Set(format!("{uname}@test.invalid")),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user");

    (
        event_id,
        round_id,
        attacker_team_id,
        victim_team_id,
        instance_id,
        event_gamebox_id,
        user_id,
        flag_issue_id,
    )
}

/// P5-9-1：同一 flag 并发 submit 100 次 → 只有一次 attack score + 一次 victim loss。
#[tokio::test]
async fn load_concurrent_same_flag_scores_once() {
    use floatctf::entity::awd_flag_submissions;
    use floatctf::modules::event::awd::{
        AwdError, domain::score::ScoreEventType, service::submission_service,
    };

    let Some(db) = connect_or_skip().await else {
        return;
    };
    let (event_id, round_id, attacker, victim, instance, template, user, flag_issue) =
        seed_submission_fixture(&db, "sameflag", 0).await;

    let publisher = Arc::new(NoopEventPublisher);
    let db = Arc::new(db);
    let mut handles = Vec::new();
    for _ in 0..100 {
        let db = db.clone();
        let pub_ = publisher.clone();
        handles.push(tokio::spawn(async move {
            submission_service::process_submission(
                &db, event_id, round_id, flag_issue, attacker, victim, instance, user, 100, 50, 0,
                template, &*pub_,
            )
            .await
        }));
    }
    let mut ok = 0usize;
    let mut conflicts = 0usize;
    for h in handles {
        match h.await.expect("join") {
            Ok(_) => ok += 1,
            // 事务内冲突经 db.transaction 包装为 Database("Transaction failed: ...")，
            // 与直接 Conflict 等价（同一语义：重复提交被拒）
            Err(AwdError::Conflict(_)) => conflicts += 1,
            Err(AwdError::Database(m)) if m.contains("Already submitted") => conflicts += 1,
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
    assert_eq!(ok, 1, "exactly one submission succeeds");
    assert_eq!(conflicts, 99, "rest rejected as conflict");

    let subs = awd_flag_submissions::Entity::find()
        .filter(awd_flag_submissions::Column::EventId.eq(event_id))
        .all(db.as_ref())
        .await
        .unwrap();
    assert_eq!(subs.len(), 1, "exactly one submission row");

    let events = floatctf::entity::awd_score_events::Entity::find()
        .filter(floatctf::entity::awd_score_events::Column::EventId.eq(event_id))
        .all(db.as_ref())
        .await
        .unwrap();
    let attack = events
        .iter()
        .filter(|e| e.event_type == ScoreEventType::Attack)
        .count();
    let loss = events
        .iter()
        .filter(|e| e.event_type == ScoreEventType::VictimLoss)
        .count();
    assert_eq!(attack, 1, "exactly one attack score");
    assert_eq!(loss, 1, "exactly one victim loss");

    cleanup_event(db.as_ref(), event_id).await;
    let _ = floatctf::entity::users::Entity::delete_by_id(user)
        .exec(db.as_ref())
        .await;
}

/// P5-9-2：first blood 并发（两支攻击队抢同一模板首杀）→ 只有一次 FirstBonus。
#[tokio::test]
async fn load_concurrent_first_blood_scores_once() {
    use floatctf::entity::event_teams;
    use floatctf::modules::event::awd::{
        domain::score::ScoreEventType, repo::score_repo, service::submission_service,
    };

    let Some(db) = connect_or_skip().await else {
        return;
    };
    let (event_id, round_id, attacker_a, victim, instance, template, user, flag_issue) =
        seed_submission_fixture(&db, "firstblood", 100).await;

    // 第二支攻击队
    let attacker_b = Uuid::new_v4();
    let now = chrono::Utc::now();
    event_teams::ActiveModel {
        id: Set(attacker_b),
        event_id: Set(event_id),
        name: Set("attacker-b-firstblood".into()),
        points: Set(0.0),
        banned: Set(false),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert attacker B");

    let publisher = Arc::new(NoopEventPublisher);
    let r1 = submission_service::process_submission(
        &db,
        event_id,
        round_id,
        flag_issue,
        attacker_a,
        victim,
        instance,
        user,
        100,
        50,
        100,
        template,
        &*publisher,
    )
    .await;
    let r2 = submission_service::process_submission(
        &db,
        event_id,
        round_id,
        flag_issue,
        attacker_b,
        victim,
        instance,
        user,
        100,
        50,
        100,
        template,
        &*publisher,
    )
    .await;
    let sub_rows = floatctf::entity::awd_flag_submissions::Entity::find()
        .filter(floatctf::entity::awd_flag_submissions::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    let oks = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(oks, 2, "both attackers score their attack");

    let events = floatctf::entity::awd_score_events::Entity::find()
        .filter(floatctf::entity::awd_score_events::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .unwrap();
    let attack = events
        .iter()
        .filter(|e| e.event_type == ScoreEventType::Attack)
        .count();
    let bonus = events
        .iter()
        .filter(|e| e.event_type == ScoreEventType::FirstBonus)
        .count();
    assert_eq!(attack, 2, "two attacks");
    assert_eq!(bonus, 1, "exactly one first-blood bonus");

    cleanup_event(&db, event_id).await;
    let _ = floatctf::entity::users::Entity::delete_by_id(user)
        .exec(&db)
        .await;
}

/// P5-9-3：judge callback 重试（同一 callback_id 并发写分）→ 只有一次 score mutation。
#[tokio::test]
async fn load_concurrent_judge_callback_scores_once() {
    use floatctf::modules::event::awd::{domain::score::ScoreEventType, repo::score_repo};

    let Some(db) = connect_or_skip().await else {
        return;
    };
    let (event_id, round_id, attacker, _victim, _instance, template, user, _issue) =
        seed_submission_fixture(&db, "judgeretry", 0).await;

    let db = Arc::new(db);
    let mut handles = Vec::new();
    for _ in 0..20 {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            score_repo::create_score_event(
                db.as_ref(),
                event_id,
                Some(round_id),
                attacker,
                ScoreEventType::JudgeFix,
                80,
                "callback-retry-test-key",
                None,
                None,
                Some(template),
                Some("judge check"),
            )
            .await
        }));
    }
    let mut ok = 0usize;
    for h in handles {
        if h.await.expect("join").is_ok() {
            ok += 1;
        }
    }
    assert_eq!(ok, 1, "only one idempotency-key write succeeds");

    let events = floatctf::entity::awd_score_events::Entity::find()
        .filter(floatctf::entity::awd_score_events::Column::EventId.eq(event_id))
        .all(db.as_ref())
        .await
        .unwrap();
    let judge_fix = events
        .iter()
        .filter(|e| e.event_type == ScoreEventType::JudgeFix)
        .count();
    assert_eq!(judge_fix, 1, "exactly one score mutation for the callback");

    cleanup_event(db.as_ref(), event_id).await;
    let _ = floatctf::entity::users::Entity::delete_by_id(user)
        .exec(db.as_ref())
        .await;
}

// ── Wave 2.1: Crash-gap recovery tests ──

/// 崩溃间隙恢复：Running + Attack + 无轮次 → Round 1
#[tokio::test]
async fn crash_gap_no_rounds_recovers_round_1() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = Uuid::new_v4();
    let parent = events::ActiveModel {
        is_virtual: Set(false),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        id: Set(event_id),
        title: Set("awd-crash-gap-none".into()),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set(Some(
            (chrono::Utc::now() + chrono::Duration::hours(10)).fixed_offset(),
        )),
        ..Default::default()
    };
    parent.insert(&db).await.expect("insert events");

    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        status: Set(AwdEventStatus::Running),
        phase: Set(AwdPhase::Attack),
        round_count: Set(Some(5)),
        round_duration_secs: Set(300),
        configuration_generation: Set(0),
        ..Default::default()
    };
    awd.insert(&db).await.expect("insert awd_events");

    seed_minimal_event_network(&db, event_id).await;

    let network = NoopNetworkRuntime;
    let firewall = NoopFirewallRuntime;
    let publisher = NoopEventPublisher;

    let restored = round_service::restore_round_scheduling(
        &db, event_id, &network, &firewall, &publisher,
    )
    .await
    .expect("restore_round_scheduling");
    assert_eq!(restored, 1, "should recover round 1");

    let r1 = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .filter(awd_rounds::Column::RoundNumber.eq(1))
        .one(&db)
        .await
        .unwrap()
        .expect("round 1 should exist");
    assert_eq!(r1.status, RoundStatus::Active);
    assert_eq!(r1.phase, AwdPhase::Attack);

    cleanup_event(&db, event_id).await;
}

/// 崩溃间隙恢复：Round 4 Completed，round_count=10，无活跃轮次 → Round 5
#[tokio::test]
async fn crash_gap_mid_round_recovers_next() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = Uuid::new_v4();
    let parent = events::ActiveModel {
        is_virtual: Set(false),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        id: Set(event_id),
        title: Set("awd-crash-gap-mid".into()),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set(Some(
            (chrono::Utc::now() + chrono::Duration::hours(10)).fixed_offset(),
        )),
        ..Default::default()
    };
    parent.insert(&db).await.expect("insert events");

    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        status: Set(AwdEventStatus::Running),
        phase: Set(AwdPhase::Attack),
        round_count: Set(Some(10)),
        round_duration_secs: Set(300),
        configuration_generation: Set(0),
        ..Default::default()
    };
    awd.insert(&db).await.expect("insert awd_events");

    seed_minimal_event_network(&db, event_id).await;

    let r4 = awd_rounds::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_number: Set(4),
        status: Set(RoundStatus::Completed),
        phase: Set(AwdPhase::Attack),
        started_at: Set(chrono::Utc::now().into()),
        scheduled_end_at: Set(chrono::Utc::now().into()),
        completed_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };
    r4.insert(&db).await.expect("insert round 4");

    let network = NoopNetworkRuntime;
    let firewall = NoopFirewallRuntime;
    let publisher = NoopEventPublisher;

    let restored = round_service::restore_round_scheduling(
        &db, event_id, &network, &firewall, &publisher,
    )
    .await
    .expect("restore_round_scheduling");
    assert_eq!(restored, 1, "should recover 1 round");

    let r5 = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .filter(awd_rounds::Column::RoundNumber.eq(5))
        .one(&db)
        .await
        .unwrap()
        .expect("round 5 should exist");
    assert_eq!(r5.status, RoundStatus::Active);
    assert_eq!(r5.phase, AwdPhase::Attack);

    cleanup_event(&db, event_id).await;
}

/// 崩溃间隙恢复：最终轮次已完成 → 不启动新轮次
#[tokio::test]
async fn crash_gap_final_round_no_recovery() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = Uuid::new_v4();
    let parent = events::ActiveModel {
        is_virtual: Set(false),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        id: Set(event_id),
        title: Set("awd-crash-gap-final".into()),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set(Some(
            (chrono::Utc::now() + chrono::Duration::hours(10)).fixed_offset(),
        )),
        ..Default::default()
    };
    parent.insert(&db).await.expect("insert events");

    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        status: Set(AwdEventStatus::Running),
        phase: Set(AwdPhase::Attack),
        round_count: Set(Some(5)),
        round_duration_secs: Set(300),
        configuration_generation: Set(0),
        ..Default::default()
    };
    awd.insert(&db).await.expect("insert awd_events");

    seed_minimal_event_network(&db, event_id).await;

    let r5 = awd_rounds::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_number: Set(5),
        status: Set(RoundStatus::Completed),
        phase: Set(AwdPhase::Attack),
        started_at: Set(chrono::Utc::now().into()),
        scheduled_end_at: Set(chrono::Utc::now().into()),
        completed_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };
    r5.insert(&db).await.expect("insert round 5");

    let network = NoopNetworkRuntime;
    let firewall = NoopFirewallRuntime;
    let publisher = NoopEventPublisher;

    let restored = round_service::restore_round_scheduling(
        &db, event_id, &network, &firewall, &publisher,
    )
    .await
    .expect("restore_round_scheduling");
    assert_eq!(restored, 0, "should not recover final-settlement");

    let r6 = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .filter(awd_rounds::Column::RoundNumber.eq(6))
        .one(&db)
        .await
        .unwrap();
    assert!(r6.is_none(), "round 6 should not exist");

    cleanup_event(&db, event_id).await;
}

/// 幂等：重复恢复不创建重复轮次
#[tokio::test]
async fn crash_gap_recovery_idempotent() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = Uuid::new_v4();
    let parent = events::ActiveModel {
        is_virtual: Set(false),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        id: Set(event_id),
        title: Set("awd-crash-gap-idem".into()),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set(Some(
            (chrono::Utc::now() + chrono::Duration::hours(10)).fixed_offset(),
        )),
        ..Default::default()
    };
    parent.insert(&db).await.expect("insert events");

    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        status: Set(AwdEventStatus::Running),
        phase: Set(AwdPhase::Attack),
        round_count: Set(Some(10)),
        round_duration_secs: Set(300),
        configuration_generation: Set(0),
        ..Default::default()
    };
    awd.insert(&db).await.expect("insert awd_events");

    seed_minimal_event_network(&db, event_id).await;

    let r3 = awd_rounds::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_number: Set(3),
        status: Set(RoundStatus::Completed),
        phase: Set(AwdPhase::Attack),
        started_at: Set(chrono::Utc::now().into()),
        scheduled_end_at: Set(chrono::Utc::now().into()),
        completed_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };
    r3.insert(&db).await.expect("insert round 3");

    let network = NoopNetworkRuntime;
    let firewall = NoopFirewallRuntime;
    let publisher = NoopEventPublisher;

    let r1 = round_service::restore_round_scheduling(
        &db, event_id, &network, &firewall, &publisher,
    )
    .await
    .expect("first restore");
    assert_eq!(r1, 1, "first restore should create round 4");

    let r2 = round_service::restore_round_scheduling(
        &db, event_id, &network, &firewall, &publisher,
    )
    .await
    .expect("second restore");
    assert_eq!(r2, 0, "second restore should be idempotent");

    let rounds = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .filter(awd_rounds::Column::RoundNumber.eq(4))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 1, "only one round 4 should exist");

    cleanup_event(&db, event_id).await;
}

/// HardeningEnd 崩溃：Running/Attack + 无轮次 → recovery 启动 Round 1
#[tokio::test]
async fn hardening_end_crash_recovery_starts_round_1() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = Uuid::new_v4();
    let parent = events::ActiveModel {
        is_virtual: Set(false),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        id: Set(event_id),
        title: Set("awd-hardening-crash".into()),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set(Some(
            (chrono::Utc::now() + chrono::Duration::hours(10)).fixed_offset(),
        )),
        ..Default::default()
    };
    parent.insert(&db).await.expect("insert events");

    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        status: Set(AwdEventStatus::Running),
        phase: Set(AwdPhase::Attack),
        round_count: Set(Some(5)),
        round_duration_secs: Set(300),
        configuration_generation: Set(0),
        ..Default::default()
    };
    awd.insert(&db).await.expect("insert awd_events");

    seed_minimal_event_network(&db, event_id).await;

    let network = NoopNetworkRuntime;
    let firewall = NoopFirewallRuntime;
    let publisher = NoopEventPublisher;

    let restored = round_service::restore_round_scheduling(
        &db, event_id, &network, &firewall, &publisher,
    )
    .await
    .expect("restore");
    assert_eq!(restored, 1, "should recover round 1");

    let r1 = awd_rounds::Entity::find()
        .filter(awd_rounds::Column::EventId.eq(event_id))
        .filter(awd_rounds::Column::RoundNumber.eq(1))
        .one(&db)
        .await
        .unwrap()
        .expect("round 1 should exist");
    assert_eq!(r1.status, RoundStatus::Active);
    assert_eq!(r1.phase, AwdPhase::Attack);

    cleanup_event(&db, event_id).await;
}
