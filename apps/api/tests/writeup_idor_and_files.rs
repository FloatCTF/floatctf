//! Writeup 提交 IDOR / 文件处理回归（真实 handler + dev DB + dev RustFS）。
//!
//! 覆盖：
//! - Team 模式：合法成员提交、伪造 team_id（他队）必须 403、跨 event 队伍拒绝、
//!   未参赛拒绝、banned user 拒绝、banned team 拒绝
//! - Individual 模式：team_id=None 成功、任意 team_id 400
//! - 文件处理：合法 PDF magic 通过、伪造 magic 拒绝、空文件拒绝、<5 字节拒绝、
//!   >50MB 单字段 multipart 拒绝、~49MB 流式接受（不整读内存）
//! - 授权主体由服务端解析：team 模式提交后 event_writeup.team_id 恒等于成员真实队伍
//!
//! 依赖 dev 库（DATABASE_URL 默认 postgres://...5432/floatctf_db）与
//! dev RustFS（http://127.0.0.1:9000）；不可达时 soft-skip。

use actix_web::{App, test as actix_test, web};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::core::security::jwt::{self, Role};
use floatctf::entity::{
    event_team_members, event_teams, event_users, event_writeup, events,
    sea_orm_active_enums::{EventFamily, EventPurpose, EventTeamMemberRole, ParticipantMode},
    users,
};
use floatctf::infrastructure::storage;
use floatctf::modules::event::jeopardy::api::submit::submit_writeup;

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn deps_or_skip() -> Option<(sea_orm::DatabaseConnection, aws_sdk_s3::Client)> {
    let db = match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("skip writeup_idor: DB unreachable ({e})");
            return None;
        }
    };
    let s3_config = floatctf::core::config::StorageConfig {
        endpoint_url: "http://127.0.0.1:9000".to_string(),
        region: "us-east-1".to_string(),
        access_key_id: "rustfsadmin".to_string(),
        secret_access_key: floatctf::core::secret::Secret::new("rustfsadmin"),
    };
    let s3 = match storage::connect(&s3_config).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skip writeup_idor: RustFS unreachable ({e})");
            return None;
        }
    };
    Some((db, s3))
}

struct Seed {
    event_id: Uuid,
    users: Vec<users::Model>,
    team_a: Uuid,
    team_b: Uuid,
}

/// Team 赛事：user0∈teamA(captain)、user1∈teamA、user2∈teamB、user3 未参赛、user4 banned。
async fn seed_team_event(db: &sea_orm::DatabaseConnection) -> Seed {
    let tag = Uuid::new_v4().simple().to_string();
    let now = Utc::now();
    let event_id = Uuid::new_v4();
    events::ActiveModel {
        is_virtual: Set(false),
        id: Set(event_id),
        family: Set(EventFamily::Jeopardy),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Team),
        system_key: Set(None),
        title: Set(format!("wp-team-{tag}")),
        hidden: Set(false),
        allow_join: Set(true),
        start_time: Set((now - Duration::hours(1)).into()),
        end_time: Set(Some((now + Duration::hours(4)).fixed_offset())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("event");

    let team_a = Uuid::new_v4();
    let team_b = Uuid::new_v4();
    for (tid, name) in [(team_a, "A"), (team_b, "B")] {
        event_teams::ActiveModel {
            id: Set(tid),
            event_id: Set(event_id),
            name: Set(format!("wp-team-{name}-{tag}")),
            points: Set(0.0),
            banned: Set(false),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("team");
    }

    let mut seeded = Vec::new();
    for (i, (uid, team, banned)) in [
        (Uuid::new_v4(), Some(team_a), false),
        (Uuid::new_v4(), Some(team_a), false),
        (Uuid::new_v4(), Some(team_b), false),
        (Uuid::new_v4(), None, false),
        (Uuid::new_v4(), Some(team_a), true),
    ]
    .into_iter()
    .enumerate()
    {
        let u = users::ActiveModel {
            id: Set(uid),
            username: Set(format!("wp{i}-{tag}")),
            nickname: Set(format!("wpn{i}-{tag}")),
            password: Set("x".into()),
            email: Set(format!("wp{i}-{tag}@example.test")),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("user");
        event_users::ActiveModel {
            event_id: Set(event_id),
            user_id: Set(uid),
            points: Set(0.0),
            banned: Set(banned),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("join");
        if let Some(t) = team {
            event_team_members::ActiveModel {
                event_id: Set(event_id),
                team_id: Set(t),
                user_id: Set(uid),
                role: Set(EventTeamMemberRole::Member),
                ..Default::default()
            }
            .insert(db)
            .await
            .expect("member");
        }
        seeded.push(u);
    }

    Seed {
        event_id,
        users: seeded,
        team_a,
        team_b,
    }
}

async fn seed_individual_event(db: &sea_orm::DatabaseConnection) -> (Uuid, users::Model) {
    let tag = Uuid::new_v4().simple().to_string();
    let now = Utc::now();
    let event_id = Uuid::new_v4();
    events::ActiveModel {
        is_virtual: Set(false),
        id: Set(event_id),
        family: Set(EventFamily::Jeopardy),
        purpose: Set(EventPurpose::Competition),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(None),
        title: Set(format!("wp-ind-{tag}")),
        hidden: Set(false),
        allow_join: Set(true),
        start_time: Set((now - Duration::hours(1)).into()),
        end_time: Set(Some((now + Duration::hours(4)).fixed_offset())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("event");
    let uid = Uuid::new_v4();
    let u = users::ActiveModel {
        id: Set(uid),
        username: Set(format!("wpi-{tag}")),
        nickname: Set(format!("wpin-{tag}")),
        password: Set("x".into()),
        email: Set(format!("wpi-{tag}@example.test")),
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
    (event_id, u)
}

/// 构造挂载真实 submit_writeup handler 的测试 App。
async fn test_app(
    db: sea_orm::DatabaseConnection,
    s3: aws_sdk_s3::Client,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let app_state = web::Data::new(floatctf::bootstrap::AppState {
        config: std::sync::Arc::new(
            floatctf::core::config::AppConfig::from_file(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/development.toml"),
            )
            .expect("config"),
        ),
        db: db.clone(),
        docker: bollard::Docker::connect_with_local_defaults().expect("docker"),
        storage: s3.clone(),
        redis: redis::Client::open("redis://127.0.0.1:6390/").expect("redis client"),
        log: floatctf::infrastructure::logging::LogService::new(web::Data::new(db.clone())),
        audit: floatctf::infrastructure::audit::AuditService::new(
            floatctf::infrastructure::logging::LogService::new(web::Data::new(db.clone())),
        ),
        publisher: std::sync::Arc::new(floatctf::infrastructure::realtime::NoopEventPublisher),
        scheduler: std::sync::Arc::new(floatctf::scheduler::TaskScheduler::new(
            web::Data::new(db.clone()),
            web::Data::new(bollard::Docker::connect_with_local_defaults().expect("docker")),
            web::Data::new(s3.clone()),
            floatctf::infrastructure::logging::LogService::new(web::Data::new(db.clone())),
        )),
        terminal_tickets: std::sync::Arc::new(
            floatctf::modules::platform::operations::terminal::TerminalTicketStore::in_memory_for_tests(
                std::time::Duration::from_secs(60),
            ),
        ),
    });
    actix_test::init_service(
        App::new()
            // 与 bootstrap::run() 相同的 413 映射（生产链路必须与测试链路一致）
            .app_data(web::Data::new(
                actix_multipart::form::MultipartFormConfig::default().error_handler(|err, _req| {
                    let overflow = matches!(
                        &err,
                        actix_multipart::MultipartError::Payload(
                            actix_web::error::PayloadError::Overflow
                        )
                    ) || err.to_string().contains("Overflow");
                    if overflow {
                        actix_web::error::PayloadError::Overflow.into()
                    } else {
                        err.into()
                    }
                }),
            ))
            .app_data(app_state)
            .app_data(web::Data::new(db))
            .app_data(web::Data::new(
                bollard::Docker::connect_with_local_defaults().expect("docker"),
            ))
            .app_data(web::Data::new(s3.clone()))
            .app_data(web::Data::new(
                floatctf::infrastructure::logging::LogService::new(web::Data::new(
                    sea_orm::Database::connect(&db_url()).await.expect("db2"),
                )),
            ))
            .service(web::scope("/api").service(web::scope("/submit").service(submit_writeup))),
    )
    .await
}

/// 构造 multipart/form-data body。
fn multipart_body(
    boundary: &str,
    event_id: Uuid,
    team_id: Option<Uuid>,
    filename: &str,
    content: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    let fields: Vec<(&str, Option<String>)> = vec![
        ("event_id", Some(event_id.to_string())),
        ("team_id", team_id.map(|t| t.to_string())),
    ];
    for (name, value) in fields {
        if let Some(v) = value {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            );
            body.extend_from_slice(v.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
    }
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"writeup_pdf\"; filename=\"{filename}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/pdf\r\n\r\n");
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// 发送 writeup multipart 请求并返回 HTTP status。
macro_rules! post_writeup {
    ($app:expr, $token:expr, $event_id:expr, $team_id:expr, $filename:expr, $content:expr) => {{
        let boundary = "----floatctfregression";
        let body = multipart_body(boundary, $event_id, $team_id, $filename, $content);
        let req = actix_test::TestRequest::post()
            .uri("/api/submit/writeup")
            .insert_header(("Authorization", format!("Bearer {}", $token)))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(body)
            .to_request();
        let resp = actix_test::call_service(&$app, req).await;
        resp.status().as_u16()
    }};
}

fn token_for(user_id: Uuid) -> String {
    jwt::gen_jwt_token(user_id, Role::User, Some(30)).expect("jwt")
}

const VALID_PDF_MIN: &[u8] = b"%PDF-1.4\n minimal but valid magic";

async fn cleanup(db: &sea_orm::DatabaseConnection, seed: &Seed) {
    let _ = event_writeup::Entity::delete_many()
        .filter(event_writeup::Column::EventId.eq(seed.event_id))
        .exec(db)
        .await;
    let _ = event_team_members::Entity::delete_many()
        .filter(event_team_members::Column::EventId.eq(seed.event_id))
        .exec(db)
        .await;
    let _ = event_users::Entity::delete_many()
        .filter(event_users::Column::EventId.eq(seed.event_id))
        .exec(db)
        .await;
    let _ = events::Entity::delete_by_id(seed.event_id).exec(db).await;
    for u in &seed.users {
        let _ = users::Entity::delete_by_id(u.id).exec(db).await;
    }
}

/// Team 模式 IDOR 全矩阵。
#[actix_web::test]
async fn writeup_team_mode_idor_matrix() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some((db, s3)) = deps_or_skip().await else {
        return;
    };
    jwt::configure_jwt_secret(floatctf::core::secret::Secret::new("regression-jwt-secret"));
    let seed = seed_team_event(&db).await;
    let app = test_app(db.clone(), s3).await;

    let u0 = seed.users[0].id; // teamA member
    let u2 = seed.users[2].id; // teamB member
    let u3 = seed.users[3].id; // not joined
    let u4 = seed.users[4].id; // banned

    // 1. 合法成员、不传 team_id：200，写入行 team_id == 真实队伍（服务端解析）
    let status = post_writeup!(
        &app,
        &token_for(u0),
        seed.event_id,
        None,
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 200, "team member no-team_id submit must succeed");
    let row = event_writeup::Entity::find_by_id((seed.event_id, u0))
        .one(&db)
        .await
        .expect("q")
        .expect("row");
    assert_eq!(row.team_id, Some(seed.team_a), "授权主体必须由服务端解析");

    // 2. 合法成员、传自己的 team_id：200
    let status = post_writeup!(
        &app,
        &token_for(u0),
        seed.event_id,
        Some(seed.team_a),
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 200, "team member own-team submit must succeed");

    // 3. 伪造 team_id（对方队伍）：403，且不覆盖 teamB 的对象
    let status = post_writeup!(
        &app,
        &token_for(u0),
        seed.event_id,
        Some(seed.team_b),
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 403, "forged team_id must be 403");
    let team_b_rows = event_writeup::Entity::find()
        .filter(event_writeup::Column::EventId.eq(seed.event_id))
        .filter(event_writeup::Column::TeamId.eq(seed.team_b))
        .all(&db)
        .await
        .expect("q");
    assert!(team_b_rows.is_empty(), "不得写入/覆盖他队 writeup 行");

    // 4. 未参赛用户：403
    let status = post_writeup!(
        &app,
        &token_for(u3),
        seed.event_id,
        None,
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 403, "not-joined user must be 403");

    // 5. banned user：403
    let status = post_writeup!(
        &app,
        &token_for(u4),
        seed.event_id,
        None,
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 403, "banned user must be 403");

    // 6. 跨 event 的队伍 id：403（event_team_members 查询限定本 event）
    let status = post_writeup!(
        &app,
        &token_for(u2),
        seed.event_id,
        Some(Uuid::new_v4()),
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 403, "random team id must be 403");

    // 7. banned team：403
    {
        use sea_orm::IntoActiveModel;
        let t = event_teams::Entity::find_by_id(seed.team_b)
            .one(&db)
            .await
            .unwrap()
            .expect("team");
        let mut m = t.into_active_model();
        m.banned = Set(true);
        m.update(&db).await.expect("ban");
    }
    let status = post_writeup!(
        &app,
        &token_for(u2),
        seed.event_id,
        None,
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 403, "banned team member must be 403");

    // 8. 练习赛（purpose != Competition）：400
    let tag = Uuid::new_v4().simple().to_string();
    let practice_id = Uuid::new_v4();
    events::ActiveModel {
        is_virtual: Set(true), // practice 赛事受 events_virtual_by_purpose_check 约束必须 virtual
        id: Set(practice_id),
        family: Set(EventFamily::Jeopardy),
        purpose: Set(EventPurpose::Practice),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(None),
        title: Set(format!("wp-prac-{tag}")),
        hidden: Set(false),
        allow_join: Set(true),
        start_time: Set(Utc::now().into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("practice event");
    let status = post_writeup!(
        &app,
        &token_for(u0),
        practice_id,
        None,
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 400, "practice event writeup must be 400");
    let _ = events::Entity::delete_by_id(practice_id).exec(&db).await;

    cleanup(&db, &seed).await;
}

/// Individual 模式：team_id=None 成功；任意 team_id 400。
#[actix_web::test]
async fn writeup_individual_mode_rejects_team_id() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some((db, s3)) = deps_or_skip().await else {
        return;
    };
    jwt::configure_jwt_secret(floatctf::core::secret::Secret::new("regression-jwt-secret"));
    let (event_id, user) = seed_individual_event(&db).await;
    let app = test_app(db.clone(), s3).await;

    let status = post_writeup!(
        &app,
        &token_for(user.id),
        event_id,
        None,
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 200, "individual submit must succeed");

    let status = post_writeup!(
        &app,
        &token_for(user.id),
        event_id,
        Some(Uuid::new_v4()),
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 400, "individual event with team_id must be 400");

    let row = event_writeup::Entity::find_by_id((event_id, user.id))
        .one(&db)
        .await
        .expect("q")
        .expect("row");
    assert_eq!(row.team_id, None, "individual writeup team_id 必须为 NULL");

    // 未参赛 individual 用户：403
    let outsider = Uuid::new_v4();
    users::ActiveModel {
        id: Set(outsider),
        username: Set(format!("wpo-{}", Uuid::new_v4().simple())),
        nickname: Set("wpo".into()),
        password: Set("x".into()),
        email: Set(format!("wpo-{}@example.test", Uuid::new_v4().simple())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("outsider");
    let status = post_writeup!(
        &app,
        &token_for(outsider),
        event_id,
        None,
        "w.pdf",
        VALID_PDF_MIN
    );
    assert_eq!(status, 403, "not-joined individual must be 403");

    let _ = event_writeup::Entity::delete_many()
        .filter(event_writeup::Column::EventId.eq(event_id))
        .exec(&db)
        .await;
    let _ = event_users::Entity::delete_many()
        .filter(event_users::Column::EventId.eq(event_id))
        .exec(&db)
        .await;
    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = users::Entity::delete_by_id(user.id).exec(&db).await;
    let _ = users::Entity::delete_by_id(outsider).exec(&db).await;
}

/// 文件处理：magic 校验 / 大小边界 / 49MB 流式。
#[actix_web::test]
async fn writeup_file_handling_magic_and_size_limits() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some((db, s3)) = deps_or_skip().await else {
        return;
    };
    jwt::configure_jwt_secret(floatctf::core::secret::Secret::new("regression-jwt-secret"));
    let (event_id, user) = seed_individual_event(&db).await;
    let app = test_app(db.clone(), s3).await;
    let token = token_for(user.id);

    // 伪造扩展名：evil.pdf 内容非 PDF → 400
    let status = post_writeup!(
        &app,
        &token,
        event_id,
        None,
        "evil.pdf",
        b"hello world not a pdf"
    );
    assert_eq!(status, 400, "non-PDF content must be 400");

    // 空文件 → 400（read_exact 5 字节失败）
    let status = post_writeup!(&app, &token, event_id, None, "empty.pdf", b"");
    assert_eq!(status, 400, "empty file must be 400");

    // <5 字节 → 400
    let status = post_writeup!(&app, &token, event_id, None, "tiny.pdf", b"%PD");
    assert_eq!(status, 400, "<5-byte file must be 400");

    // 恰好 5 字节 "%PDF-" → 200（magic 通过）
    let status = post_writeup!(&app, &token, event_id, None, "min.pdf", b"%PDF-");
    assert_eq!(status, 200, "exactly-%PDF- file must pass magic");

    // 边界语义：`#[multipart(limit = "50MB")]` 经 parse_size 解析为十进制 50_000_000 字节
    //（非 50MiB=52_428_800），超限经 MultipartFormConfig err_handler 映射为
    // 413 Payload Too Large（actix-web PayloadError::Overflow）。

    // 恰好 50_000_000 字节（=limit，checked_sub 语义含边界值）→ 200
    let mut exact = Vec::with_capacity(50_000_000);
    exact.extend_from_slice(b"%PDF-");
    exact.extend(std::iter::repeat_n(b'a', 50_000_000 - 5));
    let status = post_writeup!(&app, &token, event_id, None, "exact.pdf", &exact);
    assert_eq!(
        status, 200,
        "exactly-limit (50,000,000 B) PDF must be accepted"
    );
    drop(exact);

    // 50_000_005 字节（limit+5）→ 413
    let mut over = Vec::with_capacity(50_000_005);
    over.extend_from_slice(b"%PDF-");
    over.extend(std::iter::repeat_n(b'b', 50_000_005 - 5));
    let status = post_writeup!(&app, &token, event_id, None, "over.pdf", &over);
    assert_eq!(
        status, 413,
        "limit+5 bytes must be rejected with 413 Payload Too Large"
    );
    drop(over);

    // 大文件流式验证：47MiB（49,283,072 B < 50,000,000）→ 200
    //（handler 只读 5 字节 magic + ByteStream::from_path 流式上传，不整读内存）
    let mut big = Vec::with_capacity(47 * 1024 * 1024);
    big.extend_from_slice(b"%PDF-");
    big.extend(std::iter::repeat_n(b'c', 47 * 1024 * 1024 - 5));
    let status = post_writeup!(&app, &token, event_id, None, "big.pdf", &big);
    assert_eq!(status, 200, "47MiB PDF must be accepted (streamed)");
    drop(big);

    let _ = event_writeup::Entity::delete_many()
        .filter(event_writeup::Column::EventId.eq(event_id))
        .exec(&db)
        .await;
    let _ = event_users::Entity::delete_many()
        .filter(event_users::Column::EventId.eq(event_id))
        .exec(&db)
        .await;
    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = users::Entity::delete_by_id(user.id).exec(&db).await;
}
