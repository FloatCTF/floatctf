//! Wave 4：认证 v2 契约测试（DB-gated）。
//!
//! 覆盖 [`resolve_session_account`] 的**完整失败矩阵**与成功路径，以及
//! `401 / 403` 分流契约。守卫逻辑是纯函数（`DB + HttpRequest`），因此可以用
//! `actix_web::test::TestRequest` 在**不启动任何服务器、不绑定端口**的情况下测试
//! ——既真实又不违反「本机禁止启动开发 API」的环境约束。
//!
//! 另有少量**源码守卫**断言，用来锁定「两条登录路径复用同一个 handler」这类
//! 结构性不变式（这类不变式无法用运行期断言表达）。

use actix_web::http::header;
use actix_web::test::TestRequest;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};
use uuid::Uuid;

use floatctf::api::extractor::auth::{
    AuthFailure, has_all_access, resolve_permissions, resolve_session_account,
};
use floatctf::core::security::{gen_password_reset_token, gen_session_token};
use floatctf::core::system_ids::PLATFORM_ROLE_SUPER_ADMIN;
use floatctf::entity::sea_orm_active_enums::AccountStatus;
use floatctf::entity::{platform_roles, user_platform_roles, users};

const RBAC_TEST_LOCK_KEY: i64 = 1179204949;

async fn connect_or_skip() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into());
    match sea_orm::Database::connect(&url).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip identity_auth_v2_contract (DB unreachable): {e}");
            None
        }
    }
}

async fn begin_exclusive(db: &DatabaseConnection) -> sea_orm::DatabaseTransaction {
    let txn = db.begin().await.expect("begin exclusive txn");
    txn.execute_unprepared(&format!(
        "SELECT pg_advisory_xact_lock({RBAC_TEST_LOCK_KEY})"
    ))
    .await
    .expect("acquire advisory xact lock");
    txn
}

/// JWT 密钥是进程级 `OnceLock`；测试进程内只装一次即可。
fn ensure_jwt_secret() {
    use floatctf::core::secret::Secret;
    floatctf::core::security::configure_jwt_secret(Secret::new(
        "floatctf-test-secret-for-jwt-wave4",
    ));
}

/// 插入一个测试账号。
async fn insert_account<C: ConnectionTrait>(
    txn: &C,
    status: AccountStatus,
    token_version: i32,
) -> Uuid {
    let id = Uuid::new_v4();
    let uniq = &id.to_string()[..12];
    users::Entity::insert(users::ActiveModel {
        id: Set(id),
        username: Set(format!("wave4-{uniq}")),
        nickname: Set(format!("wave4n-{uniq}")),
        password: Set("$argon2id$v=19$m=19456,t=2,p=1$AA$BB".to_string()),
        email: Set(format!("wave4-{uniq}@example.invalid")),
        avatar: Set(None),
        status: Set(status),
        token_version: Set(token_version),
        ..Default::default()
    })
    .exec(txn)
    .await
    .unwrap_or_else(|e| panic!("insert account: {e}"));
    id
}

fn request_with_bearer(token: &str) -> actix_web::HttpRequest {
    TestRequest::default()
        .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
        .to_http_request()
}

fn request_without_auth() -> actix_web::HttpRequest {
    TestRequest::default().to_http_request()
}

// ============================================================================
// 成功路径
// ============================================================================

#[tokio::test]
async fn active_account_with_matching_version_resolves() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let id = insert_account(&txn, AccountStatus::Active, 3).await;
    let token = gen_session_token(id, 3).expect("issue token");
    let req = request_with_bearer(&token);

    let user = resolve_session_account(&txn, &req)
        .await
        .expect("有效会话令牌必须解析成功");
    assert_eq!(user.id, id);
    assert_eq!(user.token_version, 3);

    txn.rollback().await.expect("rollback");
}

/// 版本无关的其它值也应工作（0 是默认值，最容易写错成「特殊值」）。
#[tokio::test]
async fn token_version_zero_is_valid() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let id = insert_account(&txn, AccountStatus::Active, 0).await;
    let token = gen_session_token(id, 0).expect("issue token");
    assert!(
        resolve_session_account(&txn, &request_with_bearer(&token))
            .await
            .is_ok(),
        "token_version=0 是正常初始值，不得被当成「未设置」而拒绝"
    );

    txn.rollback().await.expect("rollback");
}

// ============================================================================
// 401 矩阵
// ============================================================================

#[tokio::test]
async fn missing_or_malformed_authorization_header_is_401() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    // 完全没有 Authorization 头
    let err = resolve_session_account(&txn, &request_without_auth())
        .await
        .expect_err("缺少凭据必须失败");
    assert_eq!(err, AuthFailure::MissingCredentials);
    assert_eq!(err.status_hint(), 401);

    // scheme 不是 Bearer
    let req = TestRequest::default()
        .insert_header((header::AUTHORIZATION, "Basic YWJjOmRlZg=="))
        .to_http_request();
    assert_eq!(
        resolve_session_account(&txn, &req).await.unwrap_err(),
        AuthFailure::MissingCredentials
    );

    // Bearer 后面是空串
    let req = TestRequest::default()
        .insert_header((header::AUTHORIZATION, "Bearer "))
        .to_http_request();
    assert_eq!(
        resolve_session_account(&txn, &req).await.unwrap_err(),
        AuthFailure::MissingCredentials
    );

    txn.rollback().await.expect("rollback");
}

#[tokio::test]
async fn garbage_and_tampered_tokens_are_401() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    for bad in ["not-a-jwt", "a.b.c", ""] {
        let req = request_with_bearer(bad);
        let err = resolve_session_account(&txn, &req).await.unwrap_err();
        assert!(
            matches!(
                err,
                AuthFailure::MissingCredentials | AuthFailure::InvalidToken
            ),
            "输入 {bad:?} 应产生 401 类失败，实际 {err:?}"
        );
        assert_eq!(err.status_hint(), 401);
    }

    // 签名被篡改
    let id = Uuid::new_v4();
    let mut token = gen_session_token(id, 0).expect("issue");
    token.push('x');
    assert_eq!(
        resolve_session_account(&txn, &request_with_bearer(&token))
            .await
            .unwrap_err(),
        AuthFailure::InvalidToken
    );

    txn.rollback().await.expect("rollback");
}

/// **安全关键**：口令重置令牌不得被当作会话令牌使用。
#[tokio::test]
async fn password_reset_token_is_rejected_as_session_401() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let id = insert_account(&txn, AccountStatus::Active, 0).await;
    let reset = gen_password_reset_token(id, 0).expect("issue reset token");

    let err = resolve_session_account(&txn, &request_with_bearer(&reset))
        .await
        .expect_err("重置令牌绝不能用于业务端点");
    assert_eq!(err, AuthFailure::WrongPurpose);
    assert_eq!(err.status_hint(), 401);

    // 同一账号的会话令牌仍然可用（证明失败原因是用途而不是账号）
    let session = gen_session_token(id, 0).expect("issue session");
    assert!(
        resolve_session_account(&txn, &request_with_bearer(&session))
            .await
            .is_ok()
    );

    txn.rollback().await.expect("rollback");
}

/// **安全关键**：v1 令牌（含 `role`、无 `purpose`）必须失效 → 强制重新登录。
#[tokio::test]
async fn legacy_v1_role_token_is_rejected_401() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let id = insert_account(&txn, AccountStatus::Active, 0).await;
    // 手工构造 v1 结构（无法用本版本的 gen_* 生成）
    let legacy = {
        use chrono::Utc;
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
        let secret = b"floatctf-test-secret-for-jwt-wave4";
        let claims = serde_json::json!({
            "sub": id.to_string(),
            "role": "SuperAdmin",
            "exp": (Utc::now().timestamp() + 3600) as usize,
        });
        encode(
            &Header::new(Algorithm::HS512),
            &claims,
            &EncodingKey::from_secret(secret),
        )
        .expect("encode legacy")
    };

    let err = resolve_session_account(&txn, &request_with_bearer(&legacy))
        .await
        .expect_err("v1 令牌必须失效");
    assert_eq!(err, AuthFailure::InvalidToken);
    assert_eq!(err.status_hint(), 401);

    txn.rollback().await.expect("rollback");
}

/// **安全关键**：`token_version` 不匹配 = 凭据已吊销 → 401。
#[tokio::test]
async fn revoked_credential_by_token_version_is_401() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let id = insert_account(&txn, AccountStatus::Active, 1).await;

    // 用旧版本签发（模拟改密前签发的令牌）
    let stale = gen_session_token(id, 0).expect("issue stale");
    let err = resolve_session_account(&txn, &request_with_bearer(&stale))
        .await
        .expect_err("旧版本令牌必须失效");
    assert_eq!(err, AuthFailure::RevokedCredential);
    assert_eq!(err.status_hint(), 401);

    // 用新版本签发 → 可用
    let fresh = gen_session_token(id, 1).expect("issue fresh");
    assert!(
        resolve_session_account(&txn, &request_with_bearer(&fresh))
            .await
            .is_ok()
    );

    txn.rollback().await.expect("rollback");
}

#[tokio::test]
async fn unknown_account_is_401() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    // 令牌合法但账号不存在
    let token = gen_session_token(Uuid::new_v4(), 0).expect("issue");
    let err = resolve_session_account(&txn, &request_with_bearer(&token))
        .await
        .expect_err("账号不存在必须失败");
    assert_eq!(err, AuthFailure::UnknownAccount);
    assert_eq!(err.status_hint(), 401);

    txn.rollback().await.expect("rollback");
}

// ============================================================================
// 403：已认证但账号被禁用
// ============================================================================

/// **401 / 403 分流的关键用例**：凭据有效但账号被禁用 → **403**。
///
/// 这样客户端不会去清令牌跳登录页（那解决不了问题），而是提示「账号已被禁用」。
#[tokio::test]
async fn disabled_account_is_403_not_401() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let id = insert_account(&txn, AccountStatus::Disabled, 0).await;
    let token = gen_session_token(id, 0).expect("issue token");

    let err = resolve_session_account(&txn, &request_with_bearer(&token))
        .await
        .expect_err("禁用账号必须被拒绝");
    assert_eq!(err, AuthFailure::AccountDisabled);
    assert_eq!(
        err.status_hint(),
        403,
        "账号被禁用是 403（已认证但不允许），不是 401"
    );

    txn.rollback().await.expect("rollback");
}

/// 只有 `AccountDisabled` 是 403，其余全部 401（契约穷尽性）。
#[tokio::test]
async fn only_account_disabled_maps_to_403() {
    for failure in [
        AuthFailure::MissingCredentials,
        AuthFailure::InvalidToken,
        AuthFailure::WrongPurpose,
        AuthFailure::UnknownAccount,
        AuthFailure::RevokedCredential,
    ] {
        assert_eq!(
            failure.status_hint(),
            401,
            "{failure:?} 必须是 401（调用方应重新认证）"
        );
    }
    assert_eq!(AuthFailure::AccountDisabled.status_hint(), 403);
}

/// 禁用**立刻生效**：同一令牌在账号被禁用后立即失效（v1 无法做到）。
#[tokio::test]
async fn disabling_takes_effect_immediately_without_reissue() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let id = insert_account(&txn, AccountStatus::Active, 0).await;
    let token = gen_session_token(id, 0).expect("issue token");
    assert!(
        resolve_session_account(&txn, &request_with_bearer(&token))
            .await
            .is_ok()
    );

    // 管理端禁用该账号（**不**重新签发令牌）。
    // 用原生 SQL：sea-orm 的 col_expr 会把枚举绑成 text，与 account_status 类型不符。
    txn.execute_unprepared(&format!(
        "UPDATE users SET status = 'disabled' WHERE id = '{id}'"
    ))
    .await
    .expect("disable account");

    assert_eq!(
        resolve_session_account(&txn, &request_with_bearer(&token))
            .await
            .unwrap_err(),
        AuthFailure::AccountDisabled,
        "禁用必须在下一次请求即刻生效"
    );

    txn.rollback().await.expect("rollback");
}

// ============================================================================
// 全权判定（Wave 4 过渡判据）
// ============================================================================

#[tokio::test]
async fn has_all_access_tracks_role_grants_from_db() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    // 确保内置角色存在（迁移已插入；这里幂等兜底）
    floatctf::modules::identity::authorization::ensure_builtin_roles(&txn).await;

    let plain = insert_account(&txn, AccountStatus::Active, 0).await;
    assert!(
        !has_all_access(&txn, plain).await.expect("check"),
        "无任何角色授予的账号不得有管理能力"
    );

    // 授予内置 super-admin 角色
    user_platform_roles::Entity::insert(user_platform_roles::ActiveModel {
        user_id: Set(plain),
        role_id: Set(PLATFORM_ROLE_SUPER_ADMIN),
        granted_by: Set(None),
        ..Default::default()
    })
    .exec(&txn)
    .await
    .expect("grant super-admin");

    assert!(
        has_all_access(&txn, plain).await.expect("check"),
        "持有内置 super-admin 角色的账号必须有管理能力"
    );

    // 撤销后立即失效（无需重新登录）
    user_platform_roles::Entity::delete_many()
        .filter(user_platform_roles::Column::UserId.eq(plain))
        .exec(&txn)
        .await
        .expect("revoke");
    assert!(
        !has_all_access(&txn, plain).await.expect("check"),
        "★ 撤销角色必须在**下一次请求**即刻生效（v1 需要等令牌过期）"
    );

    txn.rollback().await.expect("rollback");
}

/// 自定义 `grants_all` 角色同样获得全权（不只看内置角色 ID）。
#[tokio::test]
async fn has_all_access_honours_grants_all_flag() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let user_id = insert_account(&txn, AccountStatus::Active, 0).await;
    let role_id = Uuid::new_v4();
    platform_roles::Entity::insert(platform_roles::ActiveModel {
        id: Set(role_id),
        key: Set(format!("wave4-grants-all-{}", &role_id.to_string()[..8])),
        name: Set("Grants All".into()),
        description: Set(String::new()),
        protected: Set(false),
        grants_all: Set(true),
        ..Default::default()
    })
    .exec(&txn)
    .await
    .expect("insert grants_all role");
    user_platform_roles::Entity::insert(user_platform_roles::ActiveModel {
        user_id: Set(user_id),
        role_id: Set(role_id),
        granted_by: Set(None),
        ..Default::default()
    })
    .exec(&txn)
    .await
    .expect("grant");

    assert!(has_all_access(&txn, user_id).await.expect("check"));

    txn.rollback().await.expect("rollback");
}

/// `resolve_permissions` 必须是「多角色并集」，且 `grants_all` 短路为全集。
#[tokio::test]
async fn resolve_permissions_unions_roles_and_short_circuits() {
    ensure_jwt_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let user_id = insert_account(&txn, AccountStatus::Active, 0).await;
    let role_id = Uuid::new_v4();
    platform_roles::Entity::insert(platform_roles::ActiveModel {
        id: Set(role_id),
        key: Set(format!("wave4-perm-{}", &role_id.to_string()[..8])),
        name: Set("Perm".into()),
        description: Set(String::new()),
        protected: Set(false),
        grants_all: Set(false),
        ..Default::default()
    })
    .exec(&txn)
    .await
    .expect("insert role");
    floatctf::entity::platform_role_permissions::Entity::insert(
        floatctf::entity::platform_role_permissions::ActiveModel {
            role_id: Set(role_id),
            permission: Set("events.read".into()),
        },
    )
    .exec(&txn)
    .await
    .expect("grant permission");
    user_platform_roles::Entity::insert(user_platform_roles::ActiveModel {
        user_id: Set(user_id),
        role_id: Set(role_id),
        granted_by: Set(None),
        ..Default::default()
    })
    .exec(&txn)
    .await
    .expect("assign role");

    let perms = resolve_permissions(&txn, user_id).await.expect("resolve");
    assert_eq!(perms.len(), 1);
    assert!(perms.contains(floatctf::core::permission::Permission::EventsRead));

    // 空账号 → 空权限（fail closed）
    let empty_user = insert_account(&txn, AccountStatus::Active, 0).await;
    let none = resolve_permissions(&txn, empty_user)
        .await
        .expect("resolve");
    assert!(none.is_empty(), "无角色账号的有效权限必须是空集");

    // grants_all 短路
    floatctf::modules::identity::authorization::ensure_builtin_roles(&txn).await;
    user_platform_roles::Entity::insert(user_platform_roles::ActiveModel {
        user_id: Set(empty_user),
        role_id: Set(PLATFORM_ROLE_SUPER_ADMIN),
        granted_by: Set(None),
        ..Default::default()
    })
    .exec(&txn)
    .await
    .expect("grant super-admin");
    let all = resolve_permissions(&txn, empty_user)
        .await
        .expect("resolve");
    assert_eq!(
        all.len(),
        floatctf::core::permission::Permission::ALL.len(),
        "grants_all 必须短路为全部权限"
    );

    txn.rollback().await.expect("rollback");
}

// ============================================================================
// 源码守卫：结构性不变式
// ============================================================================

/// `/api/session` 与 `/api/users/session` 必须注册**同一个 handler**。
///
/// 这是「登录路径不可能行为分叉」的结构保证：如果将来有人复制出一个
/// `unified_login`，这条断言会红。
#[test]
fn only_unified_session_route_remains() {
    let module = include_str!("../src/modules/identity/mod.rs");
    assert!(
        module.contains("pub fn configure_unified_session_routes"),
        "必须存在统一会话路由注册函数"
    );
    // 两个注册函数都只引用 authentication::user_login
    let unified = module
        .split("pub fn configure_unified_session_routes")
        .nth(1)
        .expect("unified fn body")
        .split("pub fn configure_session_routes")
        .next()
        .expect("unified body end");
    assert!(
        unified.contains("authentication::user_login"),
        "/api/session 必须复用 authentication::user_login"
    );
    // Wave 10：`/users/session` 与 `/admin/session` 两个别名已删除，
    // 因此选手路由里**不应**再注册登录端点；只剩 `/api/session`。
    let player = module
        .split("pub fn configure_player_routes")
        .nth(1)
        .expect("player fn body")
        .split("pub fn configure_unified_session_routes")
        .next()
        .expect("player body end");
    let player_code: String = player
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !player_code.contains("user_login"),
        "Wave 10 后 /api/users/session 别名必须已删除（/api/session 是唯一登录入口）"
    );
    assert!(
        !module.contains("configure_session_routes"),
        "Wave 10 后 /api/admin/session 的注册函数必须已删除"
    );
    assert!(
        !module.contains("unified_login"),
        "不得另写一个统一的登录 handler（会与既有路径分叉）"
    );
}

/// bootstrap 必须真的把 `/api/session` 挂到 `/api` 下。
#[test]
fn bootstrap_registers_unified_session_route() {
    let routes = include_str!("../src/bootstrap/routes.rs");
    assert!(
        routes.contains("configure_unified_session_routes"),
        "bootstrap 必须注册 configure_unified_session_routes"
    );
}

/// JWT 不得再携带任何授权信息（角色/权限）。
#[test]
fn auth_claims_carry_identity_only() {
    let jwt = include_str!("../src/core/security/jwt.rs");
    let claims_start = jwt
        .split("pub struct AuthClaims {")
        .nth(1)
        .expect("AuthClaims 定义")
        .split('}')
        .next()
        .expect("AuthClaims 字段");
    for forbidden in ["role", "permission", "is_admin", "super_admin"] {
        assert!(
            !claims_start.to_lowercase().contains(forbidden),
            "AuthClaims 不得包含授权字段 {forbidden:?} —— 授权必须每请求从 DB 解析"
        );
    }
    for required in ["sub", "purpose", "token_version", "exp"] {
        assert!(
            claims_start.contains(required),
            "AuthClaims 必须包含 {required}"
        );
    }
}

/// 旧的 `Role` 枚举必须已从认证层消失。
///
/// 只在**非注释行**里断言：文件头的设计说明会提到 `Role { … AwdJudger }`，
/// 直接对全文 `contains` 会匹配到自己的说明文字（曾踩过这个坑）。
#[test]
fn legacy_role_enum_is_gone() {
    let jwt = include_str!("../src/core/security/jwt.rs");
    let code: String = jwt
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !code.contains("pub enum Role"),
        "v1 的 Role 枚举必须删除（授权信息不得进入令牌）"
    );
    assert!(
        !code.contains("AwdJudger"),
        "AwdJudger 角色必须删除（它同时是赛制判题身份，v2 用权限替代）"
    );
    assert!(code.contains("pub enum TokenPurpose"));
}

/// 登录与重置都不得再使用 `Role::`。
#[test]
fn login_paths_do_not_reference_legacy_roles() {
    for (label, src) in [(
        "authentication",
        include_str!("../src/modules/identity/authentication/mod.rs"),
    )] {
        assert!(
            !src.contains("Role::SuperAdmin")
                && !src.contains("Role::User")
                && !src.contains("Role::ResetAccount"),
            "{label} 不得再引用 v1 的 Role 变体"
        );
    }
}

/// 口令重置必须**同时**校验用途与版本，并在成功后自增 `token_version`。
#[test]
fn password_reset_validates_purpose_and_revokes_sessions() {
    let src = include_str!("../src/modules/identity/authentication/mod.rs");
    assert!(
        src.contains("claim.purpose != TokenPurpose::PasswordReset"),
        "重置必须校验用途"
    );
    assert!(
        src.contains("user.token_version != claim.token_version"),
        "重置必须校验凭据版本"
    );
    assert!(
        src.contains("m_user.token_version = Set(next_token_version)"),
        "改密必须自增 token_version 以吊销全部旧会话"
    );
}

/// 禁用账号必须被登录拒绝（403），而不是签发令牌。
#[test]
fn disabled_account_cannot_log_in() {
    let src = include_str!("../src/modules/identity/authentication/mod.rs");
    assert!(
        src.contains("user.status != AccountStatus::Active"),
        "登录必须检查账号状态"
    );
    // Wave 10：`/api/admin/session` 与整个 administrator 模块已删除，
    // 「能不能进管理端」不再由登录端点回答（认证与授权分离）：
    // 登录只回答"你是谁"（上面已断言），授权由每个管理端点上的
    // `RequirePermission<P>` 回答。因此这里改为断言**旧路径确已消失**。
    assert!(
        !include_str!("../src/modules/identity/mod.rs").contains("administrator"),
        "Wave 10 后 identity 模块不得再引用 administrator（超管入口已删除）"
    );
    let routes = include_str!("../src/bootstrap/routes.rs");
    assert!(
        !routes.contains("configure_session_routes") && !routes.contains("super_admin"),
        "Wave 10 后不得再注册 /api/admin/session 或 /super_admin"
    );
}

/// **Wave 10 终态**：`super_admin` 的整套 legacy 面已删除。
///
/// 这条断言取代了 Wave 3 的「CRUD 必须镜像到统一身份表」—— 那个不变式在
/// 过渡期是必要的（两个表各有一行会造成"无法登录的管理员"），
/// 但它之所以必要，正是因为存在第二张表。表删除后，镜像问题从根上消失，
/// 断言也随之升级为「它们确实都不在了」。
#[test]
fn legacy_super_admin_surface_is_gone() {
    let identity = include_str!("../src/modules/identity/mod.rs");
    assert!(
        !identity.contains("administrator"),
        "identity 模块不得再引用 administrator"
    );
    assert!(
        !identity.contains("super_admin"),
        "identity 模块不得再注册 /super_admin 路由"
    );
    // 只看**非注释行**：模块文档会解释「两个别名已在 Wave 10 删除」，
    // 直接对全文 contains 会匹配到自己的说明文字（这是本会话第三次踩同一个坑）。
    let identity_code: String = identity
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !identity_code.contains("/users/session"),
        "`/users/session` 过渡别名必须已删除（/api/session 是唯一入口）"
    );

    // 实体目录里也不应再有 super_admin（M6 + db:gen 之后）
    assert!(
        !std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/entity/super_admin.rs"
        ))
        .exists(),
        "M6 之后生成实体 super_admin.rs 必须消失（db:gen 的兜底作用）"
    );
}
