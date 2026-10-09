//! Wave 5：授权 v2 契约测试（DB-gated）。
//!
//! 覆盖：
//! 1. [`RequirePermission<P>`] 的 401 / 403 / 200 三态（进程内 `init_service`，
//!    不绑定端口，因此在本机也**真实执行**而不是软跳过）；
//! 2. 反提权的完整矩阵（before ⊆ / after ⊆ / `grants_all` 目标保护 / 自锁死）；
//! 3. 权限目录 API 的形状（29 条、域、高危标记）。

use actix_web::{App, HttpResponse, test as actix_test, web};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};
use uuid::Uuid;

use floatctf::api::extractor::auth::{RequirePermission, resolve_auth_context};
use floatctf::core::permission::guards::{EventsReadGuard, RolesManageGuard, UsersReadGuard};
use floatctf::core::permission::{Permission, PermissionSet};
use floatctf::core::security::gen_session_token;
use floatctf::core::system_ids::PLATFORM_ROLE_SUPER_ADMIN;
use floatctf::entity::sea_orm_active_enums::AccountStatus;
use floatctf::entity::{platform_role_permissions, platform_roles, user_platform_roles, users};
use floatctf::modules::identity::authorization::ensure_builtin_roles;
use floatctf::modules::identity::roles::service::{
    EscalationGuard, simulated_effective_permissions,
};

const RBAC_TEST_LOCK_KEY: i64 = 1179204949;

async fn connect_or_skip() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into());
    match sea_orm::Database::connect(&url).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip identity_authorization_v2_contract (DB unreachable): {e}");
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

fn install_secret() {
    use floatctf::core::secret::Secret;
    floatctf::core::security::configure_jwt_secret(Secret::new(
        "floatctf-test-secret-for-jwt-wave5",
    ));
}

async fn insert_account<C: ConnectionTrait>(db: &C, label: &str) -> Uuid {
    let id = Uuid::new_v4();
    let uniq = &id.to_string()[..12];
    users::Entity::insert(users::ActiveModel {
        id: Set(id),
        username: Set(format!("{label}-{uniq}")),
        nickname: Set(format!("{label}n-{uniq}")),
        password: Set("x".into()),
        email: Set(format!("{label}-{uniq}@example.invalid")),
        avatar: Set(None),
        status: Set(AccountStatus::Active),
        token_version: Set(0),
        ..Default::default()
    })
    .exec(db)
    .await
    .unwrap_or_else(|e| panic!("insert {label}: {e}"));
    id
}

/// 建一个自定义角色并授予权限。
async fn insert_role<C: ConnectionTrait>(
    db: &C,
    label: &str,
    permissions: &[Permission],
    grants_all: bool,
) -> Uuid {
    let id = Uuid::new_v4();
    let uniq = &id.to_string()[..8];
    platform_roles::Entity::insert(platform_roles::ActiveModel {
        id: Set(id),
        key: Set(format!("{label}-{uniq}")),
        name: Set(label.to_string()),
        description: Set(String::new()),
        protected: Set(false),
        grants_all: Set(grants_all),
        ..Default::default()
    })
    .exec(db)
    .await
    .unwrap_or_else(|e| panic!("insert role {label}: {e}"));

    for permission in permissions {
        platform_role_permissions::Entity::insert(platform_role_permissions::ActiveModel {
            role_id: Set(id),
            permission: Set(permission.as_str().to_string()),
        })
        .exec(db)
        .await
        .unwrap_or_else(|e| panic!("grant {}: {e}", permission.as_str()));
    }
    id
}

async fn assign_role<C: ConnectionTrait>(db: &C, user_id: Uuid, role_id: Uuid) {
    user_platform_roles::Entity::insert(user_platform_roles::ActiveModel {
        user_id: Set(user_id),
        role_id: Set(role_id),
        granted_by: Set(None),
        ..Default::default()
    })
    .exec(db)
    .await
    .expect("assign role");
}

// ============================================================================
// RequirePermission<P> 三态
// ============================================================================

async fn guard_events_read(_: RequirePermission<EventsReadGuard>) -> HttpResponse {
    HttpResponse::Ok().finish()
}

async fn guard_roles_manage(_: RequirePermission<RolesManageGuard>) -> HttpResponse {
    HttpResponse::Ok().finish()
}

/// **核心契约**：无令牌 401 / 有令牌无权限 403 / 有权限 200。
#[actix_web::test]
async fn require_permission_three_state_contract() {
    install_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // 前置：内置角色存在
    {
        let txn = begin_exclusive(&db).await;
        ensure_builtin_roles(&txn).await;
        txn.commit().await.expect("commit prep");
    }

    // 三个账号：无权限 / 有 events.read / 全权
    let nobody = insert_account(&db, "perm-nobody").await;
    let reader = insert_account(&db, "perm-reader").await;
    let root = insert_account(&db, "perm-root").await;

    let reader_role = insert_role(&db, "reader", &[Permission::EventsRead], false).await;
    assign_role(&db, reader, reader_role).await;
    assign_role(&db, root, PLATFORM_ROLE_SUPER_ADMIN).await;

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(db.clone()))
            .route("/events", web::get().to(guard_events_read))
            .route("/roles", web::get().to(guard_roles_manage)),
    )
    .await;

    let call = |token: Option<String>, path: &'static str| {
        let app = &app;
        async move {
            let mut req = actix_test::TestRequest::get().uri(path);
            if let Some(token) = token {
                req = req.insert_header(("Authorization", format!("Bearer {token}")));
            }
            actix_test::call_service(app, req.to_request())
                .await
                .status()
                .as_u16()
        }
    };

    // ① 无令牌 → 401
    assert_eq!(call(None, "/events").await, 401, "无令牌必须 401");

    // ② 有令牌但无所需权限 → 403（不是 401：重新登录解决不了）
    assert_eq!(
        call(Some(gen_session_token(nobody, 0).unwrap()), "/events").await,
        403,
        "已认证但缺权限必须 403"
    );

    // ③ 有权限 → 200
    assert_eq!(
        call(Some(gen_session_token(reader, 0).unwrap()), "/events").await,
        200,
        "持有 events.read 必须放行"
    );

    // ④ 权限是**逐条**判定的：有 events.read 不等于有 roles.manage
    assert_eq!(
        call(Some(gen_session_token(reader, 0).unwrap()), "/roles").await,
        403,
        "events.read 不得放行 roles.manage"
    );

    // ⑤ 全权角色短路 → 两条都放行（含将来新增的权限）
    assert_eq!(
        call(Some(gen_session_token(root, 0).unwrap()), "/events").await,
        200
    );
    assert_eq!(
        call(Some(gen_session_token(root, 0).unwrap()), "/roles").await,
        200
    );

    // 清理
    for id in [nobody, reader, root] {
        let _ = users::Entity::delete_by_id(id).exec(&db).await;
    }
    let _ = platform_roles::Entity::delete_by_id(reader_role)
        .exec(&db)
        .await;
}

/// 重置令牌不得通过这些端点（用途不符 → 401，而不是 403）。
#[actix_web::test]
async fn reset_token_is_401_on_permission_guarded_endpoints() {
    install_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let user = insert_account(&db, "perm-reset").await;

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(db.clone()))
            .route("/events", web::get().to(guard_events_read)),
    )
    .await;

    let token = floatctf::core::security::gen_password_reset_token(user, 0).unwrap();
    let req = actix_test::TestRequest::get()
        .uri("/events")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, req).await.status().as_u16(),
        401,
        "重置令牌是**用途不符**（401），不是「已认证但缺权限」（403）"
    );

    let _ = users::Entity::delete_by_id(user).exec(&db).await;
}

/// 撤销角色后，同一令牌立即 403 —— 权限每请求从 DB 解析的直接后果。
#[actix_web::test]
async fn revoking_role_takes_effect_on_the_next_request() {
    install_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let user = insert_account(&db, "perm-revoke").await;
    let role = insert_role(&db, "revoke-me", &[Permission::EventsRead], false).await;
    assign_role(&db, user, role).await;

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(db.clone()))
            .route("/events", web::get().to(guard_events_read)),
    )
    .await;

    let token = gen_session_token(user, 0).unwrap();
    let call = || {
        let app = &app;
        let token = token.clone();
        async move {
            let req = actix_test::TestRequest::get()
                .uri("/events")
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            actix_test::call_service(app, req).await.status().as_u16()
        }
    };

    assert_eq!(call().await, 200);

    // 撤销角色（**不**重新签发令牌）
    user_platform_roles::Entity::delete_many()
        .filter(user_platform_roles::Column::UserId.eq(user))
        .exec(&db)
        .await
        .expect("revoke role");

    assert_eq!(
        call().await,
        403,
        "★ 撤销角色必须在**同一令牌**的下一请求即刻生效"
    );

    let _ = users::Entity::delete_by_id(user).exec(&db).await;
    let _ = platform_roles::Entity::delete_by_id(role).exec(&db).await;
}

// ============================================================================
// 反提权（DB 解析出的权限 → EscalationGuard）
// ============================================================================

/// 用真实 DB 权限验证反提权的端到端语义（不只是纯函数单测）。
#[tokio::test]
async fn escalation_guard_uses_db_resolved_permissions() {
    install_secret();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;
    ensure_builtin_roles(&txn).await;

    // actor：只有 roles.manage + users.read
    let actor_id = insert_account(&txn, "esc-actor").await;
    let actor_role = insert_role(
        &txn,
        "esc-actor-role",
        &[Permission::RolesManage, Permission::UsersRead],
        false,
    )
    .await;
    assign_role(&txn, actor_id, actor_role).await;

    // 目标：持有一个权限超出 actor 的角色
    let target_id = insert_account(&txn, "esc-target").await;
    let over_role = insert_role(&txn, "esc-over", &[Permission::TerminalOpen], false).await;
    assign_role(&txn, target_id, over_role).await;

    let actor_user = users::Entity::find_by_id(actor_id)
        .one(&txn)
        .await
        .expect("query")
        .expect("actor row");
    let actor_ctx = resolve_auth_context(&txn, actor_user)
        .await
        .expect("resolve actor ctx");
    assert!(!actor_ctx.grants_all);
    assert!(actor_ctx.has(Permission::RolesManage));
    assert!(!actor_ctx.has(Permission::TerminalOpen));

    let guard = EscalationGuard::new(&actor_ctx);

    // 不能把目标改成「更宽」的集合
    let wider = PermissionSet::from_permissions([Permission::TerminalOpen]);
    assert!(
        guard
            .check_target_permissions(&wider, &PermissionSet::new())
            .is_err(),
        "目标**变更前**已超出 actor 权限时必须拒绝（before ⊆ 检查）"
    );

    // 也不能给自己授予自己没有的权限
    assert!(
        guard
            .check_role_permissions(&PermissionSet::new(), &wider)
            .is_err(),
        "after ⊆ 检查必须拒绝超出自身权限的授予"
    );

    // 在自身范围内的编辑是允许的
    let within = PermissionSet::from_permissions([Permission::UsersRead]);
    assert!(
        guard
            .check_role_permissions(&PermissionSet::new(), &within)
            .is_ok()
    );

    // 目标集合包含全权角色时，模拟权限必须是全集（并因此被拒）
    let mut root_role_ids = Vec::new();
    root_role_ids.push(
        platform_roles::Entity::find_by_id(PLATFORM_ROLE_SUPER_ADMIN)
            .one(&txn)
            .await
            .expect("query")
            .expect("builtin role"),
    );
    let simulated = simulated_effective_permissions(&txn, &root_role_ids)
        .await
        .expect("simulate");
    assert_eq!(
        simulated.len(),
        Permission::ALL.len(),
        "授予全权角色时模拟权限必须是全集"
    );
    assert!(
        guard
            .check_target_permissions(&PermissionSet::new(), &simulated)
            .is_err(),
        "非全权 actor 不得把目标提升为全权"
    );

    txn.rollback().await.expect("rollback");
}

/// 权限目录必须恰好 29 条、域非空、高危 5 条（与 `core::permission` 一致）。
#[tokio::test]
async fn permission_catalog_shape_is_pinned() {
    use floatctf::core::permission::PermissionDomain;

    assert_eq!(Permission::ALL.len(), 29, "权限目录规模基线");
    let high_risk: Vec<&str> = Permission::ALL
        .iter()
        .filter(|p| p.high_risk())
        .map(|p| p.as_str())
        .collect();
    assert_eq!(
        high_risk,
        vec![
            "roles.manage",
            "settings.manage",
            "docker.manage",
            "database.execute",
            "terminal.open",
        ]
    );
    // 每个域至少一条权限，且每条权限的域在 ALL 里
    for permission in Permission::ALL {
        assert!(
            PermissionDomain::ALL.contains(&permission.domain()),
            "{} 的域必须属于 PermissionDomain::ALL",
            permission.as_str()
        );
    }
    for domain in PermissionDomain::ALL {
        assert!(
            Permission::ALL.iter().any(|p| p.domain() == *domain),
            "域 {domain} 必须至少有一条权限"
        );
    }
}

// ============================================================================
// 角色管理 API 的实现期契约
// ============================================================================
//
// 这些 handler 依赖 `ReqCtx`（含 `WebDocker` / `WebRustfs`），因此**无法**在进程内
// `init_service` 里构造 —— 那需要真实的 Docker 与 RustFS 连接。本机也禁止启动
// 开发 API（会与生产争抢宿主资源），所以它们的 HTTP 级断言属于 Wave 11 的
// live 测试，在本环境会如实记为 **SKIP**，而不是 PASS。
//
// 这里用**源码守卫**锁定那些无法用运行期断言表达、且一旦改错就会静默放开授权的
// 结构性决定，并用 DB 断言锁定 DTO 的语义契约。

/// protected 角色的**结构性**变更必须是 409 Conflict，不能是 403。
///
/// 403 会让前端显示「你没有权限」，但真实原因是「这个内置角色不能这样改」——
/// 两者的用户引导完全不同。这条断言防止未来有人「顺手」改回 Forbidden。
#[test]
fn protected_role_mutation_is_conflict_not_forbidden() {
    let src = include_str!("../src/modules/identity/roles/mod.rs");

    let protected_permissions_branch = src
        .split("内置角色的权限集不可修改")
        .next()
        .expect("anchor");
    assert!(
        protected_permissions_branch
            .rsplit("return Err(")
            .next()
            .expect("branch")
            .contains("AppError::Conflict"),
        "protected 角色的权限集变更必须是 409 Conflict"
    );

    let protected_delete_branch = src.split("系统内置角色不可删除").next().expect("anchor");
    assert!(
        protected_delete_branch
            .rsplit("return Err(")
            .next()
            .expect("branch")
            .contains("AppError::Conflict"),
        "protected 角色删除必须是 409 Conflict"
    );

    // 反向断言：这两处不得再用 Forbidden
    for message in ["内置角色的权限集不可修改", "系统内置角色不可删除"] {
        let before = src.split(message).next().expect("anchor");
        let snippet = before.rsplit("return Err(").next().expect("branch");
        assert!(
            !snippet.contains("AppError::Forbidden"),
            "{message} 不得使用 403：语义是「与内置不变式冲突」而非「你没权限」"
        );
    }
}

/// 账号角色读取必须用 `users.read` —— 与 `users.read` 的权限说明
/// 「查看账号列表、详情**与其角色**」保持一致；用 `roles.read` 会自相矛盾。
#[test]
fn user_roles_read_uses_users_read() {
    let src = include_str!("../src/modules/identity/roles/mod.rs");
    let handler = src
        .split("pub async fn get_user_roles(")
        .nth(1)
        .expect("get_user_roles")
        .split("pub async fn set_user_roles(")
        .next()
        .expect("body");
    assert!(
        handler.contains("RequirePermission<UsersReadGuard>"),
        "get_user_roles 必须用 users.read"
    );
    assert!(
        !handler.contains("RequirePermission<RolesReadGuard>"),
        "get_user_roles 不得用 roles.read（与权限说明冲突）"
    );
}

/// 角色详情端点必须存在、按 `RolesReadGuard` 守卫、且真的被注册。
#[test]
fn role_detail_endpoint_is_registered() {
    let src = include_str!("../src/modules/identity/roles/mod.rs");
    let handler = src
        .split("pub async fn get_role(")
        .nth(1)
        .expect("get_role 必须存在（platform-permissions.md 定义了该端点）");
    assert!(
        handler.contains("RequirePermission<RolesReadGuard>"),
        "角色详情必须按 roles.read 守卫"
    );
    assert!(
        src.contains("cfg.service(get_role);"),
        "get_role 必须被 configure_role_routes 注册"
    );
}

/// 角色编辑的三个变更端点都必须按 `roles.manage` 守卫（不能只读权限就放行写入）。
#[test]
fn role_mutations_require_roles_manage() {
    let src = include_str!("../src/modules/identity/roles/mod.rs");
    for handler in ["create_role", "patch_role", "delete_role", "set_user_roles"] {
        let body = src
            .split(&format!("pub async fn {handler}("))
            .nth(1)
            .unwrap_or_else(|| panic!("{handler} 必须存在"))
            .split("\n}")
            .next()
            .expect("body");
        assert!(
            body.contains("RequirePermission<RolesManageGuard>"),
            "{handler} 必须按 roles.manage 守卫"
        );
    }
}

/// `grants_all` 角色**不枚举**权限（它的权限来自短路，不是行集合）。
///
/// 这是 DTO 的语义契约：如果 `permissions` 里列出了 29 条，UI 会误以为
/// 「这个角色的权限是这 29 条」，从而在将来新增权限时显示错误的权限集。
#[tokio::test]
async fn grants_all_role_reports_no_explicit_permissions() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;
    ensure_builtin_roles(&txn).await;

    let role = platform_roles::Entity::find_by_id(PLATFORM_ROLE_SUPER_ADMIN)
        .one(&txn)
        .await
        .expect("query")
        .expect("builtin role");
    assert!(role.grants_all, "内置 super-admin 必须是 grants_all");

    let explicit = platform_role_permissions::Entity::find()
        .filter(platform_role_permissions::Column::RoleId.eq(role.id))
        .all(&txn)
        .await
        .expect("query permissions");
    assert!(
        explicit.is_empty(),
        "★ grants_all 角色不得依赖权限行（否则新增权限时会漏掉、超管静默失去能力）"
    );

    // 而它的**有效权限**必须是全集
    let (set, _) =
        floatctf::modules::identity::authorization::permissions_of_roles(&txn, &[role.id])
            .await
            .expect("resolve");
    assert_eq!(
        set.len(),
        Permission::ALL.len(),
        "grants_all 必须短路为全集"
    );

    txn.rollback().await.expect("rollback");
}

/// 内置角色**恰好一个**，且其余四个域没有内置角色（其余为可编辑模板）。
#[tokio::test]
async fn only_super_admin_role_is_builtin() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;
    ensure_builtin_roles(&txn).await;

    let protected: Vec<String> = platform_roles::Entity::find()
        .filter(platform_roles::Column::Protected.eq(true))
        .all(&txn)
        .await
        .expect("query")
        .into_iter()
        .map(|r| r.key)
        .collect();
    assert_eq!(
        protected,
        vec!["super-admin".to_string()],
        "第一版只内置 super-admin（修订 1）：其余角色是 UI 模板，不 seed、不 protected"
    );

    txn.rollback().await.expect("rollback");
}
