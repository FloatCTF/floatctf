//! Live Jeopardy lifecycle coverage for the three supported operating modes.
//!
//! These tests intentionally use a real PostgreSQL database and a real Docker-compatible
//! socket. They are ignored by default because the caller must provide an isolated fixture:
//!
//! - `DATABASE_URL`
//! - `FLOATCTF_TEST_DOCKER_SOCKET`
//! - `FLOATCTF_TEST_CHALLENGE_ID` (ready, dynamic, Docker-backed challenge)
//! - `FLOATCTF_TEST_USER_A`, `FLOATCTF_TEST_USER_B`

use actix_web::web;
use bollard::{API_DEFAULT_VERSION, Docker};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
};
use uuid::Uuid;

use floatctf::{
    entity::{
        event_challenge_instance, event_instances, event_teams, event_users, events,
        jeopardy_challenge_solves, jeopardy_event_challenges,
        sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
        users,
    },
    modules::event::{
        common::application::{
            admin_service::{self, CreateEventRequest, PatchEventRequest},
            player_service,
        },
        jeopardy::application::{
            EventContext, EventContextBuilder, SubmitFlagRequest, instance, scoreboard, submit,
        },
    },
};

fn env_uuid(name: &str) -> Uuid {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} is required"))
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be a UUID"))
}

async fn fixture() -> (
    sea_orm::DatabaseConnection,
    Docker,
    Uuid,
    users::Model,
    users::Model,
) {
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required");
    let socket = std::env::var("FLOATCTF_TEST_DOCKER_SOCKET")
        .expect("FLOATCTF_TEST_DOCKER_SOCKET is required");
    let challenge_id = env_uuid("FLOATCTF_TEST_CHALLENGE_ID");
    let user_a_id = env_uuid("FLOATCTF_TEST_USER_A");
    let user_b_id = env_uuid("FLOATCTF_TEST_USER_B");

    let db = sea_orm::Database::connect(db_url)
        .await
        .expect("connect DB");
    let docker = Docker::connect_with_unix(&socket, 120, API_DEFAULT_VERSION)
        .expect("connect helper Docker socket");
    docker.ping().await.expect("helper Docker ping");

    let user_a = users::Entity::find_by_id(user_a_id)
        .one(&db)
        .await
        .expect("query user A")
        .expect("user A exists");
    let user_b = users::Entity::find_by_id(user_b_id)
        .one(&db)
        .await
        .expect("query user B")
        .expect("user B exists");

    (db, docker, challenge_id, user_a, user_b)
}

async fn create_future_event(
    db: &sea_orm::DatabaseConnection,
    participant_mode: ParticipantMode,
    title: &str,
) -> events::Model {
    let now = Utc::now();
    admin_service::create_event(
        db,
        CreateEventRequest {
            family: EventFamily::Jeopardy,
            participant_mode,
            purpose: None,
            title: title.to_owned(),
            description: Some("live three-mode lifecycle fixture".into()),
            hidden: false,
            allow_join: true,
            rules: "live e2e".into(),
            flag_prefix: Some("flag".into()),
            start_time: (now + Duration::minutes(10)).fixed_offset(),
            end_time: (now + Duration::hours(2)).fixed_offset(),
        },
    )
    .await
    .expect("create competition event")
}

async fn mount_challenge(db: &sea_orm::DatabaseConnection, event_id: Uuid, challenge_id: Uuid) {
    jeopardy_event_challenges::ActiveModel {
        event_id: Set(event_id),
        challenge_id: Set(challenge_id),
        points: Set(100.0),
        hidden: Set(false),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("mount challenge");
}

async fn make_ongoing(db: &sea_orm::DatabaseConnection, event_id: Uuid) -> events::Model {
    let now = Utc::now();
    admin_service::patch_event(
        db,
        event_id,
        PatchEventRequest {
            title: None,
            description: None,
            hidden: None,
            allow_join: None,
            rules: None,
            flag_prefix: None,
            start_time: Some((now - Duration::minutes(1)).fixed_offset()),
            end_time: Some((now + Duration::hours(1)).fixed_offset()),
        },
    )
    .await
    .expect("make event ongoing")
}

async fn context(
    db: &sea_orm::DatabaseConnection,
    docker: &Docker,
    event: events::Model,
    user: users::Model,
) -> EventContext {
    EventContextBuilder::new()
        .db(web::Data::new(db.clone()))
        .docker(web::Data::new(docker.clone()))
        .event(event)
        .user(user)
        .build()
        .await
        .expect("build event context")
}

async fn stored_flag(db: &sea_orm::DatabaseConnection, instance_id: Uuid) -> String {
    event_challenge_instance::Entity::find_by_id(instance_id)
        .one(db)
        .await
        .expect("query instance")
        .expect("instance exists")
        .flag
}

async fn runtime(db: &sea_orm::DatabaseConnection, instance_id: Uuid) -> event_instances::Model {
    event_instances::Entity::find_by_id(instance_id)
        .one(db)
        .await
        .expect("query runtime")
        .expect("runtime exists")
}

async fn remove_event(db: &sea_orm::DatabaseConnection, event_id: Uuid) {
    let _ = events::Entity::delete_by_id(event_id).exec(db).await;
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL + floatctf-helper Docker fixture"]
async fn practice_full_lifecycle_and_retraining() {
    let (db, docker, challenge_id, _, user_b) = fixture().await;
    let practice = events::Entity::find()
        .filter(events::Column::SystemKey.eq("practice:jeopardy"))
        .one(&db)
        .await
        .expect("query practice event")
        .expect("system Jeopardy practice event exists");
    assert_eq!(practice.family, EventFamily::Jeopardy);
    assert_eq!(practice.purpose, EventPurpose::Practice);
    assert_eq!(practice.participant_mode, ParticipantMode::Individual);

    let ctx = context(&db, &docker, practice.clone(), user_b.clone()).await;
    let first = instance::launch_instance(&ctx, challenge_id)
        .await
        .expect("launch practice instance");
    assert!(
        runtime(&db, first.id)
            .await
            .container_name
            .starts_with("JP-")
    );
    assert!(first.flag.is_empty(), "flag must not be returned to player");

    let first_flag = stored_flag(&db, first.id).await;
    assert!(
        submit::submit_flag(
            &ctx,
            SubmitFlagRequest {
                instance_id: Some(first.id),
                flag: "definitely-wrong".into(),
            },
        )
        .await
        .is_err(),
        "wrong practice flag must be rejected"
    );
    submit::submit_flag(
        &ctx,
        SubmitFlagRequest {
            instance_id: Some(first.id),
            flag: first_flag.clone(),
        },
    )
    .await
    .expect("correct practice flag");
    assert_eq!(runtime(&db, first.id).await.runtime_state, "completed");

    let practice_solves = || {
        jeopardy_challenge_solves::Entity::find()
            .filter(jeopardy_challenge_solves::Column::EventId.eq(practice.id))
            .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(challenge_id))
            .filter(jeopardy_challenge_solves::Column::UserId.eq(user_b.id))
            .filter(jeopardy_challenge_solves::Column::TeamId.is_null())
    };
    let solves = practice_solves()
        .all(&db)
        .await
        .expect("query first practice solve");
    assert_eq!(solves.len(), 1);
    assert!((solves[0].obtained_points - 0.0).abs() < f64::EPSILON);

    let second = instance::launch_instance(&ctx, challenge_id)
        .await
        .expect("relaunch practice after solve");
    assert_ne!(
        second.id, first.id,
        "retraining must create a fresh instance row"
    );
    let second_flag = stored_flag(&db, second.id).await;
    assert_ne!(
        second_flag, first_flag,
        "retraining must rotate the dynamic flag"
    );
    submit::submit_flag(
        &ctx,
        SubmitFlagRequest {
            instance_id: Some(second.id),
            flag: second_flag,
        },
    )
    .await
    .expect("solve retraining instance");
    assert_eq!(runtime(&db, second.id).await.runtime_state, "completed");
    assert_eq!(
        practice_solves()
            .count(&db)
            .await
            .expect("count practice solves"),
        1,
        "retraining must remain a single zero-point solved marker"
    );

    let third = instance::launch_instance(&ctx, challenge_id)
        .await
        .expect("third practice launch for manual cleanup");
    instance::destroy_instance(&ctx, third.id)
        .await
        .expect("manual practice destroy");
    assert_eq!(runtime(&db, third.id).await.runtime_state, "completed");
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL + floatctf-helper Docker fixture"]
async fn individual_competition_full_lifecycle() {
    let (db, docker, challenge_id, user_a, _) = fixture().await;
    let event = create_future_event(&db, ParticipantMode::Individual, "live-individual").await;
    let event_id = event.id;
    mount_challenge(&db, event_id, challenge_id).await;

    player_service::join_event(&web::Data::new(db.clone()), event_id, user_a.id)
        .await
        .expect("join before event start");
    let event = make_ongoing(&db, event_id).await;
    let ctx = context(&db, &docker, event.clone(), user_a.clone()).await;

    let launched = instance::launch_instance(&ctx, challenge_id)
        .await
        .expect("launch individual instance");
    assert!(
        runtime(&db, launched.id)
            .await
            .container_name
            .starts_with("JS-")
    );
    assert!(
        launched.flag.is_empty(),
        "flag must not be returned to player"
    );
    assert_eq!(launched.team_id, None);

    let reused = instance::launch_instance(&ctx, challenge_id)
        .await
        .expect("reuse individual instance");
    assert_eq!(reused.id, launched.id, "running launch must be idempotent");

    let flag = stored_flag(&db, launched.id).await;
    assert!(
        submit::submit_flag(
            &ctx,
            SubmitFlagRequest {
                instance_id: Some(launched.id),
                flag: "definitely-wrong".into(),
            },
        )
        .await
        .is_err(),
        "wrong flag must be rejected"
    );

    submit::submit_flag(
        &ctx,
        SubmitFlagRequest {
            instance_id: Some(launched.id),
            flag,
        },
    )
    .await
    .expect("correct individual flag");
    assert_eq!(runtime(&db, launched.id).await.runtime_state, "completed");

    let participant = event_users::Entity::find_by_id((event_id, user_a.id))
        .one(&db)
        .await
        .expect("query participant")
        .expect("participant exists");
    assert!((participant.points - 100.0).abs() < f64::EPSILON);

    let solves = jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
        .all(&db)
        .await
        .expect("query solves");
    assert_eq!(solves.len(), 1);
    assert!((solves[0].obtained_points - 100.0).abs() < f64::EPSILON);
    assert!((solves[0].bonus_points - 0.0).abs() < f64::EPSILON);

    let board = scoreboard::get_scoreboard(&web::Data::new(db.clone()), &event)
        .await
        .expect("individual scoreboard");
    assert_eq!(board.len(), 1);
    assert!((board[0].score - 100.0).abs() < f64::EPSILON);
    assert_eq!(board[0].solved_count, 1);

    remove_event(&db, event_id).await;
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL + floatctf-helper Docker fixture"]
async fn team_competition_shared_instance_full_lifecycle() {
    let (db, docker, challenge_id, user_a, user_b) = fixture().await;
    let event = create_future_event(&db, ParticipantMode::Team, "live-team").await;
    let event_id = event.id;
    mount_challenge(&db, event_id, challenge_id).await;

    let web_db = web::Data::new(db.clone());
    let team = player_service::create_team(&web_db, event_id, user_a.id, "live-team-a".into())
        .await
        .expect("captain creates team before start");
    player_service::join_team(&db, event_id, team.id, user_b.id)
        .await
        .expect("second member joins team before start");

    let event = make_ongoing(&db, event_id).await;
    let ctx_a = context(&db, &docker, event.clone(), user_a.clone()).await;
    let ctx_b = context(&db, &docker, event.clone(), user_b.clone()).await;

    let launched = instance::launch_instance(&ctx_a, challenge_id)
        .await
        .expect("captain launches team instance");
    assert!(
        runtime(&db, launched.id)
            .await
            .container_name
            .starts_with("JT-")
    );
    assert_eq!(launched.team_id, Some(team.id));
    assert!(
        launched.flag.is_empty(),
        "flag must not be returned to player"
    );

    let visible_to_b = instance::get_instance_by_challenge_id(&ctx_b, challenge_id)
        .await
        .expect("team member sees shared instance");
    assert_eq!(visible_to_b.0.id, launched.id);

    let reused_by_b = instance::launch_instance(&ctx_b, challenge_id)
        .await
        .expect("team member reuses shared instance");
    assert_eq!(reused_by_b.id, launched.id, "team launch must be shared");

    // A non-launching teammate must also be allowed to explicitly destroy the shared runtime.
    instance::destroy_instance(&ctx_b, launched.id)
        .await
        .expect("teammate destroys shared instance");
    assert_eq!(runtime(&db, launched.id).await.runtime_state, "completed");

    let relaunched = instance::launch_instance(&ctx_a, challenge_id)
        .await
        .expect("relaunch shared team instance after teammate cleanup");
    assert_ne!(relaunched.id, launched.id);
    let seen_again_by_b = instance::get_instance_by_challenge_id(&ctx_b, challenge_id)
        .await
        .expect("teammate sees relaunched shared instance");
    assert_eq!(seen_again_by_b.0.id, relaunched.id);

    let flag = stored_flag(&db, relaunched.id).await;
    submit::submit_flag(
        &ctx_b,
        SubmitFlagRequest {
            instance_id: Some(relaunched.id),
            flag,
        },
    )
    .await
    .expect("second team member submits shared flag");

    assert_eq!(
        runtime(&db, relaunched.id).await.runtime_state,
        "completed",
        "a solve by any team member must destroy the shared runtime"
    );

    let team_after = event_teams::Entity::find_by_id(team.id)
        .one(&db)
        .await
        .expect("query team")
        .expect("team exists");
    assert!((team_after.points - 100.0).abs() < f64::EPSILON);

    let solves = jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
        .filter(jeopardy_challenge_solves::Column::TeamId.eq(team.id))
        .all(&db)
        .await
        .expect("query team solves");
    assert_eq!(solves.len(), 1);
    assert!((solves[0].obtained_points - 100.0).abs() < f64::EPSILON);
    assert!((solves[0].bonus_points - 0.0).abs() < f64::EPSILON);

    let board = scoreboard::get_scoreboard(&web::Data::new(db.clone()), &event)
        .await
        .expect("team scoreboard");
    assert_eq!(board.len(), 1);
    assert!((board[0].score - 100.0).abs() < f64::EPSILON);
    assert_eq!(board[0].solved_count, 1);

    remove_event(&db, event_id).await;
}
