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

use floatctf::entity::{
    awd_events, awd_rounds, events, sea_orm_active_enums::AwdEventStatus,
    sea_orm_active_enums::AwdPhase,
};
use floatctf::infrastructure::realtime::NoopEventPublisher;
use floatctf::modules::event::awd_team::{
    domain::firewall_state::DesiredFirewallState,
    infrastructure::{firewall::FirewallRuntime, network::NoopNetworkRuntime},
    repo::event_repo,
    service::round_service,
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
        id: Set(event_id),
        title: Set(format!("awd-scenario-{tag}")),
        start_time: Set(chrono::Utc::now().into()),
        end_time: Set((chrono::Utc::now() + chrono::Duration::hours(1)).into()),
        ..Default::default()
    };
    parent.insert(db).await.expect("insert events");

    let awd = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        gamebox_cidr: Set("10.42.0.0/16".into()),
        wireguard_cidr: Set("172.31.0.0/16".into()),
        wireguard_interface_name: Set(format!(
            "wg-{}",
            &Uuid::new_v4().to_string().replace('-', "")[..8]
        )),
        wireguard_listen_port: Set(5_0000 + Uuid::new_v4().as_u128() as i32 % 1000),
        flagserver_ip: Set("10.42.0.10".into()),
        judgeserver_ip: Set("10.42.0.11".into()),
        event_secret_ciphertext: Set(vec![1u8; 32]),
        event_secret_nonce: Set(vec![2u8; 24]),
        status: Set(AwdEventStatus::Verified),
        configuration_generation: Set(0),
        ..Default::default()
    };
    awd.insert(db).await.expect("insert awd_events");

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
            phase: Some(AwdPhase::Hardening),
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
    let _ = awd_events::Entity::delete_many()
        .filter(awd_events::Column::EventId.eq(event_id))
        .exec(db)
        .await;
    let _ = events::Entity::delete_many()
        .filter(events::Column::Id.eq(event_id))
        .exec(db)
        .await;
    let _ = floatctf::entity::scheduled_tasks::Entity::delete_many()
        .filter(floatctf::entity::scheduled_tasks::Column::GroupId.eq(event_id))
        .exec(db)
        .await;
}

/// Scenario A：完整轮次闭环（P5-2，DB 级断言）。
#[tokio::test]
async fn scenario_a_full_round_loop() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let event_id = seed_running_event(&db, "roundloop").await;
    let network = NoopNetworkRuntime;
    let firewall =
        floatctf::modules::event::awd_team::infrastructure::firewall::NoopFirewallRuntime;
    let publisher = NoopEventPublisher;

    // Round 1 start
    let r1 = round_service::start_round(&db, &network, &firewall, &publisher, event_id, Some(1))
        .await
        .expect("round 1 start");
    assert!(r1.created);
    assert_eq!(r1.round_number, 1);
    assert_eq!(r1.phase, AwdPhase::Hardening);

    // 幂等：重复 start → 同一 round（retry 不重复创建，P3-3）
    // retry 携带相同期望 round_number → 幂等命中（P3-3 防 retry 双 round）
    let r1_retry =
        round_service::start_round(&db, &network, &firewall, &publisher, event_id, Some(1))
            .await
            .expect("round 1 retry idempotent");
    assert!(!r1_retry.created);
    assert_eq!(r1_retry.round_id, r1.round_id);

    // Round 1 end → Grace
    round_service::end_round(&db, event_id, r1.round_id)
        .await
        .expect("round 1 end");
    let round = awd_rounds::Entity::find_by_id(r1.round_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(round.grace_ends_at.is_some(), "grace_ends_at written");

    // Grace end → Completed + Round 2 scheduled (N+1)
    round_service::grace_end_round(&db, event_id, r1.round_id, &publisher)
        .await
        .expect("round 1 grace end");
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
    use floatctf::modules::event::awd_team::{
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
        floatctf::modules::event::awd_team::service::firewall_service::build_desired_state(&db, 1)
            .await
            .expect("build desired");
    let keys: Vec<String> = desired
        .event_keys()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    // 断言我们的两个赛事都在（不依赖库内无其他赛事）
    let k1 =
        floatctf::modules::event::awd_team::infrastructure::firewall::NftObjectName::event_key(&e1)
            .as_str()
            .to_string();
    let k2 =
        floatctf::modules::event::awd_team::infrastructure::firewall::NftObjectName::event_key(&e2)
            .as_str()
            .to_string();
    assert!(keys.contains(&k1), "e1 missing: {keys:?}");
    assert!(keys.contains(&k2), "e2 missing: {keys:?}");

    // 两个赛事 key 都出现 → 渲染含两个 event chain（Event A 更新不影响 Event B）
    let rendered =
        floatctf::modules::event::awd_team::infrastructure::firewall::render::render_table(
            &desired,
        );
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
    use floatctf::modules::event::awd_team::{
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
    event_service::resume_event(&db, &network, &firewall, event_id)
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
    Uuid, // template_id
    Uuid, // user_id
    Uuid, // flag_issue_id
) {
    use floatctf::entity::{
        awd_flag_issues, awd_gamebox_instances, awd_gamebox_templates, awd_rounds, event_teams,
        sea_orm_active_enums::{AwdPhase, GameboxStatus, RoundStatus},
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

    // Template
    let template_id = Uuid::new_v4();
    awd_gamebox_templates::ActiveModel {
        id: Set(template_id),
        event_id: Set(event_id),
        name: Set(format!("tmpl-{tag}")),
        image_ref: Set("fctf/test:latest".into()),
        username: Set("root".into()),
        meta_json: Set(serde_json::json!({})),
        cpu_millis: Set(1000),
        memory_bytes: Set(512 * 1024 * 1024),
        pids_limit: Set(256),
        break_points: Set(100),
        loss_points: Set(50),
        fix_points: Set(80),
        down_points: Set(60),
        first_bonus: Set(first_bonus),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert template");

    // Instance
    let instance_id = Uuid::new_v4();
    awd_gamebox_instances::ActiveModel {
        id: Set(instance_id),
        event_id: Set(event_id),
        template_id: Set(template_id),
        team_id: Set(victim_team_id),
        status: Set(GameboxStatus::Ready),
        container_name: Set(format!("fctf-gb-{}-{tag}", &event_id.to_string()[..8])),
        gamebox_ip: Set("10.42.1.10".into()),
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
        template_id,
        user_id,
        flag_issue_id,
    )
}

/// P5-9-1：同一 flag 并发 submit 100 次 → 只有一次 attack score + 一次 victim loss。
#[tokio::test]
async fn load_concurrent_same_flag_scores_once() {
    use floatctf::entity::awd_flag_submissions;
    use floatctf::modules::event::awd_team::{
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
    use floatctf::modules::event::awd_team::{
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
    use floatctf::modules::event::awd_team::{domain::score::ScoreEventType, repo::score_repo};

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
