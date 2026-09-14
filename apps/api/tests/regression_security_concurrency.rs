//! 本批未提交修改的回归验证（DB-gated，无 Docker 依赖）。
//!
//! 覆盖：
//! 1. JWT Role 隔离（User/SuperAdmin 守卫；越权 token 全部 401）
//! 2. Jeopardy 原子计分：并发不同题 / 并发同题重复 / 团队成员并发 / 动态分值 advisory lock
//! 3. 实例生命周期：未过期保留 / 过期回收 / failed 重试 / 删除失败 → failed → completed
//! 4. Orphan 容器补偿：launch 成功后 DB 持久化失败必须 stop_and_remove
//! 5. finalize_launch：inspect 失败 / 端口缺失 → 补偿删除
//! 6. 系统任务 seed/repair 幂等（system.practice.clean → cron */
//! 30 * * * * *）

use std::sync::Arc;

use actix_web::{App, HttpResponse, test as actix_test, web};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use floatctf::api::extractor::auth::{SuperAdminJwtGuard, UserJwtGuard};
use floatctf::core::security::jwt::{self, Role};
use floatctf::entity::{
    challenges, event_challenge_instance, event_instances, event_team_members, event_teams,
    event_users, events, jeopardy_challenge_solves, jeopardy_event_challenges, scheduled_tasks,
    sea_orm_active_enums::{EventFamily, EventPurpose, EventTeamMemberRole, ParticipantMode},
    super_admin, users,
};
use floatctf::infrastructure::{LogService, WebDb};
use floatctf::modules::event::jeopardy::application::instance_service::InstanceService;
use floatctf::modules::event::jeopardy::application::submission_service::JeopardySubmissionService;
use floatctf::modules::event::jeopardy::domain::solve::{JeopardySubmitRequest, SolveSubject};
use floatctf::modules::event::jeopardy::infrastructure::container_runtime::{
    ChallengeRuntimeSpec, InstanceRuntime,
};
use floatctf::scheduler::TaskScheduler;

/// 测试文件级串行：所有测试共享 dev 库，避免 advisory lock / settings 写互踩。
static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn connect_or_skip() -> Option<DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip regression_security_concurrency: DB unreachable ({e})");
            None
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// §1 JWT Role 隔离
// ─────────────────────────────────────────────────────────────────

async fn guard_user(_g: UserJwtGuard) -> HttpResponse {
    HttpResponse::Ok().finish()
}
async fn guard_admin(_g: SuperAdminJwtGuard) -> HttpResponse {
    HttpResponse::Ok().finish()
}

fn token_for(sub: Uuid, role: Role) -> String {
    jwt::gen_jwt_token(sub, role, Some(30)).expect("jwt")
}

async fn seed_guard_rows(db: &DatabaseConnection) -> (Uuid, Uuid, Uuid) {
    let tag = Uuid::new_v4().simple().to_string();
    let user_id = Uuid::new_v4();
    let admin_id = Uuid::new_v4();
    // 同 UUID 双实体：users 与 super_admins 各有一行 id=both_id。
    let both_id = Uuid::new_v4();
    users::ActiveModel {
        id: Set(user_id),
        username: Set(format!("ru-{tag}")),
        nickname: Set(format!("rn-{tag}")),
        password: Set("x".into()),
        email: Set(format!("u-{tag}@example.test")),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("user");
    users::ActiveModel {
        id: Set(both_id),
        username: Set(format!("bu-{tag}")),
        nickname: Set(format!("bn-{tag}")),
        password: Set("x".into()),
        email: Set(format!("b-{tag}@example.test")),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("both-user");
    super_admin::ActiveModel {
        id: Set(admin_id),
        username: Set(format!("sa-{tag}")),
        password: Set("x".into()),
        email: Set(format!("a-{tag}@example.test")),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("admin");
    super_admin::ActiveModel {
        id: Set(both_id),
        username: Set(format!("bsa-{tag}")),
        password: Set("x".into()),
        email: Set(format!("ba-{tag}@example.test")),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("both-admin");
    (user_id, admin_id, both_id)
}

#[tokio::test]
async fn jwt_role_isolation_matrix() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    jwt::configure_jwt_secret(floatctf::core::secret::Secret::new("regression-jwt-secret"));
    let (user_id, admin_id, both_id) = seed_guard_rows(&db).await;

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(db.clone()))
            .route("/user", web::post().to(guard_user))
            .route("/admin", web::post().to(guard_admin)),
    )
    .await;

    // (name, token, route, expect_ok)
    let cases: Vec<(&str, String, &str, bool)> = vec![
        (
            "user token → user route",
            token_for(user_id, Role::User),
            "/user",
            true,
        ),
        (
            "admin token → admin route",
            token_for(admin_id, Role::SuperAdmin),
            "/admin",
            true,
        ),
        (
            "admin token → user route (must 401)",
            token_for(admin_id, Role::SuperAdmin),
            "/user",
            false,
        ),
        (
            "user token → admin route (must 401)",
            token_for(user_id, Role::User),
            "/admin",
            false,
        ),
        (
            "reset token → user route (must 401)",
            token_for(user_id, Role::ResetAccount),
            "/user",
            false,
        ),
        (
            "reset token → admin route (must 401)",
            token_for(user_id, Role::ResetAccount),
            "/admin",
            false,
        ),
        (
            "awd judger token → user route (must 401)",
            token_for(user_id, Role::AwdJudger),
            "/user",
            false,
        ),
        (
            "awd judger token → admin route (must 401)",
            token_for(user_id, Role::AwdJudger),
            "/admin",
            false,
        ),
        (
            "user-role token with both-entity uuid → admin route (must 401)",
            token_for(both_id, Role::User),
            "/admin",
            false,
        ),
        (
            "admin-role token with both-entity uuid → user route (must 401)",
            token_for(both_id, Role::SuperAdmin),
            "/user",
            false,
        ),
        (
            "user-role token with both-entity uuid → user route (users row exists)",
            token_for(both_id, Role::User),
            "/user",
            true,
        ),
        (
            "admin-role token with both-entity uuid → admin route (super_admin row exists)",
            token_for(both_id, Role::SuperAdmin),
            "/admin",
            true,
        ),
        (
            "valid signature but unknown subject → user route",
            token_for(Uuid::new_v4(), Role::User),
            "/user",
            false,
        ),
        (
            "valid signature but unknown subject → admin route",
            token_for(Uuid::new_v4(), Role::SuperAdmin),
            "/admin",
            false,
        ),
    ];

    for (name, token, route, expect_ok) in cases {
        let req = actix_test::TestRequest::post()
            .uri(route)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        let status = resp.status().as_u16();
        if expect_ok {
            assert_eq!(status, 200, "{name}: expected 200, got {status}");
        } else {
            assert_eq!(status, 401, "{name}: expected 401, got {status}");
        }
    }

    // 无 Authorization 头
    for route in ["/user", "/admin"] {
        let req = actix_test::TestRequest::post().uri(route).to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 401, "no-token {route} must 401");
    }

    // 篡改 token（签名无效）
    let mut tampered = token_for(user_id, Role::User);
    tampered.push('x');
    let req = actix_test::TestRequest::post()
        .uri("/user")
        .insert_header(("Authorization", format!("Bearer {tampered}")))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status().as_u16(), 401, "tampered token must 401");

    for id in [user_id, admin_id, both_id] {
        let _ = users::Entity::delete_by_id(id).exec(&db).await;
        let _ = super_admin::Entity::delete_by_id(id).exec(&db).await;
    }
}

// ─────────────────────────────────────────────────────────────────
// §2 Jeopardy 原子计分
// ─────────────────────────────────────────────────────────────────

struct SeedEvent {
    event_id: Uuid,
    challenge_ids: Vec<Uuid>,
    team_ids: Vec<Uuid>,
    user_ids: Vec<Uuid>,
    flag: String,
}

/// seed 1 个 Jeopardy 竞赛事件：`n_challenges` 题（points 100/200/…）+ `n_users` 用户
///（individual 模式）；team 模式由调用方自行插 team。
async fn seed_jeopardy_event(
    db: &DatabaseConnection,
    tag: &str,
    team_mode: bool,
    n_challenges: usize,
    n_users: usize,
) -> SeedEvent {
    let now = Utc::now();
    let event_id = Uuid::new_v4();
    events::ActiveModel {
        is_virtual: Set(false),
        id: Set(event_id),
        family: Set(EventFamily::Jeopardy),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(if team_mode {
            ParticipantMode::Team
        } else {
            ParticipantMode::Individual
        }),
        system_key: Set(None),
        title: Set(format!("reg-{tag}")),
        hidden: Set(false),
        allow_join: Set(true),
        start_time: Set((now - Duration::hours(1)).into()),
        end_time: Set(Some((now + Duration::hours(4)).fixed_offset())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("event");

    let mut challenge_ids = Vec::new();
    for (i, points) in [100.0, 200.0, 300.0, 400.0, 500.0]
        .into_iter()
        .take(n_challenges)
        .enumerate()
    {
        let cid = Uuid::new_v4();
        challenges::ActiveModel {
            id: Set(cid),
            name: Set(format!("ch-{tag}-{i}")),
            safe_name: Set(format!("ch-{tag}-{i}")),
            category: Set("web".into()),
            description: Set("t".into()),
            hidden: Set(true),
            container_port: Set(Some(8080)),
            build_status: Set(Some("ready".into())),
            flag_type: Set(Some("dynamic".into())),
            image_repo_digest: Set(Some("registry.example.test/ch@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("challenge");
        jeopardy_event_challenges::ActiveModel {
            event_id: Set(event_id),
            challenge_id: Set(cid),
            points: Set(points),
            hidden: Set(false),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("jec");
        challenge_ids.push(cid);
    }

    let mut user_ids = Vec::new();
    for i in 0..n_users {
        let uid = Uuid::new_v4();
        users::ActiveModel {
            id: Set(uid),
            username: Set(format!("u{i}-{tag}")),
            nickname: Set(format!("n{i}-{tag}")),
            password: Set("x".into()),
            email: Set(format!("u{i}-{tag}@example.test")),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("user");
        event_users::ActiveModel {
            event_id: Set(event_id),
            user_id: Set(uid),
            points: Set(0.0),
            banned: Set(false),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("join");
        user_ids.push(uid);
    }

    let flag = format!("flag{{reg-{tag}}}");
    SeedEvent {
        event_id,
        challenge_ids,
        team_ids: Vec::new(),
        user_ids,
        flag,
    }
}

/// 为 (user, challenge) 插一对 running 实例行（non-docker：无容器）。
async fn seed_instance(
    db: &DatabaseConnection,
    event_id: Uuid,
    challenge_id: Uuid,
    user_id: Uuid,
    team_id: Option<Uuid>,
    flag: &str,
) -> Uuid {
    let tag = Uuid::new_v4().simple().to_string();
    let now = Utc::now();
    let id = Uuid::new_v4();
    event_instances::ActiveModel {
        id: Set(id),
        event_id: Set(event_id),
        owner_user_id: Set(Some(user_id)),
        owner_team_id: Set(team_id),
        image_ref: Set(None),
        container_id: Set(None),
        container_name: Set(format!("RGT-{tag}")),
        runtime_state: Set("running".to_string()),
        runtime_generation: Set(1),
        created_at: Set(now.into()),
        started_at: Set(Some(now.into())),
        expires_at: Set(Some((now + Duration::hours(1)).into())),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("inst runtime");
    event_challenge_instance::ActiveModel {
        id: Set(id),
        flag: Set(flag.to_string()),
        content: Set(None),
        user_id: Set(user_id),
        challenge_id: Set(challenge_id),
        event_id: Set(event_id),
        team_id: Set(team_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("inst");
    id
}

fn docker_or_panic() -> bollard::Docker {
    bollard::Docker::connect_with_local_defaults().expect("docker client")
}

/// 并发不同 challenge 提交（lost-update 回归）：20 轮并发提交 ch100+ch200，总分必须 300。
#[tokio::test]
async fn jeopardy_concurrent_distinct_challenges_exact_sum() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = Uuid::new_v4().simple().to_string();
    let seed = seed_jeopardy_event(&db, &tag, false, 2, 1).await;
    let user = seed.user_ids[0];
    let docker = docker_or_panic();

    // 20 轮，每轮并发提交两道题
    for _round in 0..20 {
        let event_id = seed.event_id;
        let ch0 = seed.challenge_ids[0];
        let ch1 = seed.challenge_ids[1];
        let flag = seed.flag.clone();
        let i0 = seed_instance(&db, event_id, ch0, user, None, &flag).await;
        let i1 = seed_instance(&db, event_id, ch1, user, None, &flag).await;
        let svc0 = JeopardySubmissionService::new(db.clone(), docker.clone());
        let svc1 = JeopardySubmissionService::new(db.clone(), docker.clone());
        let flag0 = flag.clone();
        let h0 = tokio::spawn(async move {
            svc0.submit(JeopardySubmitRequest {
                event_id,
                user_id: user,
                instance_id: i0,
                flag: flag0,
                subject: SolveSubject::User,
            })
            .await
        });
        let flag1 = flag.clone();
        let h1 = tokio::spawn(async move {
            svc1.submit(JeopardySubmitRequest {
                event_id,
                user_id: user,
                instance_id: i1,
                flag: flag1,
                subject: SolveSubject::User,
            })
            .await
        });
        let (r0, r1) = tokio::join!(h0, h1);
        assert!(r0.unwrap().is_ok(), "round submit ch100");
        assert!(r1.unwrap().is_ok(), "round submit ch200");

        let points = event_users::Entity::find_by_id((seed.event_id, user))
            .one(&db)
            .await
            .expect("q")
            .expect("row")
            .points;
        assert_eq!(points, 300.0, "round {tag}: lost update detected");

        // 清理 solve 行以便下一轮从零开始计分
        jeopardy_challenge_solves::Entity::delete_many()
            .filter(jeopardy_challenge_solves::Column::EventId.eq(seed.event_id))
            .filter(jeopardy_challenge_solves::Column::UserId.eq(user))
            .exec(&db)
            .await
            .expect("clear solves");
        event_users::ActiveModel {
            event_id: Set(seed.event_id),
            user_id: Set(user),
            points: Set(0.0),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("reset points");
    }

    let _ = events::Entity::delete_by_id(seed.event_id).exec(&db).await;
    for c in &seed.challenge_ids {
        let _ = challenges::Entity::delete_by_id(*c).exec(&db).await;
    }
    for u in &seed.user_ids {
        let _ = users::Entity::delete_by_id(*u).exec(&db).await;
    }
}

/// 并发同 challenge 重复提交：50 个并发提交，solve row = 1、积分只加一次。
#[tokio::test]
async fn jeopardy_concurrent_duplicate_submissions_single_solve() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = Uuid::new_v4().simple().to_string();
    let seed = seed_jeopardy_event(&db, &tag, false, 1, 1).await;
    let user = seed.user_ids[0];
    let docker = docker_or_panic();

    // 50 个实例行（同题同 flag），并发提交
    let mut instances = Vec::new();
    for _ in 0..50 {
        instances.push(
            seed_instance(
                &db,
                seed.event_id,
                seed.challenge_ids[0],
                user,
                None,
                &seed.flag,
            )
            .await,
        );
    }

    let mut handles = Vec::new();
    for iid in instances {
        let svc = JeopardySubmissionService::new(db.clone(), docker.clone());
        let flag = seed.flag.clone();
        handles.push(tokio::spawn(async move {
            svc.submit(JeopardySubmitRequest {
                event_id: seed.event_id,
                user_id: user,
                instance_id: iid,
                flag,
                subject: SolveSubject::User,
            })
            .await
        }));
    }
    for h in handles {
        let _ = h.await.expect("join");
    }

    let solves = jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(seed.event_id))
        .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(seed.challenge_ids[0]))
        .filter(jeopardy_challenge_solves::Column::UserId.eq(user))
        .count(&db)
        .await
        .expect("count");
    assert_eq!(solves, 1, "并发重复提交必须恰好 1 条 solve");

    let points = event_users::Entity::find_by_id((seed.event_id, user))
        .one(&db)
        .await
        .expect("q")
        .expect("row")
        .points;
    assert_eq!(points, 100.0, "积分只增加一次（首解 base 分）");

    let _ = events::Entity::delete_by_id(seed.event_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(seed.challenge_ids[0])
        .exec(&db)
        .await;
    let _ = users::Entity::delete_by_id(user).exec(&db).await;
}

/// 团队两个成员并发提交同一题：team solve = 1、积分一次。
#[tokio::test]
async fn jeopardy_team_members_concurrent_single_solve() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = Uuid::new_v4().simple().to_string();
    let seed = seed_jeopardy_event(&db, &tag, true, 1, 2).await;
    let team_id = Uuid::new_v4();
    event_teams::ActiveModel {
        id: Set(team_id),
        event_id: Set(seed.event_id),
        name: Set(format!("team-{tag}")),
        points: Set(0.0),
        banned: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("team");
    for (i, uid) in seed.user_ids.iter().enumerate() {
        event_team_members::ActiveModel {
            event_id: Set(seed.event_id),
            team_id: Set(team_id),
            user_id: Set(*uid),
            role: Set(if i == 0 {
                EventTeamMemberRole::Captain
            } else {
                EventTeamMemberRole::Member
            }),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("member");
    }

    let docker = docker_or_panic();
    let event_id = seed.event_id;
    let challenge_id = seed.challenge_ids[0];
    let flag = seed.flag.clone();
    let mut handles = Vec::new();
    for uid in seed.user_ids.clone() {
        let iid = seed_instance(&db, event_id, challenge_id, uid, Some(team_id), &flag).await;
        let svc = JeopardySubmissionService::new(db.clone(), docker.clone());
        let flag = flag.clone();
        handles.push(tokio::spawn(async move {
            svc.submit(JeopardySubmitRequest {
                event_id,
                user_id: uid,
                instance_id: iid,
                flag,
                subject: SolveSubject::Team,
            })
            .await
        }));
    }
    for h in handles {
        let _ = h.await.expect("join");
    }

    let solves = jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(seed.event_id))
        .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(seed.challenge_ids[0]))
        .filter(jeopardy_challenge_solves::Column::TeamId.eq(team_id))
        .count(&db)
        .await
        .expect("count");
    assert_eq!(solves, 1, "团队并发提交必须恰好 1 条 team solve");

    let points = event_teams::Entity::find_by_id(team_id)
        .one(&db)
        .await
        .expect("q")
        .expect("row")
        .points;
    assert_eq!(points, 100.0, "团队积分只增加一次");

    let _ = events::Entity::delete_by_id(seed.event_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(seed.challenge_ids[0])
        .exec(&db)
        .await;
    for u in &seed.user_ids {
        let _ = users::Entity::delete_by_id(*u).exec(&db).await;
    }
}

/// 动态分值 + advisory lock：20 个用户并发首解同一题。
/// 每条 solve 的 points 必须等于按 solve 顺序计算的动态分（solves=0..19）。
#[tokio::test]
async fn jeopardy_dynamic_scoring_concurrent_deterministic_order() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = Uuid::new_v4().simple().to_string();
    let seed = seed_jeopardy_event(&db, &tag, false, 1, 20).await;
    let docker = docker_or_panic();

    // 固定动态分参数（避免依赖 dev 库 settings 漂移）
    floatctf::infrastructure::settings::upsert_setting(&db, "EVENT_SCORE_DECAY", "500")
        .await
        .expect("set decay");
    floatctf::infrastructure::settings::upsert_setting(&db, "EVENT_SCORE_MIN_PERCENT", "0.45")
        .await
        .expect("set min");

    let mut handles = Vec::new();
    let event_id = seed.event_id;
    let challenge_id = seed.challenge_ids[0];
    let flag = seed.flag.clone();
    for uid in seed.user_ids.clone() {
        let iid = seed_instance(&db, event_id, challenge_id, uid, None, &flag).await;
        let svc = JeopardySubmissionService::new(db.clone(), docker.clone());
        let flag = flag.clone();
        handles.push(tokio::spawn(async move {
            svc.submit(JeopardySubmitRequest {
                event_id,
                user_id: uid,
                instance_id: iid,
                flag,
                subject: SolveSubject::User,
            })
            .await
        }));
    }
    for h in handles {
        let _ = h.await.expect("join");
    }

    let solves = jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(seed.event_id))
        .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(seed.challenge_ids[0]))
        .order_by_asc(jeopardy_challenge_solves::Column::ObtainedPoints)
        .all(&db)
        .await
        .expect("solves");
    assert_eq!(solves.len(), 20, "20 个用户并发解题应产生 20 条 solve");

    // 期望：第 k 个（0-based）解题者得 dynamic_score(100, k, 500, 0.45)
    let mut expected: Vec<f64> = (0..20u64)
        .map(|k| {
            floatctf::modules::event::jeopardy::domain::scoring::dynamic_score(
                100.0, k, 500.0, 0.45,
            )
        })
        .collect();
    expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut actual: Vec<f64> = solves.iter().map(|s| s.obtained_points).collect();
    actual.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert!(
            (a - e).abs() < 1e-6,
            "solve[{}] points {a} != expected {e}（顺序动态分被破坏）",
            i
        );
    }

    // 总积分 = Σ 动态分（advisory lock 下严格按顺序计费，无重复也无丢失）
    let total: f64 = actual.iter().sum();
    let expected_total: f64 = expected.iter().sum();
    let user_points: f64 = event_users::Entity::find()
        .filter(event_users::Column::EventId.eq(seed.event_id))
        .all(&db)
        .await
        .expect("q")
        .iter()
        .map(|eu| eu.points)
        .sum();
    assert!(
        (user_points - expected_total).abs() < 1e-6,
        "用户总积分 {user_points} != 动态分总和 {expected_total}"
    );
    assert!((total - expected_total).abs() < 1e-6);

    let _ = events::Entity::delete_by_id(seed.event_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(seed.challenge_ids[0])
        .exec(&db)
        .await;
    for u in &seed.user_ids {
        let _ = users::Entity::delete_by_id(*u).exec(&db).await;
    }
}

// ─────────────────────────────────────────────────────────────────
// §3 实例生命周期 + orphan 补偿
// ─────────────────────────────────────────────────────────────────

/// 可注入行为的 Fake runtime：记录 launch / stop_and_remove 调用。
struct FakeRuntime {
    launch_port: std::sync::Mutex<Option<u16>>,
    stop_fail: std::sync::atomic::AtomicBool,
    launches: std::sync::atomic::AtomicUsize,
    removals: std::sync::Mutex<Vec<String>>,
}

impl FakeRuntime {
    fn new() -> Self {
        Self {
            launch_port: std::sync::Mutex::new(Some(31337)),
            stop_fail: std::sync::atomic::AtomicBool::new(false),
            launches: std::sync::atomic::AtomicUsize::new(0),
            removals: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl InstanceRuntime for FakeRuntime {
    async fn launch(&self, _spec: &ChallengeRuntimeSpec, _identifier: &str) -> anyhow::Result<u16> {
        self.launches
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match *self.launch_port.lock().unwrap() {
            Some(port) => Ok(port),
            None => Err(anyhow::anyhow!("fake launch failure")),
        }
    }

    async fn stop_and_remove(&self, identifier: &str) -> anyhow::Result<()> {
        self.removals.lock().unwrap().push(identifier.to_string());
        if self.stop_fail.load(std::sync::atomic::Ordering::SeqCst) {
            Err(anyhow::anyhow!("fake docker remove failure"))
        } else {
            Ok(())
        }
    }
}

struct LifecycleSeed {
    event_id: Uuid,
    challenge_id: Uuid,
    user_id: Uuid,
}

async fn seed_lifecycle(db: &DatabaseConnection, tag: &str) -> LifecycleSeed {
    let seed = seed_jeopardy_event(db, tag, false, 1, 1).await;
    LifecycleSeed {
        event_id: seed.event_id,
        challenge_id: seed.challenge_ids[0],
        user_id: seed.user_ids[0],
    }
}

async fn insert_runtime_row(
    db: &DatabaseConnection,
    event_id: Uuid,
    challenge_id: Uuid,
    user_id: Uuid,
    state: &str,
    expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    flag: &str,
) -> Uuid {
    let tag = Uuid::new_v4().simple().to_string();
    let now = Utc::now();
    let id = Uuid::new_v4();
    event_instances::ActiveModel {
        id: Set(id),
        event_id: Set(event_id),
        owner_user_id: Set(Some(user_id)),
        owner_team_id: Set(None),
        image_ref: Set(None),
        container_id: Set(None),
        container_name: Set(format!("RGL-{tag}")),
        runtime_state: Set(state.to_string()),
        runtime_generation: Set(1),
        created_at: Set(now.into()),
        started_at: Set(Some(now.into())),
        expires_at: Set(expires_at),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("inst");
    event_challenge_instance::ActiveModel {
        id: Set(id),
        flag: Set(flag.to_string()),
        content: Set(None),
        user_id: Set(user_id),
        challenge_id: Set(challenge_id),
        event_id: Set(event_id),
        team_id: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("eci");
    id
}

fn state_of(row: Option<event_instances::Model>) -> String {
    row.expect("row").runtime_state
}

/// 未过期 running 实例在 cleanup_running 后必须保留（API 重启语义）。
#[tokio::test]
async fn cleanup_keeps_unexpired_running_instance() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = format!("lc-keep-{}", Uuid::new_v4().simple());
    let s = seed_lifecycle(&db, &tag).await;
    let id = insert_runtime_row(
        &db,
        s.event_id,
        s.challenge_id,
        s.user_id,
        "running",
        Some((Utc::now() + Duration::minutes(30)).into()),
        "flag{x}",
    )
    .await;

    let runtime = Arc::new(FakeRuntime::new());
    let svc = InstanceService::new(db.clone(), runtime.clone() as Arc<dyn InstanceRuntime>);
    let report = svc.cleanup_running().await.expect("cleanup");
    assert!(report.completed.is_empty(), "未过期实例不得被回收");
    assert!(report.failed.is_empty());

    let row = event_instances::Entity::find_by_id(id)
        .one(&db)
        .await
        .unwrap();
    assert_eq!(state_of(row), "running", "未过期 running 必须保留");
    assert_eq!(
        runtime.launches.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert!(runtime.removals.lock().unwrap().is_empty());

    let _ = events::Entity::delete_by_id(s.event_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(s.challenge_id)
        .exec(&db)
        .await;
    let _ = users::Entity::delete_by_id(s.user_id).exec(&db).await;
}

/// 过期 running 实例 → completed。
#[tokio::test]
async fn cleanup_recycles_expired_running_instance() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = format!("lc-exp-{}", Uuid::new_v4().simple());
    let s = seed_lifecycle(&db, &tag).await;
    let id = insert_runtime_row(
        &db,
        s.event_id,
        s.challenge_id,
        s.user_id,
        "running",
        Some((Utc::now() - Duration::minutes(5)).into()),
        "flag{x}",
    )
    .await;

    let runtime = Arc::new(FakeRuntime::new());
    let svc = InstanceService::new(db.clone(), runtime.clone() as Arc<dyn InstanceRuntime>);
    let report = svc.cleanup_running().await.expect("cleanup");
    assert_eq!(report.completed, vec![id]);

    let row = event_instances::Entity::find_by_id(id)
        .one(&db)
        .await
        .unwrap();
    assert_eq!(state_of(row), "completed", "过期实例必须回收为 completed");
    assert_eq!(runtime.removals.lock().unwrap().len(), 1, "容器必须被删除");

    let _ = events::Entity::delete_by_id(s.event_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(s.challenge_id)
        .exec(&db)
        .await;
    let _ = users::Entity::delete_by_id(s.user_id).exec(&db).await;
}

/// failed 实例（runtime 已可删除）→ completed。
#[tokio::test]
async fn cleanup_retries_failed_instance_to_completed() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = format!("lc-fail-{}", Uuid::new_v4().simple());
    let s = seed_lifecycle(&db, &tag).await;
    let id = insert_runtime_row(
        &db,
        s.event_id,
        s.challenge_id,
        s.user_id,
        "failed",
        None,
        "flag{x}",
    )
    .await;

    let runtime = Arc::new(FakeRuntime::new());
    let svc = InstanceService::new(db.clone(), runtime.clone() as Arc<dyn InstanceRuntime>);
    let report = svc.cleanup_running().await.expect("cleanup");
    assert_eq!(report.completed, vec![id]);

    let row = event_instances::Entity::find_by_id(id)
        .one(&db)
        .await
        .unwrap();
    assert_eq!(
        state_of(row),
        "completed",
        "failed 实例重试成功后必须 completed"
    );

    let _ = events::Entity::delete_by_id(s.event_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(s.challenge_id)
        .exec(&db)
        .await;
    let _ = users::Entity::delete_by_id(s.user_id).exec(&db).await;
}

/// 删除失败：过期 running → failed（不得 completed）；恢复后 failed → completed。
#[tokio::test]
async fn cleanup_docker_failure_moves_running_to_failed_then_completed() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = format!("lc-df-{}", Uuid::new_v4().simple());
    let s = seed_lifecycle(&db, &tag).await;
    let id = insert_runtime_row(
        &db,
        s.event_id,
        s.challenge_id,
        s.user_id,
        "running",
        Some((Utc::now() - Duration::minutes(5)).into()),
        "flag{x}",
    )
    .await;

    // 第一轮：docker 删除失败
    let runtime = Arc::new(FakeRuntime::new());
    runtime
        .stop_fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let svc = InstanceService::new(db.clone(), runtime.clone() as Arc<dyn InstanceRuntime>);
    let report = svc.cleanup_running().await.expect("cleanup");
    assert!(report.completed.is_empty(), "删除失败不得进入 completed");
    assert_eq!(report.failed.len(), 1);

    let row = event_instances::Entity::find_by_id(id)
        .one(&db)
        .await
        .unwrap();
    assert_eq!(state_of(row), "failed", "running 删除失败必须转 failed");

    // 第二轮：runtime 恢复正常
    runtime
        .stop_fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let report2 = svc.cleanup_running().await.expect("cleanup2");
    assert_eq!(report2.completed, vec![id]);
    let row2 = event_instances::Entity::find_by_id(id)
        .one(&db)
        .await
        .unwrap();
    assert_eq!(
        state_of(row2),
        "completed",
        "恢复后 failed 必须收敛 completed"
    );

    let _ = events::Entity::delete_by_id(s.event_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(s.challenge_id)
        .exec(&db)
        .await;
    let _ = users::Entity::delete_by_id(s.user_id).exec(&db).await;
}

/// expires_at 为 NULL 的 running 行不是回收候选（SQL NULL 比较语义 → 保留）。
#[tokio::test]
async fn cleanup_keeps_running_instance_with_null_expires_at() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = format!("lc-null-{}", Uuid::new_v4().simple());
    let s = seed_lifecycle(&db, &tag).await;
    let id = insert_runtime_row(
        &db,
        s.event_id,
        s.challenge_id,
        s.user_id,
        "running",
        None,
        "flag{x}",
    )
    .await;

    let runtime = Arc::new(FakeRuntime::new());
    let svc = InstanceService::new(db.clone(), runtime.clone() as Arc<dyn InstanceRuntime>);
    let report = svc.cleanup_running().await.expect("cleanup");
    assert!(
        report.completed.is_empty(),
        "NULL expires_at 的 running 实例不得被回收"
    );
    assert!(report.failed.is_empty());

    let row = event_instances::Entity::find_by_id(id)
        .one(&db)
        .await
        .unwrap();
    assert_eq!(state_of(row), "running", "NULL expires_at 必须保留");
    assert!(runtime.removals.lock().unwrap().is_empty());

    let _ = events::Entity::delete_by_id(s.event_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(s.challenge_id)
        .exec(&db)
        .await;
    let _ = users::Entity::delete_by_id(s.user_id).exec(&db).await;
}

/// Orphan 情况 A：容器 launch 成功但 DB 持久化失败 → 必须 stop_and_remove 补偿。
#[tokio::test]
async fn launch_db_persistence_failure_compensates_container() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let tag = format!("lc-orp-{}", Uuid::new_v4().simple());
    let s = seed_lifecycle(&db, &tag).await;

    // 占住容器名唯一索引的 running 行：launch 里 persist 的 event_instances insert
    // 将撞 event_instances_container_name_uidx → 事务失败。
    let identifier = format!("JP-{}-{}", s.user_id, s.challenge_id);
    insert_runtime_row(
        &db,
        s.event_id,
        s.challenge_id,
        s.user_id,
        "running",
        None,
        "flag{x}",
    )
    .await;
    // 手动把刚插入行的 container_name 改成 launch 将使用的 identifier
    let blocker = event_instances::Entity::find()
        .filter(event_instances::Column::EventId.eq(s.event_id))
        .filter(event_instances::Column::RuntimeState.eq("running"))
        .one(&db)
        .await
        .unwrap()
        .expect("blocker");
    let mut m = blocker.into_active_model();
    m.container_name = Set(identifier.clone());
    m.update(&db).await.expect("rename blocker");

    let runtime = Arc::new(FakeRuntime::new());
    let svc = InstanceService::new(db.clone(), runtime.clone() as Arc<dyn InstanceRuntime>);

    let result = svc
        .launch(
            s.event_id,
            s.challenge_id,
            identifier.clone(),
            s.user_id,
            None,
            None,
        )
        .await;
    assert!(result.is_err(), "DB 唯一约束冲突必须令 launch 失败");

    let removals = runtime.removals.lock().unwrap();
    assert!(
        removals.iter().any(|r| r == &identifier),
        "DB 持久化失败后必须 stop_and_remove 容器（实际 removals: {removals:?}）"
    );

    // DB 无新实例行（只有 blocker 一行）
    let count = event_instances::Entity::find()
        .filter(event_instances::Column::EventId.eq(s.event_id))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(count, 1, "不得残留半插入的实例行");

    let _ = events::Entity::delete_by_id(s.event_id).exec(&db).await;
    let _ = challenges::Entity::delete_by_id(s.challenge_id)
        .exec(&db)
        .await;
    let _ = users::Entity::delete_by_id(s.user_id).exec(&db).await;
}

/// Orphan 情况 B/C：finalize_launch 在 inspect 失败 / 端口缺失时必须 stop_and_remove。
#[tokio::test]
async fn finalize_launch_compensates_inspect_failure_and_missing_port() {
    use fcmc::{
        ContainerFilter, ContainerHandle, ContainerRuntime, ContainerSpec, ContainerState,
        ExecOptions, ExecOutcome, NetworkHandle, NetworkInspect, NetworkSpec,
    };

    #[derive(Default)]
    struct FakeContainerRuntime {
        inspect_error: bool,
        ports: std::sync::Mutex<std::collections::HashMap<String, u16>>,
        removals: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl ContainerRuntime for FakeContainerRuntime {
        async fn create_network(&self, _spec: NetworkSpec) -> anyhow::Result<NetworkHandle> {
            unimplemented!("unused in test")
        }
        async fn remove_network(&self, _id_or_name: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn inspect_network(&self, _id_or_name: &str) -> anyhow::Result<NetworkInspect> {
            unimplemented!("unused in test")
        }
        async fn create_container(&self, _spec: ContainerSpec) -> anyhow::Result<ContainerHandle> {
            unimplemented!("unused in test")
        }
        async fn start_container(&self, _id_or_name: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn inspect_container(&self, id_or_name: &str) -> anyhow::Result<ContainerState> {
            if self.inspect_error {
                return Err(anyhow::anyhow!("fake inspect failure"));
            }
            Ok(ContainerState {
                container_id: id_or_name.to_string(),
                container_name: "fake".into(),
                image: "fake".into(),
                status: "running".into(),
                running: true,
                labels: Default::default(),
                created_at: None,
                published_ports: self.ports.lock().unwrap().clone(),
                ip_address: None,
            })
        }
        async fn stop_container(
            &self,
            _id_or_name: &str,
            _timeout: std::time::Duration,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn remove_container(&self, id_or_name: &str, _force: bool) -> anyhow::Result<()> {
            self.removals.lock().unwrap().push(id_or_name.to_string());
            Ok(())
        }
        async fn list_containers(
            &self,
            _filter: ContainerFilter,
        ) -> anyhow::Result<Vec<ContainerState>> {
            Ok(vec![])
        }
        async fn logs(&self, _id_or_name: &str, _limit: usize) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        async fn copy_from_container(
            &self,
            _id_or_name: &str,
            _path: &str,
        ) -> anyhow::Result<Vec<u8>> {
            unimplemented!("unused in test")
        }
        async fn copy_into_container(
            &self,
            _id_or_name: &str,
            _dest_dir: &str,
            _tar_bytes: Vec<u8>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn exec(
            &self,
            _id_or_name: &str,
            _options: ExecOptions,
        ) -> anyhow::Result<ExecOutcome> {
            unimplemented!("unused in test")
        }
        async fn restart_container(
            &self,
            _id_or_name: &str,
            _timeout: std::time::Duration,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let handle = ContainerHandle {
        container_id: "cid-1".into(),
        container_name: "c-1".into(),
    };

    // 情况 B：create_and_start 成功、inspect 失败 → stop_and_remove
    let rt = FakeContainerRuntime {
        inspect_error: true,
        ..Default::default()
    };
    let result =
        floatctf::modules::event::jeopardy::infrastructure::container_runtime::finalize_launch(
            &rt, &handle, "8080/tcp",
        )
        .await;
    assert!(result.is_err());
    assert_eq!(rt.removals.lock().unwrap().as_slice(), ["cid-1"]);

    // 情况 C：inspect 成功但 published_ports 缺少期望端口 → stop_and_remove
    let rt2 = FakeContainerRuntime {
        inspect_error: false,
        ports: std::sync::Mutex::new(std::collections::HashMap::from([(
            "9999/tcp".to_string(),
            41414u16,
        )])),
        ..Default::default()
    };
    let result2 =
        floatctf::modules::event::jeopardy::infrastructure::container_runtime::finalize_launch(
            &rt2, &handle, "8080/tcp",
        )
        .await;
    assert!(result2.is_err(), "端口缺失必须失败");
    assert_eq!(rt2.removals.lock().unwrap().as_slice(), ["cid-1"]);

    // 对照：端口存在 → 成功返回 host port，无补偿删除
    let rt3 = FakeContainerRuntime {
        inspect_error: false,
        ports: std::sync::Mutex::new(std::collections::HashMap::from([(
            "8080/tcp".to_string(),
            42424u16,
        )])),
        ..Default::default()
    };
    let port =
        floatctf::modules::event::jeopardy::infrastructure::container_runtime::finalize_launch(
            &rt3, &handle, "8080/tcp",
        )
        .await
        .expect("port");
    assert_eq!(port, 42424);
    assert!(rt3.removals.lock().unwrap().is_empty());
}

// ─────────────────────────────────────────────────────────────────
// §6 系统任务 seed / repair 幂等
// ─────────────────────────────────────────────────────────────────

fn scheduler_for(db: &DatabaseConnection) -> TaskScheduler {
    let web_db: WebDb = web::Data::new(db.clone());
    let docker = web::Data::new(bollard::Docker::connect_with_local_defaults().expect("docker"));
    let rustfs = web::Data::new(aws_sdk_s3::Client::new(
        &aws_config::SdkConfig::builder()
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build(),
    ));
    let logger = LogService::new(web_db.clone());
    TaskScheduler::new(web_db, docker, rustfs, logger)
}

/// 老库升级路径：startup + NULL cron 的旧行必须被修复为 cron + */30 表达式，
/// 且连续两次 seed 幂等（不产生重复任务、不反复重置）。
#[tokio::test]
async fn system_practice_clean_seed_repairs_legacy_startup_row_idempotently() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let sched = scheduler_for(&db);

    // 先跑一次，保证任务存在（全新库路径）
    sched.seed_startup_tasks().await.expect("seed fresh");

    let task_id = floatctf::core::system_ids::SCHED_CLEAN_INSTANCES;
    // 模拟老库：trigger_type=startup、cron_expr=NULL
    let current = scheduled_tasks::Entity::find_by_id(task_id)
        .one(&db)
        .await
        .unwrap()
        .expect("seeded task");
    let mut m = current.into_active_model();
    m.trigger_type = Set("startup".to_string());
    m.cron_expr = Set(None);
    m.status = Set("completed".to_string());
    m.execute_at = Set(None);
    m.update(&db).await.expect("degrade to legacy");

    // 再跑两次 seed：修复 + 幂等
    sched.seed_startup_tasks().await.expect("repair");
    sched.seed_startup_tasks().await.expect("idempotent re-run");

    let rows = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::TaskKey.eq("system.practice.clean"))
        .filter(scheduled_tasks::Column::Id.eq(task_id))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "seed 不得产生重复任务行");

    let task = &rows[0];
    assert_eq!(task.trigger_type, "cron", "老库 startup 必须修复为 cron");
    assert_eq!(task.cron_expr.as_deref(), Some("*/30 * * * * *"));
    assert!(task.protected, "系统任务必须 protected");
    assert_eq!(task.status, "pending", "修复后必须恢复 pending");
    assert!(task.execute_at.is_some(), "修复后 execute_at 必须有值");

    // 全新库字段契约：三个平台任务齐全且字段正确
    for &(id, _name, task_key, trigger) in
        floatctf::core::system_ids::startup_scheduled_task_seeds()
    {
        let row = scheduled_tasks::Entity::find_by_id(id)
            .one(&db)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("task {task_key} must exist"));
        assert_eq!(row.trigger_type, trigger, "{task_key} trigger");
        let expected_cron = floatctf::core::system_ids::system_task_cron_expr(task_key);
        assert_eq!(row.cron_expr.as_deref(), expected_cron, "{task_key} cron");
        assert!(row.protected, "{task_key} protected");
    }
}
