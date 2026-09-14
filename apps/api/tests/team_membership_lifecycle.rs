//! Team-enrollment lifecycle regression tests.
//!
//! These are DB-gated because the invariants span `events`, `event_users`,
//! `event_teams`, and `event_team_members`. The tests intentionally exercise
//! the common player service used by Jeopardy/AWD/AWDP team competitions.

use actix_web::web;
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter,
};
use uuid::Uuid;

use floatctf::{
    entity::{
        event_team_members, event_teams, event_users, events,
        sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
        users,
    },
    modules::event::common::application::player_service,
};

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(error) => {
            eprintln!("skip team_membership_lifecycle: DB unreachable ({error})");
            None
        }
    }
}

async fn seed_user(db: &DatabaseConnection, tag: &str) -> users::Model {
    let id = Uuid::new_v4();
    users::ActiveModel {
        id: Set(id),
        username: Set(format!("team-life-{tag}-{}", id.simple())),
        nickname: Set(format!("team-life-{tag}-{}", id.simple())),
        password: Set("x".into()),
        email: Set(format!("team-life-{tag}-{}@example.test", id.simple())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed user")
}

async fn seed_team_event(
    db: &DatabaseConnection,
    tag: &str,
    allow_join: bool,
    ongoing: bool,
    hidden: bool,
) -> events::Model {
    let now = Utc::now();
    events::ActiveModel {
        id: Set(Uuid::new_v4()),
        is_virtual: Set(false),
        family: Set(EventFamily::Jeopardy),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        title: Set(format!("team-life-{tag}-{}", Uuid::new_v4().simple())),
        hidden: Set(hidden),
        allow_join: Set(allow_join),
        start_time: Set(if ongoing {
            (now - Duration::minutes(5)).into()
        } else {
            (now + Duration::hours(1)).into()
        }),
        end_time: Set(Some((now + Duration::hours(2)).fixed_offset())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed team event")
}

async fn membership_count(db: &DatabaseConnection, event_id: Uuid, user_id: Uuid) -> u64 {
    event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .count(db)
        .await
        .expect("count memberships")
}

async fn enrolled(db: &DatabaseConnection, event_id: Uuid, user_id: Uuid) -> bool {
    event_users::Entity::find_by_id((event_id, user_id))
        .one(db)
        .await
        .expect("query event user")
        .is_some()
}

#[actix_web::test]
async fn join_team_rejects_started_event_without_side_effects() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let captain = seed_user(&db, "started-cap").await;
    let late_user = seed_user(&db, "started-late").await;
    let future = seed_team_event(&db, "started-seed", true, false, false).await;
    let team = player_service::create_team(
        &web::Data::new(db.clone()),
        future.id,
        captain.id,
        "started-team".into(),
    )
    .await
    .expect("create team before start");

    let mut active: events::ActiveModel = future.into();
    active.start_time = Set((Utc::now() - Duration::minutes(1)).into());
    active.update(&db).await.expect("move event to ongoing");

    let result = player_service::join_team(&db, team.event_id, team.id, late_user.id).await;
    assert!(result.is_err(), "joining a started event must be rejected");
    assert_eq!(membership_count(&db, team.event_id, late_user.id).await, 0);
    assert!(!enrolled(&db, team.event_id, late_user.id).await);
}

#[actix_web::test]
async fn team_enrollment_honors_registration_closed_and_hidden_events() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let user = seed_user(&db, "closed-user").await;
    let closed = seed_team_event(&db, "closed", false, false, false).await;
    let create = player_service::create_team(
        &web::Data::new(db.clone()),
        closed.id,
        user.id,
        "closed-team".into(),
    )
    .await;
    assert!(create.is_err(), "allow_join=false must block team creation");
    assert!(!enrolled(&db, closed.id, user.id).await);

    let captain = seed_user(&db, "hidden-cap").await;
    let joiner = seed_user(&db, "hidden-joiner").await;
    let visible = seed_team_event(&db, "hidden-seed", true, false, false).await;
    let team = player_service::create_team(
        &web::Data::new(db.clone()),
        visible.id,
        captain.id,
        "hidden-team".into(),
    )
    .await
    .expect("create visible team");
    let mut active: events::ActiveModel = visible.into();
    active.hidden = Set(true);
    active.update(&db).await.expect("hide event");

    let result = player_service::join_team(&db, team.event_id, team.id, joiner.id).await;
    assert!(
        result.is_err(),
        "hidden event must not accept team enrollment"
    );
    assert_eq!(membership_count(&db, team.event_id, joiner.id).await, 0);
    assert!(!enrolled(&db, team.event_id, joiner.id).await);
}

#[actix_web::test]
async fn user_cannot_join_two_teams_and_failed_join_is_atomic() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let cap_a = seed_user(&db, "dupe-cap-a").await;
    let cap_b = seed_user(&db, "dupe-cap-b").await;
    let member = seed_user(&db, "dupe-member").await;
    let event = seed_team_event(&db, "dupe", true, false, false).await;
    let web_db = web::Data::new(db.clone());
    let team_a = player_service::create_team(&web_db, event.id, cap_a.id, "dupe-a".into())
        .await
        .expect("create team a");
    let team_b = player_service::create_team(&web_db, event.id, cap_b.id, "dupe-b".into())
        .await
        .expect("create team b");

    player_service::join_team(&db, event.id, team_a.id, member.id)
        .await
        .expect("join first team");
    let second = player_service::join_team(&db, event.id, team_b.id, member.id).await;
    assert!(
        second.is_err(),
        "a user may belong to only one team per event"
    );

    assert_eq!(membership_count(&db, event.id, member.id).await, 1);
    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event.id))
        .filter(event_team_members::Column::UserId.eq(member.id))
        .one(&db)
        .await
        .expect("query membership")
        .expect("membership remains");
    assert_eq!(
        membership.team_id, team_a.id,
        "failed join must not leave a second row"
    );
    assert!(enrolled(&db, event.id, member.id).await);
}

#[actix_web::test]
async fn cross_event_team_id_is_rejected_without_side_effects() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let cap = seed_user(&db, "cross-cap").await;
    let user = seed_user(&db, "cross-user").await;
    let event_a = seed_team_event(&db, "cross-a", true, false, false).await;
    let event_b = seed_team_event(&db, "cross-b", true, false, false).await;
    let team_b = player_service::create_team(
        &web::Data::new(db.clone()),
        event_b.id,
        cap.id,
        "cross-team-b".into(),
    )
    .await
    .expect("create team in event b");

    let result = player_service::join_team(&db, event_a.id, team_b.id, user.id).await;
    assert!(
        result.is_err(),
        "team id from another event must be rejected"
    );
    assert_eq!(membership_count(&db, event_a.id, user.id).await, 0);
    assert!(!enrolled(&db, event_a.id, user.id).await);
}

#[actix_web::test]
async fn captain_quit_removes_whole_team_and_all_team_enrollments_atomically() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let captain = seed_user(&db, "quit-cap").await;
    let member = seed_user(&db, "quit-member").await;
    let event = seed_team_event(&db, "quit", true, false, false).await;
    let team = player_service::create_team(
        &web::Data::new(db.clone()),
        event.id,
        captain.id,
        "quit-team".into(),
    )
    .await
    .expect("create team");
    player_service::join_team(&db, event.id, team.id, member.id)
        .await
        .expect("join member");

    player_service::quit_team(&db, event.id, team.id, captain.id)
        .await
        .expect("captain deletes team");

    assert!(
        event_teams::Entity::find_by_id(team.id)
            .one(&db)
            .await
            .expect("query team")
            .is_none()
    );
    assert_eq!(membership_count(&db, event.id, captain.id).await, 0);
    assert_eq!(membership_count(&db, event.id, member.id).await, 0);
    assert!(!enrolled(&db, event.id, captain.id).await);
    assert!(!enrolled(&db, event.id, member.id).await);
}

#[actix_web::test]
async fn team_membership_cannot_change_after_event_start() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let captain = seed_user(&db, "lock-cap").await;
    let member = seed_user(&db, "lock-member").await;
    let event = seed_team_event(&db, "lock", true, false, false).await;
    let team = player_service::create_team(
        &web::Data::new(db.clone()),
        event.id,
        captain.id,
        "lock-team".into(),
    )
    .await
    .expect("create team");
    player_service::join_team(&db, event.id, team.id, member.id)
        .await
        .expect("join member");

    let mut active: events::ActiveModel = event.into();
    active.start_time = Set((Utc::now() - Duration::minutes(1)).into());
    active.update(&db).await.expect("start event");

    assert!(
        player_service::quit_team(&db, team.event_id, team.id, captain.id)
            .await
            .is_err(),
        "captain must not delete a team after start"
    );
    assert!(
        player_service::leave_team(&db, team.event_id, team.id, member.id)
            .await
            .is_err(),
        "member must not leave a team after start"
    );
    assert!(
        event_teams::Entity::find_by_id(team.id)
            .one(&db)
            .await
            .expect("query team")
            .is_some()
    );
    assert_eq!(membership_count(&db, team.event_id, captain.id).await, 1);
    assert_eq!(membership_count(&db, team.event_id, member.id).await, 1);
    assert!(enrolled(&db, team.event_id, captain.id).await);
    assert!(enrolled(&db, team.event_id, member.id).await);
}

#[actix_web::test]
async fn member_leave_removes_membership_and_enrollment_but_keeps_team() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let captain = seed_user(&db, "member-leave-cap").await;
    let member = seed_user(&db, "member-leave-member").await;
    let event = seed_team_event(&db, "member-leave", true, false, false).await;
    let team = player_service::create_team(
        &web::Data::new(db.clone()),
        event.id,
        captain.id,
        "member-leave-team".into(),
    )
    .await
    .expect("create team");
    player_service::join_team(&db, event.id, team.id, member.id)
        .await
        .expect("join member");

    player_service::leave_team(&db, event.id, team.id, member.id)
        .await
        .expect("member leaves team");

    assert_eq!(membership_count(&db, event.id, member.id).await, 0);
    assert!(!enrolled(&db, event.id, member.id).await);
    assert_eq!(membership_count(&db, event.id, captain.id).await, 1);
    assert!(enrolled(&db, event.id, captain.id).await);
    assert!(
        event_teams::Entity::find_by_id(team.id)
            .one(&db)
            .await
            .expect("query team")
            .is_some()
    );
}

#[actix_web::test]
async fn concurrent_join_to_two_teams_has_exactly_one_winner() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let cap_a = seed_user(&db, "race-cap-a").await;
    let cap_b = seed_user(&db, "race-cap-b").await;
    let member = seed_user(&db, "race-member").await;
    let event = seed_team_event(&db, "race", true, false, false).await;
    let web_db = web::Data::new(db.clone());
    let team_a = player_service::create_team(&web_db, event.id, cap_a.id, "race-a".into())
        .await
        .expect("create race team a");
    let team_b = player_service::create_team(&web_db, event.id, cap_b.id, "race-b".into())
        .await
        .expect("create race team b");

    let db_a = db.clone();
    let db_b = db.clone();
    let (join_a, join_b) = tokio::join!(
        player_service::join_team(&db_a, event.id, team_a.id, member.id),
        player_service::join_team(&db_b, event.id, team_b.id, member.id),
    );

    assert_eq!(
        usize::from(join_a.is_ok()) + usize::from(join_b.is_ok()),
        1,
        "concurrent joins must serialize to exactly one team"
    );
    assert_eq!(membership_count(&db, event.id, member.id).await, 1);
    assert!(enrolled(&db, event.id, member.id).await);
}

#[actix_web::test]
async fn generic_leave_event_rejects_team_mode_without_corrupting_membership() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let captain = seed_user(&db, "leave-cap").await;
    let event = seed_team_event(&db, "leave", true, false, false).await;
    let team = player_service::create_team(
        &web::Data::new(db.clone()),
        event.id,
        captain.id,
        "leave-team".into(),
    )
    .await
    .expect("create team");

    let result =
        player_service::leave_event(&web::Data::new(db.clone()), event.id, captain.id).await;
    assert!(
        result.is_err(),
        "team event must use team leave/quit workflow"
    );
    assert!(enrolled(&db, event.id, captain.id).await);
    assert_eq!(membership_count(&db, event.id, captain.id).await, 1);
    assert!(
        event_teams::Entity::find_by_id(team.id)
            .one(&db)
            .await
            .expect("query team")
            .is_some()
    );
}
