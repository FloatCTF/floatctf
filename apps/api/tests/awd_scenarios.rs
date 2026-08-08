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
