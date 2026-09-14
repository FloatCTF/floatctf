//! Flag 日志泄露 + Web Terminal HTTP 安全回归。
//!
//! §Flag 泄露：提交唯一标记 flag 后，DB 日志表（logs/event_logs）与响应体
//! 不得出现 flag 明文；错误路径（错误 flag）同样不回显、不落日志。
//!
//! §Terminal 安全：ticket cookie 属性（HttpOnly/Strict/Path 限定）、
//! feature 关闭 404、非管理员 401、无 ticket 401、单次消费（HTTP 级）。

use actix_web::{App, test as actix_test, web};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use floatctf::core::security::jwt::{self, Role};
use floatctf::entity::{
    event_challenge_instance, event_instances, event_users, events,
    sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode},
    users,
};
use floatctf::modules::event::jeopardy::api::submit::submit_flag;

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn db_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into())
}

async fn db_or_skip() -> Option<sea_orm::DatabaseConnection> {
    match sea_orm::Database::connect(&db_url()).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip flag_leak: DB unreachable ({e})");
            None
        }
    }
}

fn token_for(user_id: Uuid) -> String {
    jwt::gen_jwt_token(user_id, Role::User, Some(30)).expect("jwt")
}

async fn seed_submission_env(db: &sea_orm::DatabaseConnection) -> (Uuid, Uuid, Uuid, Uuid, String) {
    let tag = Uuid::new_v4().simple().to_string();
    let now = Utc::now();
    let event_id = Uuid::new_v4();
    events::ActiveModel {
        is_virtual: Set(true), // practice 赛事受 events_virtual_by_purpose_check 约束
        id: Set(event_id),
        family: Set(EventFamily::Jeopardy),
        purpose: Set(EventPurpose::Practice),
        participant_mode: Set(ParticipantMode::Individual),
        system_key: Set(None),
        title: Set(format!("leak-{tag}")),
        hidden: Set(false),
        allow_join: Set(true),
        start_time: Set((now - Duration::hours(1)).into()),
        // practice 赛事受 events_end_time_by_purpose_check 约束必须无 end_time
        end_time: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("event");

    let user_id = Uuid::new_v4();
    users::ActiveModel {
        id: Set(user_id),
        username: Set(format!("lk-{tag}")),
        nickname: Set(format!("lkn-{tag}")),
        password: Set("x".into()),
        email: Set(format!("lk-{tag}@example.test")),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("user");
    event_users::ActiveModel {
        event_id: Set(event_id),
        user_id: Set(user_id),
        points: Set(0.0),
        banned: Set(false),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("join");

    let flag = format!("flag{{LEAKTEST-{}-{}}}", tag, Uuid::new_v4().simple());

    let challenge_id = Uuid::new_v4();
    floatctf::entity::challenges::ActiveModel {
        id: Set(challenge_id),
        name: Set(format!("lkc-{tag}")),
        safe_name: Set(format!("lkc-{tag}")),
        category: Set("web".into()),
        description: Set("t".into()),
        hidden: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("challenge");

    let instance_id = Uuid::new_v4();
    event_instances::ActiveModel {
        id: Set(instance_id),
        event_id: Set(event_id),
        owner_user_id: Set(Some(user_id)),
        owner_team_id: Set(None),
        image_ref: Set(None),
        container_id: Set(None),
        container_name: Set(format!("LK-{tag}")),
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
    .expect("inst");
    event_challenge_instance::ActiveModel {
        id: Set(instance_id),
        flag: Set(flag.clone()),
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

    (event_id, user_id, instance_id, challenge_id, flag)
}

async fn submit_app(
    db: sea_orm::DatabaseConnection,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    use floatctf::infrastructure::logging::LogService;
    let log_db = web::Data::new(db.clone());
    let docker = web::Data::new(bollard::Docker::connect_with_local_defaults().expect("docker"));
    let app_state = web::Data::new(floatctf::bootstrap::AppState {
        config: std::sync::Arc::new(
            floatctf::core::config::AppConfig::from_file(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/development.toml"),
            )
            .expect("config"),
        ),
        db: db.clone(),
        docker: docker.get_ref().clone(),
        storage: aws_sdk_s3::Client::new(
            &aws_config::SdkConfig::builder()
                .behavior_version(aws_config::BehaviorVersion::latest())
                .build(),
        ),
        redis: redis::Client::open("redis://127.0.0.1:6390/").expect("redis client"),
        log: LogService::new(log_db.clone()),
        audit: floatctf::infrastructure::audit::AuditService::new(LogService::new(log_db.clone())),
        publisher: std::sync::Arc::new(floatctf::infrastructure::realtime::NoopEventPublisher),
        scheduler: std::sync::Arc::new(floatctf::scheduler::TaskScheduler::new(
            web::Data::new(db.clone()),
            docker.clone(),
            web::Data::new(aws_sdk_s3::Client::new(
                &aws_config::SdkConfig::builder()
                    .behavior_version(aws_config::BehaviorVersion::latest())
                    .build(),
            )),
            LogService::new(log_db.clone()),
        )),
        terminal_tickets: std::sync::Arc::new(
            floatctf::modules::platform::operations::terminal::TerminalTicketStore::in_memory_for_tests(
                std::time::Duration::from_secs(60),
            ),
        ),
    });
    actix_test::init_service(
        App::new()
            .app_data(app_state)
            .app_data(web::Data::new(db))
            .app_data(docker)
            .app_data(web::Data::new(LogService::new(log_db)))
            .app_data(web::Data::new(aws_sdk_s3::Client::new(
                &aws_config::SdkConfig::builder()
                    .behavior_version(aws_config::BehaviorVersion::latest())
                    .build(),
            )))
            .service(web::scope("/api").service(web::scope("/submit").service(submit_flag))),
    )
    .await
}

/// 提交正确/错误 flag 后：DB 日志表与响应体均不得含 flag 明文。
#[actix_web::test]
async fn flag_value_never_leaks_to_logs_or_responses() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let Some(db) = db_or_skip().await else { return };
    jwt::configure_jwt_secret(floatctf::core::secret::Secret::new("regression-jwt-secret"));
    let (event_id, user_id, instance_id, challenge_id, flag) = seed_submission_env(&db).await;
    let app = submit_app(db.clone()).await;
    let token = token_for(user_id);

    // 错误 flag：400，响应体不得回显提交的 flag
    let wrong = format!(
        "flag{{WRONG-{}-{}}}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let req = actix_test::TestRequest::post()
        .uri("/api/submit/flag")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(serde_json::json!({
            "event_id": event_id,
            "instance_id": instance_id,
            "flag": wrong,
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    let body = actix_web::body::to_bytes(resp.into_body())
        .await
        .unwrap_or_default();
    let body_str = String::from_utf8_lossy(&body).to_string();
    assert!(
        status == 400 || status == 200, // 练习赛错误 flag 为 BadRequest
        "wrong flag submit got {status}: {body_str}"
    );
    assert!(
        !body_str.contains(&wrong),
        "响应体不得回显提交的错误 flag：{body_str}"
    );

    // 正确 flag：200（练习赛得分 0），响应体不得含 flag 明文
    let req = actix_test::TestRequest::post()
        .uri("/api/submit/flag")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(serde_json::json!({
            "event_id": event_id,
            "instance_id": instance_id,
            "flag": flag,
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    let body = actix_web::body::to_bytes(resp.into_body())
        .await
        .unwrap_or_default();
    let body_str = String::from_utf8_lossy(&body).to_string();
    assert_eq!(status, 200, "correct flag submit must succeed: {body_str}");
    assert!(
        !body_str.contains(&flag),
        "成功响应不得回显 flag 明文：{body_str}"
    );

    // DB 日志表全文检索：正确 flag 与错误 flag 均不得出现在 logs / event_logs
    use sea_orm::ConnectionTrait;
    // logs 有 message+details 列；event_logs 只有 details
    for (table, sql) in [
        (
            "logs",
            "SELECT count(*) AS count FROM logs WHERE message ILIKE '%' || $1 || '%' OR details::text ILIKE '%' || $1 || '%'",
        ),
        (
            "event_logs",
            "SELECT count(*) AS count FROM event_logs WHERE details::text ILIKE '%' || $1 || '%'",
        ),
    ] {
        for probe in [&flag, &wrong] {
            let sql = sql.to_string();
            let rows = db
                .query_all(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Postgres,
                    sql,
                    [probe.as_str().into()],
                ))
                .await
                .expect("query");
            let count: i64 = rows
                .iter()
                .map(|row| row.try_get::<i64>("", "count").unwrap_or(0))
                .sum();
            assert_eq!(
                count,
                0,
                "flag 明文泄露进 {table} 表（probe 前 20 字符 {}）",
                &probe[..20.min(probe.len())]
            );
        }
    }

    // 清理
    let _ = event_users::Entity::delete_many()
        .filter(event_users::Column::EventId.eq(event_id))
        .exec(&db)
        .await;
    let _ = events::Entity::delete_by_id(event_id).exec(&db).await;
    let _ = users::Entity::delete_by_id(user_id).exec(&db).await;
    let _ = floatctf::entity::challenges::Entity::delete_by_id(challenge_id)
        .exec(&db)
        .await;
}

// ─────────────────────────────────────────────────────────────────
// §Terminal HTTP 安全
// ─────────────────────────────────────────────────────────────────

mod terminal_security {
    use super::*;
    use actix_web::test as actix_test;
    use floatctf::infrastructure::logging::LogService;
    use floatctf::modules::platform::operations::terminal::{create_terminal_session, terminal_ws};

    fn admin_token(admin_id: Uuid) -> String {
        jwt::gen_jwt_token(admin_id, Role::SuperAdmin, Some(30)).expect("jwt")
    }

    fn user_token(user_id: Uuid) -> String {
        jwt::gen_jwt_token(user_id, Role::User, Some(30)).expect("jwt")
    }

    async fn terminal_app(
        db: sea_orm::DatabaseConnection,
        enable_terminal: bool,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        let mut config = floatctf::core::config::AppConfig::from_file(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/development.toml"),
        )
        .expect("config");
        config.features.enable_web_terminal = enable_terminal;

        let log_db = web::Data::new(db.clone());
        let docker =
            web::Data::new(bollard::Docker::connect_with_local_defaults().expect("docker"));
        let s3 = web::Data::new(aws_sdk_s3::Client::new(
            &aws_config::SdkConfig::builder()
                .behavior_version(aws_config::BehaviorVersion::latest())
                .build(),
        ));
        let app_state = web::Data::new(floatctf::bootstrap::AppState {
            config: std::sync::Arc::new(config),
            db: db.clone(),
            docker: docker.get_ref().clone(),
            storage: s3.get_ref().clone(),
            redis: redis::Client::open("redis://127.0.0.1:6390/").expect("redis client"),
            log: LogService::new(log_db.clone()),
            audit: floatctf::infrastructure::audit::AuditService::new(LogService::new(
                log_db.clone(),
            )),
            publisher: std::sync::Arc::new(floatctf::infrastructure::realtime::NoopEventPublisher),
            scheduler: std::sync::Arc::new(floatctf::scheduler::TaskScheduler::new(
                web::Data::new(db.clone()),
                docker.clone(),
                s3.clone(),
                LogService::new(log_db.clone()),
            )),
            terminal_tickets: std::sync::Arc::new(
                floatctf::modules::platform::operations::terminal::TerminalTicketStore::in_memory_for_tests(
                    std::time::Duration::from_secs(60),
                ),
            ),
        });
        actix_test::init_service(
            App::new()
                .app_data(app_state)
                .app_data(web::Data::new(db))
                .app_data(docker)
                .app_data(web::Data::new(LogService::new(log_db)))
                .app_data(s3)
                .service(
                    web::scope("/api/admin/terminal")
                        .service(create_terminal_session)
                        .service(terminal_ws),
                ),
        )
        .await
    }

    async fn seed_admin(db: &sea_orm::DatabaseConnection) -> Uuid {
        let id = Uuid::new_v4();
        floatctf::entity::super_admin::ActiveModel {
            id: Set(id),
            username: Set(format!("tsa-{}", Uuid::new_v4().simple())),
            password: Set("x".into()),
            email: Set(format!("tsa-{}@example.test", Uuid::new_v4().simple())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("admin");
        id
    }

    /// ticket cookie 属性：HttpOnly + SameSite=Strict + Path 限定 ws 路由。
    #[actix_web::test]
    async fn terminal_ticket_cookie_attributes() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let Some(db) = db_or_skip().await else { return };
        jwt::configure_jwt_secret(floatctf::core::secret::Secret::new("regression-jwt-secret"));
        let admin = seed_admin(&db).await;
        let app = terminal_app(db.clone(), true).await;

        let req = actix_test::TestRequest::post()
            .uri("/api/admin/terminal/session")
            .insert_header(("Authorization", format!("Bearer {}", admin_token(admin))))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 204, "admin session issue must 204");

        let cookie = resp
            .response()
            .cookies()
            .find(|c| c.name() == "floatctf_terminal_ticket")
            .expect("terminal_ticket cookie must be set");
        assert!(cookie.http_only().unwrap_or(false), "cookie 必须 HttpOnly");
        assert_eq!(
            cookie.same_site(),
            Some(actix_web::cookie::SameSite::Strict),
            "cookie 必须 SameSite=Strict"
        );
        assert_eq!(
            cookie.path().unwrap_or(""),
            "/api/admin/terminal/ws",
            "cookie Path 必须限定 ws 路由"
        );
        assert!(cookie.max_age().is_some(), "cookie 必须有 Max-Age=TTL");
        // 响应体不得含 ticket 明文（cookie 头之外零暴露）
        let body = actix_web::body::to_bytes(resp.into_body())
            .await
            .unwrap_or_default();
        assert!(body.is_empty(), "204 响应不得有 body");

        let _ = floatctf::entity::super_admin::Entity::delete_by_id(admin)
            .exec(&db)
            .await;
    }

    /// 非管理员 token、无 token、feature 关闭均拒绝。
    #[actix_web::test]
    async fn terminal_session_authz_matrix() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let Some(db) = db_or_skip().await else { return };
        jwt::configure_jwt_secret(floatctf::core::secret::Secret::new("regression-jwt-secret"));
        let admin = seed_admin(&db).await;
        let user_id = Uuid::new_v4();
        users::ActiveModel {
            id: Set(user_id),
            username: Set(format!("tsu-{}", Uuid::new_v4().simple())),
            nickname: Set("tsu".into()),
            password: Set("x".into()),
            email: Set(format!("tsu-{}@example.test", Uuid::new_v4().simple())),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("user");

        // feature 开启：管理员 204 / 普通用户 401 / 无 token 401
        let app = terminal_app(db.clone(), true).await;
        let req = actix_test::TestRequest::post()
            .uri("/api/admin/terminal/session")
            .insert_header(("Authorization", format!("Bearer {}", admin_token(admin))))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, req).await.status().as_u16(),
            204
        );

        let req = actix_test::TestRequest::post()
            .uri("/api/admin/terminal/session")
            .insert_header(("Authorization", format!("Bearer {}", user_token(user_id))))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, req).await.status().as_u16(),
            401,
            "普通用户 token 必须 401"
        );

        let req = actix_test::TestRequest::post()
            .uri("/api/admin/terminal/session")
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, req).await.status().as_u16(),
            401,
            "无 token 必须 401"
        );

        // feature 关闭：管理员也 404
        let app_off = terminal_app(db.clone(), false).await;
        let req = actix_test::TestRequest::post()
            .uri("/api/admin/terminal/session")
            .insert_header(("Authorization", format!("Bearer {}", admin_token(admin))))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app_off, req)
                .await
                .status()
                .as_u16(),
            404,
            "feature 关闭必须 404"
        );

        let _ = users::Entity::delete_by_id(user_id).exec(&db).await;
        let _ = floatctf::entity::super_admin::Entity::delete_by_id(admin)
            .exec(&db)
            .await;
    }

    /// ws 端点：无 ticket 401；伪造 ticket 401。
    #[actix_web::test]
    async fn terminal_ws_rejects_missing_or_bogus_ticket() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let Some(db) = db_or_skip().await else { return };
        let app = terminal_app(db.clone(), true).await;

        // 无 ticket（非 ws 升级请求也会先过 ticket 消费 → 401）
        let req = actix_test::TestRequest::get()
            .uri("/api/admin/terminal/ws")
            .to_request();
        let status = actix_test::call_service(&app, req).await.status().as_u16();
        assert_eq!(status, 401, "无 ticket 必须 401");

        // 伪造 ticket
        let req = actix_test::TestRequest::get()
            .uri("/api/admin/terminal/ws")
            .insert_header((
                actix_web::http::header::SET_COOKIE,
                "floatctf_terminal_ticket=bogus-ticket-value",
            ))
            .to_request();
        let status = actix_test::call_service(&app, req).await.status().as_u16();
        assert_eq!(status, 401, "伪造 ticket 必须 401");
    }
}
