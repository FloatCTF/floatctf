//! Event Domain invariant regression tests (DB-gated).
//!
//! Covers ownership FKs, participant_mode↔team_id, system_key immutability,
//! AWD parent/allocation family guards, Practice bootstrap, and team duplicate solve.
//!
//! Soft-skip when PostgreSQL is unreachable. Prefer rolled-back transactions.

use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait,
    PaginatorTrait, QueryFilter, Statement, TransactionTrait,
};
use uuid::Uuid;

use floatctf::entity::{
    awd_events, awd_network_allocations, challenges, event_challenge_instance, event_instances,
    event_teams, events, jeopardy_challenge_solves, jeopardy_event_challenges,
    sea_orm_active_enums::{
        AwdEventStatus, AwdNetworkAllocationKind, AwdPhase, EventFamily, EventPurpose,
        ParticipantMode,
    },
    users,
};
use floatctf::modules::event::common::application::admin_service::{
    self as common_admin, PatchEventRequest,
};
use floatctf::modules::event::common::domain::event_mode::{
    PRACTICE_JEOPARDY_EVENT_ID, PRACTICE_JEOPARDY_SYSTEM_KEY,
};
use floatctf::modules::event::common::domain::practice_event::{
    ensure_practice_jeopardy_event, find_practice_jeopardy_event,
};

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(error) => {
            eprintln!("skip event_domain_invariants: DB unreachable ({error})");
            None
        }
    }
}

fn assert_db_err(err: sea_orm::DbErr, needle: &str) {
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains(&needle.to_lowercase())
            || msg.contains("check")
            || msg.contains("violat")
            || msg.contains("immutable")
            || msg.contains("assert_"),
        "expected error containing '{needle}', got: {msg}"
    );
}

async fn exec_sql<C: ConnectionTrait>(db: &C, sql: &str) -> Result<(), sea_orm::DbErr> {
    db.execute(Statement::from_string(
        DatabaseBackend::Postgres,
        sql.to_string(),
    ))
    .await
    .map(|_| ())
}

/// Run `f` under a SAVEPOINT so a deliberate DB error does not abort the outer txn.
async fn with_savepoint<F, T>(
    txn: &sea_orm::DatabaseTransaction,
    name: &str,
    f: F,
) -> Result<T, sea_orm::DbErr>
where
    F: std::future::Future<Output = Result<T, sea_orm::DbErr>>,
{
    exec_sql(txn, &format!("SAVEPOINT {name}")).await?;
    match f.await {
        Ok(v) => {
            exec_sql(txn, &format!("RELEASE SAVEPOINT {name}")).await?;
            Ok(v)
        }
        Err(e) => {
            let _ = exec_sql(txn, &format!("ROLLBACK TO SAVEPOINT {name}")).await;
            Err(e)
        }
    }
}

async fn seed_user<C: ConnectionTrait>(db: &C, tag: &str) -> users::Model {
    let id = Uuid::new_v4();
    users::ActiveModel {
        id: Set(id),
        username: Set(format!("u-{tag}-{}", id.simple())),
        nickname: Set(format!("n-{tag}-{}", id.simple())),
        password: Set("x".into()),
        email: Set(format!("{tag}-{}@example.test", id.simple())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert user")
}

async fn seed_challenge<C: ConnectionTrait>(db: &C, tag: &str) -> challenges::Model {
    let id = Uuid::new_v4();
    challenges::ActiveModel {
        id: Set(id),
        name: Set(format!("ch-{tag}-{}", id.simple())),
        safe_name: Set(format!("ch-{tag}-{}", id.simple())),
        category: Set("web".into()),
        description: Set("test".into()),
        hidden: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert challenge")
}

async fn seed_jeopardy_event<C: ConnectionTrait>(
    db: &C,
    participant: ParticipantMode,
    tag: &str,
) -> events::Model {
    let id = Uuid::new_v4();
    let now = Utc::now();
    events::ActiveModel {
        is_virtual: Set(false),
        id: Set(id),
        family: Set(EventFamily::Jeopardy),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(participant),
        system_key: Set(None),
        title: Set(format!("evt-domain-{tag}-{}", id.simple())),
        hidden: Set(false),
        allow_join: Set(true),
        start_time: Set((now - Duration::hours(1)).into()),
        end_time: Set(Some((now + Duration::hours(4)).fixed_offset())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert jeopardy event")
}

async fn seed_team<C: ConnectionTrait>(db: &C, event_id: Uuid, name: &str) -> event_teams::Model {
    event_teams::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        name: Set(name.into()),
        points: Set(0.0),
        banned: Set(false),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insert team")
}

// ── DB combination / immutable ─────────────────────────────────────────────

#[tokio::test]
async fn db_rejects_illegal_mode_combination() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");
    let now = Utc::now();
    let res = events::ActiveModel {
        is_virtual: Set(false),
        id: Set(Uuid::new_v4()),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(None),
        title: Set("illegal-mode".into()),
        start_time: Set(now.into()),
        end_time: Set(Some((now + Duration::hours(1)).fixed_offset())),
        ..Default::default()
    }
    .insert(&txn)
    .await;
    assert!(res.is_err(), "illegal Awd/Individual must fail");
    assert_db_err(res.err().unwrap(), "mode");
    txn.rollback().await.ok();
}

#[tokio::test]
async fn db_rejects_identity_field_updates() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");
    let event = seed_jeopardy_event(&txn, ParticipantMode::Individual, "imm").await;

    for (i, (sql_set, needle)) in [
        ("family = 'awd'", "immutable"),
        ("purpose = 'practice'", "immutable"),
        ("participant_mode = 'team'", "immutable"),
    ]
    .into_iter()
    .enumerate()
    {
        let sp = format!("sp_id_{i}");
        let res = with_savepoint(&txn, &sp, async {
            exec_sql(
                &txn,
                &format!("UPDATE events SET {sql_set} WHERE id = '{}'", event.id),
            )
            .await
        })
        .await;
        assert!(res.is_err(), "UPDATE {sql_set} should fail");
        assert_db_err(res.err().unwrap(), needle);
    }
    txn.rollback().await.ok();
}

#[tokio::test]
async fn db_rejects_system_key_update_all_directions() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");
    let event = seed_jeopardy_event(&txn, ParticipantMode::Individual, "syskey").await;

    // NULL → value
    let res = with_savepoint(&txn, "sp_sys_null", async {
        exec_sql(
            &txn,
            &format!(
                "UPDATE events SET system_key = 'practice:foo' WHERE id = '{}'",
                event.id
            ),
        )
        .await
    })
    .await;
    assert!(res.is_err(), "NULL→value system_key must fail");
    assert_db_err(res.err().unwrap(), "system_key");

    // existing → other (on practice row if present)
    if let Some(practice) = find_practice_jeopardy_event(&txn)
        .await
        .expect("find practice")
    {
        let res = with_savepoint(&txn, "sp_sys_other", async {
            exec_sql(
                &txn,
                &format!(
                    "UPDATE events SET system_key = 'practice:other' WHERE id = '{}'",
                    practice.id
                ),
            )
            .await
        })
        .await;
        assert!(res.is_err(), "value→other system_key must fail");
        assert_db_err(res.err().unwrap(), "system_key");

        let res = with_savepoint(&txn, "sp_sys_clear", async {
            exec_sql(
                &txn,
                &format!(
                    "UPDATE events SET system_key = NULL WHERE id = '{}'",
                    practice.id
                ),
            )
            .await
        })
        .await;
        assert!(res.is_err(), "value→NULL system_key must fail");
        assert_db_err(res.err().unwrap(), "system_key");
    }

    txn.rollback().await.ok();
}

// ── Cross-event team ownership ─────────────────────────────────────────────

#[tokio::test]
async fn db_rejects_cross_event_team_instance_and_solve() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");

    let event_a = seed_jeopardy_event(&txn, ParticipantMode::Team, "a").await;
    let event_b = seed_jeopardy_event(&txn, ParticipantMode::Team, "b").await;
    let team_b = seed_team(&txn, event_b.id, "team-b").await;
    let user = seed_user(&txn, "cross").await;
    let challenge = seed_challenge(&txn, "cross").await;

    // Attach challenge to event A so family/challenge FKs ok; team is from B.
    jeopardy_event_challenges::ActiveModel {
        event_id: Set(event_a.id),
        challenge_id: Set(challenge.id),
        points: Set(100.0),
        hidden: Set(false),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .expect("event challenge");

    let inst = with_savepoint(&txn, "sp_cross_inst", async {
        event_challenge_instance::ActiveModel {
            id: Set(Uuid::new_v4()),
            flag: Set("flag{x}".into()),
            challenge_id: Set(challenge.id),
            user_id: Set(user.id),
            event_id: Set(event_a.id),
            team_id: Set(Some(team_b.id)),
            ..Default::default()
        }
        .insert(&txn)
        .await
    })
    .await;
    assert!(inst.is_err(), "cross-event team instance must fail");
    assert_db_err(inst.err().unwrap(), "foreign");

    let solve = with_savepoint(&txn, "sp_cross_solve", async {
        jeopardy_challenge_solves::ActiveModel {
            event_id: Set(event_a.id),
            challenge_id: Set(challenge.id),
            user_id: Set(user.id),
            team_id: Set(Some(team_b.id)),
            obtained_points: Set(0.0),
            bonus_points: Set(0.0),
            ..Default::default()
        }
        .insert(&txn)
        .await
    })
    .await;
    assert!(solve.is_err(), "cross-event team solve must fail");
    assert_db_err(solve.err().unwrap(), "foreign");

    txn.rollback().await.ok();
}

// ── participant_mode ↔ team_id ─────────────────────────────────────────────

#[tokio::test]
async fn db_enforces_participant_team_presence() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");

    let indiv = seed_jeopardy_event(&txn, ParticipantMode::Individual, "indiv").await;
    let team_evt = seed_jeopardy_event(&txn, ParticipantMode::Team, "team").await;
    // Team row under individual event (allowed on event_teams) to isolate participant guard.
    let indiv_team = seed_team(&txn, indiv.id, "indiv-t").await;
    let team = seed_team(&txn, team_evt.id, "t1").await;
    let user = seed_user(&txn, "pm").await;
    let challenge = seed_challenge(&txn, "pm").await;

    for eid in [indiv.id, team_evt.id] {
        jeopardy_event_challenges::ActiveModel {
            event_id: Set(eid),
            challenge_id: Set(challenge.id),
            points: Set(50.0),
            hidden: Set(false),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .expect("jec");
    }

    // Individual + team_id NOT NULL → reject (participant ownership)
    let bad_inst = with_savepoint(&txn, "sp_indiv_inst", async {
        event_challenge_instance::ActiveModel {
            id: Set(Uuid::new_v4()),
            flag: Set("flag{i}".into()),
            challenge_id: Set(challenge.id),
            user_id: Set(user.id),
            event_id: Set(indiv.id),
            team_id: Set(Some(indiv_team.id)),
            ..Default::default()
        }
        .insert(&txn)
        .await
    })
    .await;
    assert!(bad_inst.is_err());
    assert_db_err(bad_inst.err().unwrap(), "individual");

    let bad_solve = with_savepoint(&txn, "sp_indiv_solve", async {
        jeopardy_challenge_solves::ActiveModel {
            event_id: Set(indiv.id),
            challenge_id: Set(challenge.id),
            user_id: Set(user.id),
            team_id: Set(Some(indiv_team.id)),
            obtained_points: Set(0.0),
            bonus_points: Set(0.0),
            ..Default::default()
        }
        .insert(&txn)
        .await
    })
    .await;
    assert!(bad_solve.is_err());
    assert_db_err(bad_solve.err().unwrap(), "individual");
    let _ = team;

    // Team + team_id NULL → reject
    let null_inst = with_savepoint(&txn, "sp_team_null_inst", async {
        event_challenge_instance::ActiveModel {
            id: Set(Uuid::new_v4()),
            flag: Set("flag{t}".into()),
            challenge_id: Set(challenge.id),
            user_id: Set(user.id),
            event_id: Set(team_evt.id),
            team_id: Set(None),
            ..Default::default()
        }
        .insert(&txn)
        .await
    })
    .await;
    assert!(null_inst.is_err());
    assert_db_err(null_inst.err().unwrap(), "team_id");

    let null_solve = with_savepoint(&txn, "sp_team_null_solve", async {
        jeopardy_challenge_solves::ActiveModel {
            event_id: Set(team_evt.id),
            challenge_id: Set(challenge.id),
            user_id: Set(user.id),
            team_id: Set(None),
            obtained_points: Set(0.0),
            bonus_points: Set(0.0),
            ..Default::default()
        }
        .insert(&txn)
        .await
    })
    .await;
    assert!(null_solve.is_err());
    assert_db_err(null_solve.err().unwrap(), "team_id");

    txn.rollback().await.ok();
}

// ── AWD family guards ──────────────────────────────────────────────────────

#[tokio::test]
async fn db_rejects_awd_events_under_jeopardy_parent() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");
    let jeop = seed_jeopardy_event(&txn, ParticipantMode::Individual, "awd-parent").await;

    let res = awd_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(jeop.id),
        status: Set(AwdEventStatus::Draft),
        phase: Set(AwdPhase::Hardening),
        event_secret_ciphertext: Set(vec![1; 32]),
        event_secret_nonce: Set(vec![2; 24]),
        ..Default::default()
    }
    .insert(&txn)
    .await;
    assert!(res.is_err(), "awd_events under jeopardy must fail");
    assert_db_err(res.err().unwrap(), "assert_event_family");
    txn.rollback().await.ok();
}

#[tokio::test]
async fn db_awd_network_allocation_family_guard() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");

    let jeop = seed_jeopardy_event(&txn, ParticipantMode::Individual, "alloc-j").await;
    let bad = with_savepoint(&txn, "sp_alloc_bad", async {
        awd_network_allocations::ActiveModel {
            id: Set(Uuid::new_v4()),
            event_id: Set(jeop.id),
            kind: Set(AwdNetworkAllocationKind::Gamebox),
            cidr: Set("10.200.0.0/24".parse().expect("cidr")),
            ..Default::default()
        }
        .insert(&txn)
        .await
    })
    .await;
    assert!(bad.is_err(), "allocation → Jeopardy must fail");
    assert_db_err(bad.err().unwrap(), "assert_event_family");

    // AWD event without awd_events row should still accept allocation.
    let awd_id = Uuid::new_v4();
    let now = Utc::now();
    events::ActiveModel {
        is_virtual: Set(false),
        id: Set(awd_id),
        family: Set(EventFamily::Awd),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        title: Set(format!("alloc-awd-{}", awd_id.simple())),
        start_time: Set(now.into()),
        end_time: Set(Some((now + Duration::hours(2)).fixed_offset())),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .expect("awd parent");

    let ok = awd_network_allocations::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(awd_id),
        kind: Set(AwdNetworkAllocationKind::Wireguard),
        cidr: Set("10.201.0.0/24".parse().expect("cidr")),
        ..Default::default()
    }
    .insert(&txn)
    .await;
    assert!(
        ok.is_ok(),
        "allocation → AWD before awd_events must succeed: {ok:?}"
    );

    txn.rollback().await.ok();
}

// ── Practice bootstrap ─────────────────────────────────────────────────────

#[tokio::test]
async fn practice_bootstrap_idempotent_and_fields() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    // Use connection (not rolled back) so ensure can see unique index; cleanup is reselect.
    let first = ensure_practice_jeopardy_event(&db)
        .await
        .expect("ensure first");
    let second = ensure_practice_jeopardy_event(&db)
        .await
        .expect("ensure second");
    assert_eq!(first.id, second.id);
    assert_eq!(first.id, PRACTICE_JEOPARDY_EVENT_ID);
    assert_eq!(
        first.system_key.as_deref(),
        Some(PRACTICE_JEOPARDY_SYSTEM_KEY)
    );
    assert_eq!(first.family, EventFamily::Jeopardy);
    assert_eq!(first.purpose, EventPurpose::Practice);
    assert_eq!(first.participant_mode, ParticipantMode::Individual);
    assert!(first.hidden);
    assert!(!first.allow_join);
    // 练习模式即虚拟赛事（is_virtual 与 purpose 一致性由 CHECK 约束保证）。
    assert!(first.is_virtual);
    assert!(first.end_time.is_none());

    let count = events::Entity::find()
        .filter(events::Column::SystemKey.eq(PRACTICE_JEOPARDY_SYSTEM_KEY))
        .count(&db)
        .await
        .expect("count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn practice_bootstrap_unique_conflict_reselects() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    // Ensure exists, then simulate race: second ensure must reselect same row.
    let a = ensure_practice_jeopardy_event(&db).await.expect("a");
    let b = ensure_practice_jeopardy_event(&db).await.expect("b");
    assert_eq!(a.id, b.id);
    // Concurrent-ish sequential calls still yield one row.
    let c1 = ensure_practice_jeopardy_event(&db).await.expect("c1");
    let c2 = ensure_practice_jeopardy_event(&db).await.expect("c2");
    assert_eq!(c1.id, c2.id);
    assert_eq!(c1.id, a.id);
}

// ── Admin system event patch protection ────────────────────────────────────

#[tokio::test]
async fn admin_patch_rejects_system_managed_event() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let practice = ensure_practice_jeopardy_event(&db).await.expect("practice");
    let err = common_admin::patch_event(
        &db,
        practice.id,
        PatchEventRequest {
            title: Some("hacked".into()),
            description: None,
            hidden: Some(false),
            allow_join: Some(true),
            rules: None,
            flag_prefix: None,
            start_time: None,
            end_time: None,
        },
    )
    .await;
    assert!(err.is_err(), "system event patch must fail");
    let msg = err.err().unwrap().to_string();
    assert!(
        msg.contains("SystemManagedEvent") || msg.to_lowercase().contains("system"),
        "unexpected error: {msg}"
    );
}

// ── Practice solve regression (0 points, no duplicate) ─────────────────────

#[tokio::test]
async fn practice_solve_records_once_zero_points() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");
    let practice = ensure_practice_jeopardy_event(&txn)
        .await
        .expect("practice");
    let user = seed_user(&txn, "prac-solve").await;
    let challenge = seed_challenge(&txn, "prac-solve").await;
    let flag = "flag{practice}";

    let instance_id = Uuid::new_v4();
    event_instances::ActiveModel {
        id: Set(instance_id),
        event_id: Set(practice.id),
        owner_user_id: Set(Some(user.id)),
        owner_team_id: Set(None),
        image_ref: Set(None),
        container_id: Set(None),
        container_name: Set(format!("JP-test-{}", instance_id.simple())),
        runtime_state: Set("running".to_string()),
        runtime_generation: Set(1),
        created_at: Set(Utc::now().into()),
        started_at: Set(Some(Utc::now().into())),
        expires_at: Set(Some((Utc::now() + Duration::hours(1)).into())),
        updated_at: Set(Utc::now().into()),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .expect("instance runtime");
    let instance = event_challenge_instance::ActiveModel {
        id: Set(instance_id),
        flag: Set(flag.into()),
        challenge_id: Set(challenge.id),
        user_id: Set(user.id),
        event_id: Set(practice.id),
        team_id: Set(None),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .expect("instance");

    // First solve insert (mirrors submit_practice path without Docker destroy).
    jeopardy_challenge_solves::ActiveModel {
        event_id: Set(practice.id),
        challenge_id: Set(challenge.id),
        user_id: Set(user.id),
        team_id: Set(None),
        obtained_points: Set(0.0),
        bonus_points: Set(0.0),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .expect("first solve");

    // Duplicate individual unique must fail at DB (app short-circuits before insert).
    let dup = with_savepoint(&txn, "sp_prac_dup", async {
        jeopardy_challenge_solves::ActiveModel {
            event_id: Set(practice.id),
            challenge_id: Set(challenge.id),
            user_id: Set(user.id),
            team_id: Set(None),
            obtained_points: Set(0.0),
            bonus_points: Set(0.0),
            ..Default::default()
        }
        .insert(&txn)
        .await
    })
    .await;
    assert!(dup.is_err(), "duplicate practice solve must fail unique");

    let rows = jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(practice.id))
        .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge.id))
        .filter(jeopardy_challenge_solves::Column::UserId.eq(user.id))
        .all(&txn)
        .await
        .expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].obtained_points, 0.0);
    assert_eq!(rows[0].bonus_points, 0.0);
    assert!(rows[0].team_id.is_none());
    assert_eq!(rows[0].event_id, practice.id);
    let _ = instance; // keep seeded instance for family/ownership path

    txn.rollback().await.ok();
}

/// 目录 Solved 列判定：个人 solve + 团队 solve（队友提交，队长经 event_team_members 也算）。
#[tokio::test]
async fn solved_challenge_ids_include_individual_and_team_solves() {
    use floatctf::entity::{event_team_members, sea_orm_active_enums::EventTeamMemberRole};
    use floatctf::modules::challenge::catalog::player::solved_challenge_ids_for;

    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = db.begin().await.expect("begin");

    let practice = ensure_practice_jeopardy_event(&txn)
        .await
        .expect("practice");
    let a = seed_user(&txn, "sol-a").await;
    let b = seed_user(&txn, "sol-b").await;
    let c = seed_user(&txn, "sol-c").await;
    let ch1 = seed_challenge(&txn, "sol-1").await;
    let ch2 = seed_challenge(&txn, "sol-2").await;

    // 1. 个人 solve（练习事件）→ 本人算、无关用户不算。
    jeopardy_challenge_solves::ActiveModel {
        event_id: Set(practice.id),
        challenge_id: Set(ch1.id),
        user_id: Set(a.id),
        team_id: Set(None),
        obtained_points: Set(0.0),
        bonus_points: Set(0.0),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .expect("individual solve");

    let solved_a = solved_challenge_ids_for(&txn, a.id).await.expect("a");
    assert!(solved_a.contains(&ch1.id), "个人 solve 应算");
    let solved_c = solved_challenge_ids_for(&txn, c.id).await.expect("c");
    assert!(!solved_c.contains(&ch1.id), "无关用户不算");

    // 2. 团队 solve：b 提交（team_id 指向队伍），a 与 b 同队 → a 视角也算。
    let ev = seed_jeopardy_event(&txn, ParticipantMode::Team, "sol-team").await;
    let team = seed_team(&txn, ev.id, "sol-team").await;
    for (i, uid) in [a.id, b.id].iter().enumerate() {
        event_team_members::ActiveModel {
            event_id: Set(ev.id),
            team_id: Set(team.id),
            user_id: Set(*uid),
            role: Set(if i == 0 {
                EventTeamMemberRole::Captain
            } else {
                EventTeamMemberRole::Member
            }),
            joined_at: Set(Utc::now().into()),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .expect("member");
    }
    jeopardy_challenge_solves::ActiveModel {
        event_id: Set(ev.id),
        challenge_id: Set(ch2.id),
        user_id: Set(b.id),
        team_id: Set(Some(team.id)),
        obtained_points: Set(100.0),
        bonus_points: Set(0.0),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .expect("team solve");

    let solved_a2 = solved_challenge_ids_for(&txn, a.id).await.expect("a2");
    assert!(solved_a2.contains(&ch2.id), "队友解题，队长视角应算");
    let solved_c2 = solved_challenge_ids_for(&txn, c.id).await.expect("c2");
    assert!(!solved_c2.contains(&ch2.id), "未入队用户不算");

    txn.rollback().await.expect("rollback");
}

/// 回归（练习复练）：identifier 对 (event, user, challenge) 是确定性的（`JP-{user}-{challenge}`），
/// 销毁后行保留为 completed 且容器名仍占着唯一索引，再次启动同一题会撞
/// `event_instances_container_name_uidx` → 400（修复前复练被阻塞）。
/// 非 Docker 题（container_port None）全流程不触 Docker，可 DB-gated 直测。
/// 用 `#[actix_web::test]`：launch 会 `actix_web::rt::spawn` 自动销毁任务，需 actix 运行时。
#[actix_web::test]
async fn practice_relaunch_after_destroy_removes_completed_row() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let docker = match bollard::Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("skip practice_relaunch: docker client ({e})");
            return;
        }
    };

    use floatctf::modules::event::jeopardy::{
        application::instance_service::InstanceService,
        infrastructure::container_runtime::DockerInstanceRuntime,
    };

    let practice = ensure_practice_jeopardy_event(&db)
        .await
        .expect("practice event");
    let user = seed_user(&db, "relaunch").await;
    let mut ch = seed_challenge(&db, "relaunch").await;
    challenges::ActiveModel {
        id: Set(ch.id),
        flag_type: Set(Some("static".into())),
        static_flag_value: Set(Some(format!("flag{{relaunch-{}}}", ch.id.simple()))),
        build_status: Set(Some("ready".into())),
        ..Default::default()
    }
    .update(&db)
    .await
    .expect("make static non-docker challenge");

    let service = InstanceService::with_docker(db.clone(), docker);

    let identifier = format!(
        "JP-{}-{}",
        &user.id.to_string()[..8],
        &ch.id.to_string()[..8]
    );

    // 1. 第一次启动（非 Docker：content 空、无容器调用）。
    let first = service
        .launch(practice.id, ch.id, identifier.clone(), user.id, None, None)
        .await
        .expect("first launch");
    let first_id = first.id;

    // 2. 销毁 → runtime_state completed（非 Docker：不调 stop_and_remove）。
    service
        .destroy_owned(first_id, user.id)
        .await
        .expect("destroy");
    let state = event_instances::Entity::find_by_id(first_id)
        .one(&db)
        .await
        .expect("row")
        .expect("exists");
    assert_eq!(state.runtime_state, "completed", "销毁后应为 completed");

    // 3. 再次启动同一题（修复前：撞 event_instances_container_name_uidx 报错）。
    let second = service
        .launch(practice.id, ch.id, identifier.clone(), user.id, None, None)
        .await
        .expect("relaunch after destroy must succeed");

    assert_ne!(second.id, first_id, "重新启动应创建新实例行");
    let second_state = event_instances::Entity::find_by_id(second.id)
        .one(&db)
        .await
        .expect("row")
        .expect("exists");
    assert_eq!(second_state.runtime_state, "running", "新实例应 running");
    assert_eq!(second_state.container_name, identifier);

    // 同容器名最多一行（旧 completed 行已被清掉）。
    let cnt = event_instances::Entity::find()
        .filter(event_instances::Column::ContainerName.eq(identifier.as_str()))
        .count(&db)
        .await
        .expect("count");
    assert_eq!(cnt, 1, "同一容器名只允许一行");

    // 清理。
    let _ = challenges::Entity::delete_by_id(ch.id).exec(&db).await;
    let _ = users::Entity::delete_by_id(user.id).exec(&db).await;
}

/// 归一化后用户侧练习实例列表应为合并视图：Jeopardy 练习 + AWDP 练习（同一根表 event_instances）。
/// 覆盖：两类实例都出现、AWDP 行带 run_id/gamebox_id/gamebox_title 且 flag 为空、
/// 其他用户看不到、竞赛实例（owner_user_id 为空）不出现。
#[tokio::test]
async fn collect_practice_instances_merges_challenge_and_awdp() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    use floatctf::entity::{awdp_instances, awdp_runs, gameboxes};
    use floatctf::modules::event::jeopardy::api::instances::collect_practice_instances;

    let practice = ensure_practice_jeopardy_event(&db)
        .await
        .expect("practice event");
    let user = seed_user(&db, "pinst").await;
    let other = seed_user(&db, "pinst-other").await;
    let mut ch = seed_challenge(&db, "pinst").await;
    challenges::ActiveModel {
        id: Set(ch.id),
        flag_type: Set(Some("static".into())),
        static_flag_value: Set(Some(format!("flag{{pinst-{}}}", ch.id.simple()))),
        build_status: Set(Some("ready".into())),
        ..Default::default()
    }
    .update(&db)
    .await
    .expect("static challenge");

    // --- Jeopardy 练习实例（挑战行） ---
    let inst_a = Uuid::new_v4();
    event_instances::ActiveModel {
        id: Set(inst_a),
        event_id: Set(practice.id),
        owner_user_id: Set(Some(user.id)),
        owner_team_id: Set(None),
        container_name: Set(format!("tst-chal-{}", &inst_a.to_string()[..8])),
        runtime_state: Set("running".into()),
        runtime_generation: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("challenge runtime");
    event_challenge_instance::ActiveModel {
        id: Set(inst_a),
        event_id: Set(practice.id),
        user_id: Set(user.id),
        team_id: Set(None),
        challenge_id: Set(ch.id),
        flag: Set(format!("flag{{chal-{}}}", inst_a.simple())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("challenge instance");

    // --- AWDP 练习实例（gamebox 行，挂虚拟 awdp 练习 event） ---
    let awdp_event_id = Uuid::new_v4();
    events::ActiveModel {
        id: Set(awdp_event_id),
        title: Set(format!(
            "awdp-practice-test-{}",
            &awdp_event_id.to_string()[..8]
        )),
        system_key: Set(Some(format!(
            "awdp-practice-test:{}",
            &awdp_event_id.to_string()[..8]
        ))),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Practice),
        participant_mode: Set(ParticipantMode::Individual),
        is_virtual: Set(true),
        hidden: Set(true),
        start_time: Set(Utc::now().into()),
        end_time: Set(None),
        rules: Set(String::new()),
        allow_join: Set(true),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("awdp practice event");
    let gb_id = Uuid::new_v4();
    gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(format!("gb-pinst-{}", &gb_id.to_string()[..8])),
        safe_name: Set(format!("gb-pinst-{}", &gb_id.to_string()[..8])),
        category: Set("pwn".into()),
        description: Set("test".into()),
        hidden: Set(false),
        recommended_cpu_millis: Set(1000),
        recommended_memory_bytes: Set(0),
        recommended_pids_limit: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("gamebox");
    let run_id = Uuid::new_v4();
    awdp_runs::ActiveModel {
        id: Set(run_id),
        event_id: Set(awdp_event_id),
        gamebox_id: Set(Some(gb_id)),
        owner_user_id: Set(Some(user.id)),
        owner_team_id: Set(None),
        phase: Set(floatctf::entity::sea_orm_active_enums::AwdpPhase::Break),
        break_duration_secs: Set(3600),
        fix_duration_secs: Set(3600),
        fix_round_interval_secs: Set(600),
        break_score: Set(1000),
        fix_round_score: Set(150),
        current_round: Set(0),
        total_rounds: Set(6),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("practice run");
    let inst_b = Uuid::new_v4();
    event_instances::ActiveModel {
        id: Set(inst_b),
        event_id: Set(awdp_event_id),
        owner_user_id: Set(Some(user.id)),
        owner_team_id: Set(None),
        container_name: Set(format!("tst-gb-{}", &inst_b.to_string()[..8])),
        runtime_state: Set("running".into()),
        runtime_generation: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("gamebox runtime");
    awdp_instances::ActiveModel {
        instance_id: Set(inst_b),
        event_id: Set(awdp_event_id),
        owner_user_id: Set(Some(user.id)),
        owner_team_id: Set(None),
        run_id: Set(run_id),
        gamebox_id: Set(gb_id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("awdp instance");

    // --- 其他用户视角：空 ---
    let other_view = collect_practice_instances(&db, other.id)
        .await
        .expect("other view");
    assert!(other_view.is_empty(), "其他用户应看不到当前用户实例");

    // --- 当前用户视角：两类都出现 ---
    let dtos = collect_practice_instances(&db, user.id)
        .await
        .expect("collect");
    assert_eq!(dtos.len(), 2, "应合并 Jeopardy 练习 + AWDP 练习各一条");

    let chal = dtos
        .iter()
        .find(|d| d.challenge_id.is_some())
        .expect("chal row");
    assert_eq!(chal.id, inst_a);
    assert_eq!(
        chal.identifier,
        format!("tst-chal-{}", &inst_a.to_string()[..8])
    );
    assert_eq!(
        chal.challenge_title.as_deref(),
        Some(ch.name.as_str()),
        "挑战名应补齐"
    );
    assert_eq!(chal.status, "running");

    let gb = dtos
        .iter()
        .find(|d| d.run_id.is_some())
        .expect("gamebox row");
    assert_eq!(gb.id, inst_b);
    assert_eq!(gb.challenge_id, None, "AWDP 实例无 challenge_id");
    assert_eq!(gb.run_id, Some(run_id));
    assert_eq!(gb.gamebox_id, Some(gb_id));
    assert_eq!(
        gb.gamebox_title.as_deref(),
        Some(format!("gb-pinst-{}", &gb_id.to_string()[..8]).as_str())
    );
    assert_eq!(gb.flag, "", "AWDP 实例不返回 flag");
    assert!(gb.content.is_none());

    // --- 清理 ---
    let _ = event_instances::Entity::delete_many()
        .filter(event_instances::Column::Id.is_in(vec![inst_a, inst_b]))
        .exec(&db)
        .await;
    let _ = awdp_runs::Entity::delete_by_id(run_id).exec(&db).await;
    let _ = gameboxes::Entity::delete_by_id(gb_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(ch.id).exec(&db).await;
    let _ = events::Entity::delete_by_id(awdp_event_id).exec(&db).await;
    let _ = users::Entity::delete_by_id(user.id).exec(&db).await;
    let _ = users::Entity::delete_by_id(other.id).exec(&db).await;
}

/// 批量删除练习实例：挑战行走 destroy（停容器 + completed），AWDP 行停容器 + 删根行
/// （CASCADE 清 awdp_instances），run 保留；缺失行幂等跳过；非本人行报 Forbidden。
#[tokio::test]
async fn bulk_delete_practice_instances_mixed_families() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let docker = match bollard::Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("skip bulk_delete: docker client ({e})");
            return;
        }
    };

    use floatctf::entity::{awdp_instances, awdp_runs, gameboxes};
    use floatctf::modules::event::jeopardy::api::instances::bulk_delete_practice_instances;

    let practice = ensure_practice_jeopardy_event(&db)
        .await
        .expect("practice event");
    let user = seed_user(&db, "bdel").await;
    let other = seed_user(&db, "bdel-other").await;
    let mut ch = seed_challenge(&db, "bdel").await;
    challenges::ActiveModel {
        id: Set(ch.id),
        flag_type: Set(Some("static".into())),
        static_flag_value: Set(Some(format!("flag{{bdel-{}}}", ch.id.simple()))),
        build_status: Set(Some("ready".into())),
        ..Default::default()
    }
    .update(&db)
    .await
    .expect("static challenge");

    // --- 挑战练习实例（非 Docker 题） ---
    let inst_a = Uuid::new_v4();
    event_instances::ActiveModel {
        id: Set(inst_a),
        event_id: Set(practice.id),
        owner_user_id: Set(Some(user.id)),
        owner_team_id: Set(None),
        container_name: Set(format!("tst-bdel-chal-{}", &inst_a.to_string()[..8])),
        runtime_state: Set("running".into()),
        runtime_generation: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("challenge runtime");
    event_challenge_instance::ActiveModel {
        id: Set(inst_a),
        event_id: Set(practice.id),
        user_id: Set(user.id),
        team_id: Set(None),
        challenge_id: Set(ch.id),
        flag: Set(format!("flag{{chal-{}}}", inst_a.simple())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("challenge instance");

    // --- AWDP 练习实例（state=stopped 避免 docker 调用） ---
    let awdp_event_id = Uuid::new_v4();
    events::ActiveModel {
        id: Set(awdp_event_id),
        title: Set(format!(
            "awdp-bdel-test-{}",
            &awdp_event_id.to_string()[..8]
        )),
        system_key: Set(Some(format!(
            "awdp-bdel-test:{}",
            &awdp_event_id.to_string()[..8]
        ))),
        family: Set(EventFamily::Awdp),
        purpose: Set(EventPurpose::Practice),
        participant_mode: Set(ParticipantMode::Individual),
        is_virtual: Set(true),
        hidden: Set(true),
        start_time: Set(Utc::now().into()),
        end_time: Set(None),
        rules: Set(String::new()),
        allow_join: Set(true),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("awdp practice event");
    let gb_id = Uuid::new_v4();
    gameboxes::ActiveModel {
        id: Set(gb_id),
        name: Set(format!("gb-bdel-{}", &gb_id.to_string()[..8])),
        safe_name: Set(format!("gb-bdel-{}", &gb_id.to_string()[..8])),
        category: Set("pwn".into()),
        description: Set("test".into()),
        hidden: Set(false),
        recommended_cpu_millis: Set(1000),
        recommended_memory_bytes: Set(0),
        recommended_pids_limit: Set(0),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("gamebox");
    let run_id = Uuid::new_v4();
    awdp_runs::ActiveModel {
        id: Set(run_id),
        event_id: Set(awdp_event_id),
        gamebox_id: Set(Some(gb_id)),
        owner_user_id: Set(Some(user.id)),
        owner_team_id: Set(None),
        phase: Set(floatctf::entity::sea_orm_active_enums::AwdpPhase::Break),
        break_duration_secs: Set(3600),
        fix_duration_secs: Set(3600),
        fix_round_interval_secs: Set(600),
        break_score: Set(1000),
        fix_round_score: Set(150),
        current_round: Set(0),
        total_rounds: Set(6),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("practice run");
    let inst_b = Uuid::new_v4();
    event_instances::ActiveModel {
        id: Set(inst_b),
        event_id: Set(awdp_event_id),
        owner_user_id: Set(Some(user.id)),
        owner_team_id: Set(None),
        container_name: Set(format!("tst-bdel-gb-{}", &inst_b.to_string()[..8])),
        runtime_state: Set("stopped".into()),
        runtime_generation: Set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("gamebox runtime");
    awdp_instances::ActiveModel {
        instance_id: Set(inst_b),
        event_id: Set(awdp_event_id),
        owner_user_id: Set(Some(user.id)),
        owner_team_id: Set(None),
        run_id: Set(run_id),
        gamebox_id: Set(gb_id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("awdp instance");

    // --- 非本人：Forbidden ---
    let forbidden = bulk_delete_practice_instances(&db, &docker, other.id, vec![inst_a]).await;
    assert!(
        matches!(forbidden, Err(floatctf::api::AppError::Forbidden(_))),
        "其他用户批量删除应 Forbidden"
    );

    // --- 缺失 id：幂等跳过，不影响总数 ---
    let none = bulk_delete_practice_instances(&db, &docker, user.id, vec![Uuid::new_v4()]).await;
    assert_eq!(none.expect("skip missing"), 0);

    // --- 本人批量删除两类实例 ---
    let deleted = bulk_delete_practice_instances(&db, &docker, user.id, vec![inst_a, inst_b])
        .await
        .expect("bulk delete");
    assert_eq!(deleted, 2, "应删除/销毁两条");

    let chal_row = event_instances::Entity::find_by_id(inst_a)
        .one(&db)
        .await
        .expect("chal row")
        .expect("挑战行保留（completed）");
    assert_eq!(
        chal_row.runtime_state, "completed",
        "挑战行 destroy 后为 completed"
    );
    assert!(
        event_instances::Entity::find_by_id(inst_b)
            .one(&db)
            .await
            .expect("gb row")
            .is_none(),
        "AWDP 根行应被删除"
    );
    let awdp_cnt = awdp_instances::Entity::find()
        .filter(awdp_instances::Column::InstanceId.eq(inst_b))
        .count(&db)
        .await
        .expect("awdp count");
    assert_eq!(awdp_cnt, 0, "awdp_instances 应随根行 CASCADE 删除");
    assert!(
        awdp_runs::Entity::find_by_id(run_id)
            .one(&db)
            .await
            .expect("run")
            .is_some(),
        "run 应保留（可重新 Start）"
    );

    // --- 清理 ---
    let _ = event_instances::Entity::delete_by_id(inst_a)
        .exec(&db)
        .await;
    let _ = awdp_runs::Entity::delete_by_id(run_id).exec(&db).await;
    let _ = gameboxes::Entity::delete_by_id(gb_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(ch.id).exec(&db).await;
    let _ = events::Entity::delete_by_id(awdp_event_id).exec(&db).await;
    let _ = users::Entity::delete_by_id(user.id).exec(&db).await;
    let _ = users::Entity::delete_by_id(other.id).exec(&db).await;
}
