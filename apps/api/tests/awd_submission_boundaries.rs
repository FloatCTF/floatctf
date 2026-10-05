//! AWD Flag 提交边界测试（DB-gated）。
//!
//! 覆盖业务主流程 E2E 覆盖不到的提交守卫分支：
//! - 阶段边界：Hardening / Pause 阶段提交必须被拒绝（仅 Attack 允许）；
//! - 状态边界：NetworkError / Finished / Archived 提交必须被拒绝；
//! - 轮次边界：没有 active round 时提交被拒绝；
//! - 时效边界：上一轮的 flag 在轮换后提交必须被拒绝；
//! - 自攻击边界：提交本队 GameBox 的 flag 必须被拒绝；
//! - 正向对照：Running + Attack + active round 的合法攻击必须成功。
//!
//! 这些断言与 `flag_service::validate_submission` 的守卫顺序一致：
//! 状态 → 阶段 → active round → flag 时效 → 目标归属 → 封禁。

use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use uuid::Uuid;

use floatctf::entity::sea_orm_active_enums::{
    AwdEventStatus, AwdPhase, EventFamily, EventPurpose, GameboxStatus, ParticipantMode,
    RoundStatus,
};
use floatctf::entity::{
    awd_event_gameboxes, awd_events, awd_flag_issues, awd_flag_submissions, awd_rounds,
    event_gamebox_instances, event_instances, event_teams, events, gameboxes,
};
use floatctf::modules::event::awd::{
    AwdError, domain::flag, repo::flag_repo, service::flag_service,
};

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip awd_submission_boundaries: DB unreachable ({e})");
            None
        }
    }
}

struct Fixture {
    db: DatabaseConnection,
    event_id: Uuid,
    team_a_id: Uuid,
    team_b_id: Uuid,
    instance_a_id: Uuid,
    instance_b_id: Uuid,
    gamebox_id: Uuid,
}

impl Fixture {
    async fn cleanup(&self) {
        use sea_orm::{ColumnTrait, QueryFilter};

        let _ = awd_flag_submissions::Entity::delete_many()
            .filter(awd_flag_submissions::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = awd_flag_issues::Entity::delete_many()
            .filter(awd_flag_issues::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = awd_rounds::Entity::delete_many()
            .filter(awd_rounds::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = event_gamebox_instances::Entity::delete_many()
            .filter(event_gamebox_instances::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = event_instances::Entity::delete_many()
            .filter(event_instances::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = awd_event_gameboxes::Entity::delete_many()
            .filter(awd_event_gameboxes::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = event_teams::Entity::delete_many()
            .filter(event_teams::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = awd_events::Entity::delete_many()
            .filter(awd_events::Column::EventId.eq(self.event_id))
            .exec(&self.db)
            .await;
        let _ = events::Entity::delete_by_id(self.event_id)
            .exec(&self.db)
            .await;
        let _ = gameboxes::Entity::delete_by_id(self.gamebox_id)
            .exec(&self.db)
            .await;
    }
}

/// 播种一个最小 AWD 赛事：状态 / 阶段 / 轮次由调用方指定。
/// 返回 fixture 与按传入顺序创建的轮次 id。
async fn seed(
    status: AwdEventStatus,
    phase: AwdPhase,
    rounds: &[(i32, RoundStatus)],
) -> Option<(Fixture, Vec<Uuid>)> {
    let db = connect_or_skip().await?;
    let now = chrono::Utc::now();
    let suffix = Uuid::new_v4()
        .to_string()
        .split('-')
        .next()
        .unwrap()
        .to_string();
    let event_id = Uuid::new_v4();

    events::ActiveModel {
        id: Set(event_id),
        title: Set(format!("Submission Boundary {suffix}")),
        description: Set(Some("submission boundary fixture".into())),
        start_time: Set(now.into()),
        end_time: Set(Some((now + chrono::Duration::hours(2)).into())),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert generic event");

    awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        status: Set(status),
        phase: Set(phase),
        round_count: Set(Some(2)),
        round_duration_secs: Set(300),
        initial_score: Set(1000),
        free_reset_count: Set(1),
        extra_reset_penalty: Set(100),
        judge_max_concurrency: Set(2),
        judge_default_timeout_secs: Set(10),
        judge_retry_interval_secs: Set(2),
        judge_grace_period_secs: Set(2),
        archive_retention_hours: Set(1),
        verified_at: Set(Some(now.into())),
        verified_generation: Set(Some(1)),
        event_secret_ciphertext: Set(vec![0u8; 32]),
        event_secret_nonce: Set(vec![0u8; 12]),
        key_version: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert awd event");

    let team_a_id = Uuid::new_v4();
    let team_b_id = Uuid::new_v4();
    for (team_id, name) in [(team_a_id, "Team A"), (team_b_id, "Team B")] {
        event_teams::ActiveModel {
            id: Set(team_id),
            event_id: Set(event_id),
            name: Set(name.into()),
            banned: Set(false),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert event team");
    }

    let gamebox_id = Uuid::new_v4();
    gameboxes::ActiveModel {
        id: Set(gamebox_id),
        name: Set(format!("boundary-gb-{suffix}")),
        safe_name: Set(format!("boundary-gb-{suffix}")),
        category: Set("other".into()),
        hidden: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert gamebox");

    let event_gamebox_id = Uuid::new_v4();
    awd_event_gameboxes::ActiveModel {
        id: Set(event_gamebox_id),
        event_id: Set(event_id),
        gamebox_id: Set(gamebox_id),
        attack_score: Set(100),
        judge_down_penalty: Set(40),
        first_bonus: Set(20),
        host_offset: Set(2),
        enabled: Set(true),
        hidden: Set(false),
        cpu_millis: Set(250),
        memory_bytes: Set(128 * 1024 * 1024),
        pids_limit: Set(64),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("insert event gamebox");

    let mut instance_a_id = Uuid::nil();
    let mut instance_b_id = Uuid::nil();
    for (team_id, slot, ip) in [
        (team_a_id, &mut instance_a_id, "10.42.1.5/32"),
        (team_b_id, &mut instance_b_id, "10.42.2.5/32"),
    ] {
        let root_id = Uuid::new_v4();
        let instance_id = Uuid::new_v4();
        *slot = instance_id;

        event_instances::ActiveModel {
            id: Set(root_id),
            event_id: Set(event_id),
            owner_team_id: Set(Some(team_id)),
            container_name: Set(format!("boundary-{}", &root_id.simple().to_string()[..8])),
            container_id: Set(Some(format!(
                "docker-{}",
                &root_id.simple().to_string()[..8]
            ))),
            runtime_generation: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert event instance");

        event_gamebox_instances::ActiveModel {
            id: Set(instance_id),
            event_id: Set(event_id),
            team_id: Set(team_id),
            event_gamebox_id: Set(event_gamebox_id),
            instance_id: Set(root_id),
            status: Set(GameboxStatus::Ready),
            gamebox_ip: Set(ip.parse::<ipnetwork::IpNetwork>().unwrap()),
            health_status: Set("healthy".into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert gamebox instance");
    }

    let mut round_ids = Vec::new();
    for (number, round_status) in rounds {
        let round_id = Uuid::new_v4();
        awd_rounds::ActiveModel {
            id: Set(round_id),
            event_id: Set(event_id),
            round_number: Set(*number),
            status: Set(round_status.clone()),
            phase: Set(AwdPhase::Attack),
            started_at: Set(now.into()),
            scheduled_end_at: Set((now + chrono::Duration::seconds(300)).into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("insert round");
        round_ids.push(round_id);
    }

    Some((
        Fixture {
            db,
            event_id,
            team_a_id,
            team_b_id,
            instance_a_id,
            instance_b_id,
            gamebox_id,
        },
        round_ids,
    ))
}

/// 为指定实例在指定轮次签发一个可提交的 flag。
async fn issue_flag(fixture: &Fixture, round_id: Uuid, instance_id: Uuid) -> String {
    let issued = flag::generate_flag(
        &[7u8; 32],
        &fixture.event_id.to_string(),
        &round_id.to_string(),
        &instance_id.to_string(),
        "flag{",
    );
    flag_repo::find_or_create_issue(
        &fixture.db,
        fixture.event_id,
        round_id,
        instance_id,
        &flag::hash_flag(&issued),
    )
    .await
    .expect("create flag issue");
    issued
}

async fn submit(fixture: &Fixture, flag: &str) -> Result<(Uuid, Uuid, Uuid), AwdError> {
    flag_service::validate_submission(
        &fixture.db,
        fixture.event_id,
        flag,
        fixture.team_a_id,
        Uuid::new_v4(),
    )
    .await
}

#[tokio::test]
async fn valid_attack_accepted_in_attack_phase_with_active_round() {
    let Some((fixture, rounds)) = seed(
        AwdEventStatus::Running,
        AwdPhase::Attack,
        &[(1, RoundStatus::Active)],
    )
    .await
    else {
        return;
    };

    let flag = issue_flag(&fixture, rounds[0], fixture.instance_b_id).await;
    let (_, victim_team, victim_instance) = submit(&fixture, &flag)
        .await
        .expect("valid attack must be accepted");
    assert_eq!(victim_team, fixture.team_b_id);
    assert_eq!(victim_instance, fixture.instance_b_id);

    fixture.cleanup().await;
}

#[tokio::test]
async fn submission_rejected_during_hardening() {
    let Some((fixture, _)) = seed(AwdEventStatus::Running, AwdPhase::Hardening, &[]).await else {
        return;
    };

    let err = submit(&fixture, "flag{not-issued-yet}")
        .await
        .expect_err("hardening must reject flag submission");
    assert!(matches!(err, AwdError::Forbidden(_)), "got {err:?}");
    assert!(
        err.to_string().contains("not allowed in current phase"),
        "got {err}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn submission_rejected_while_paused() {
    let Some((fixture, _)) = seed(AwdEventStatus::Paused, AwdPhase::Pause, &[]).await else {
        return;
    };

    let err = submit(&fixture, "flag{not-issued-yet}")
        .await
        .expect_err("pause must reject flag submission");
    assert!(matches!(err, AwdError::Forbidden(_)), "got {err:?}");
    assert!(
        err.to_string().contains("not allowed in current phase"),
        "got {err}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn submission_rejected_after_network_error() {
    let Some((fixture, _)) = seed(AwdEventStatus::NetworkError, AwdPhase::Attack, &[]).await else {
        return;
    };

    let err = submit(&fixture, "flag{not-issued-yet}")
        .await
        .expect_err("network error must reject flag submission");
    assert!(matches!(err, AwdError::Forbidden(_)), "got {err:?}");
    assert!(err.to_string().contains("not running"), "got {err}");

    fixture.cleanup().await;
}

#[tokio::test]
async fn submission_rejected_after_finished() {
    let Some((fixture, rounds)) = seed(
        AwdEventStatus::Finished,
        AwdPhase::Attack,
        &[(1, RoundStatus::Completed)],
    )
    .await
    else {
        return;
    };

    // 即使 flag 仍然有效，终态也不允许提交。
    let flag = issue_flag(&fixture, rounds[0], fixture.instance_b_id).await;
    let err = submit(&fixture, &flag)
        .await
        .expect_err("finished event must reject flag submission");
    assert!(matches!(err, AwdError::Forbidden(_)), "got {err:?}");
    assert!(err.to_string().contains("not running"), "got {err}");

    fixture.cleanup().await;
}

#[tokio::test]
async fn submission_rejected_after_archived() {
    let Some((fixture, _)) = seed(AwdEventStatus::Archived, AwdPhase::Attack, &[]).await else {
        return;
    };

    let err = submit(&fixture, "flag{not-issued-yet}")
        .await
        .expect_err("archived event must reject flag submission");
    assert!(matches!(err, AwdError::Forbidden(_)), "got {err:?}");

    fixture.cleanup().await;
}

#[tokio::test]
async fn submission_requires_active_round() {
    let Some((fixture, rounds)) = seed(
        AwdEventStatus::Running,
        AwdPhase::Attack,
        &[(1, RoundStatus::Completed)],
    )
    .await
    else {
        return;
    };

    let flag = issue_flag(&fixture, rounds[0], fixture.instance_b_id).await;
    let err = submit(&fixture, &flag)
        .await
        .expect_err("no active round must reject flag submission");
    assert!(matches!(err, AwdError::NotFound(_)), "got {err:?}");
    assert!(err.to_string().contains("No active round"), "got {err}");

    fixture.cleanup().await;
}

#[tokio::test]
async fn stale_flag_from_previous_round_rejected() {
    let Some((fixture, rounds)) = seed(
        AwdEventStatus::Running,
        AwdPhase::Attack,
        &[(1, RoundStatus::Completed), (2, RoundStatus::Active)],
    )
    .await
    else {
        return;
    };

    // 第 1 轮的 flag 在第 2 轮必须失效（flag 轮换边界）。
    let stale = issue_flag(&fixture, rounds[0], fixture.instance_b_id).await;
    let err = submit(&fixture, &stale)
        .await
        .expect_err("stale flag must be rejected after rotation");
    assert!(matches!(err, AwdError::NotFound(_)), "got {err:?}");
    assert!(err.to_string().contains("flag 无效或已过期"), "got {err}");

    // 第 2 轮签发的 flag 仍然可用，确认拒绝原因只与轮次相关。
    let fresh = issue_flag(&fixture, rounds[1], fixture.instance_b_id).await;
    submit(&fixture, &fresh)
        .await
        .expect("fresh round flag must be accepted");

    fixture.cleanup().await;
}

#[tokio::test]
async fn self_attack_rejected() {
    let Some((fixture, rounds)) = seed(
        AwdEventStatus::Running,
        AwdPhase::Attack,
        &[(1, RoundStatus::Active)],
    )
    .await
    else {
        return;
    };

    let own_flag = issue_flag(&fixture, rounds[0], fixture.instance_a_id).await;
    let err = submit(&fixture, &own_flag)
        .await
        .expect_err("self attack must be rejected");
    assert!(matches!(err, AwdError::Forbidden(_)), "got {err:?}");
    assert!(
        err.to_string().contains("不能提交本队自己的 flag"),
        "got {err}"
    );

    fixture.cleanup().await;
}
