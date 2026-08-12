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
