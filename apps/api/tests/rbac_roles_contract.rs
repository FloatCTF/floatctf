//! Wave 1：内置角色 reconcile 与权限解析（DB-gated）。
//!
//! 验证 [`ensure_builtin_roles`] 的幂等性、结构字段校正、orphan 权限**只报告不删除**，
//! 以及 [`permissions_of_roles`] 的并集 / `grants_all` 短路语义。
//!
//! cargo 为每个测试文件起独立进程，这些用例都会写 `platform_roles`，
//! 因此用 Postgres 事务级 advisory lock 串行（与 `migrate.sh` 同一思路），
//! 并在事务结束回滚，绝不污染数据库。

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};
use uuid::Uuid;

use floatctf::core::permission::{Permission, PermissionSet};
use floatctf::core::system_ids::{PLATFORM_ROLE_SUPER_ADMIN, PLATFORM_ROLE_SUPER_ADMIN_KEY};
use floatctf::entity::{platform_role_permissions, platform_roles};
use floatctf::modules::identity::authorization::{
    BUILTIN_ROLES, ensure_builtin_roles, permissions_of_roles,
};

const RBAC_TEST_LOCK_KEY: i64 = 1179204949;

async fn connect_or_skip() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into());
    match sea_orm::Database::connect(&url).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip rbac_roles_contract (DB unreachable): {e}");
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

/// 插入一个自定义角色（事务结束随回滚消失）。
async fn insert_role<C: ConnectionTrait>(txn: &C, key: &str) -> Uuid {
    let id = Uuid::new_v4();
    platform_roles::Entity::insert(platform_roles::ActiveModel {
        id: Set(id),
        key: Set(key.to_string()),
        name: Set("Test Role".into()),
        description: Set(String::new()),
        protected: Set(false),
        grants_all: Set(false),
        ..Default::default()
    })
    .exec(txn)
    .await
    .unwrap_or_else(|e| panic!("insert role {key}: {e}"));
    id
}

async fn grant<C: ConnectionTrait>(txn: &C, role_id: Uuid, permission: Permission) {
    platform_role_permissions::Entity::insert(platform_role_permissions::ActiveModel {
        role_id: Set(role_id),
        permission: Set(permission.as_str().to_string()),
    })
    .exec(txn)
    .await
    .unwrap_or_else(|e| panic!("grant {permission}: {e}"));
}

// ============================================================================
// 内置 super-admin
// ============================================================================

#[tokio::test]
async fn ensure_builtin_roles_creates_super_admin_idempotently() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let first = ensure_builtin_roles(&txn).await;
    assert!(
        first.conflicts.is_empty(),
        "reconcile 不应有冲突: {:?}",
        first.conflicts
    );

    let role = platform_roles::Entity::find_by_id(PLATFORM_ROLE_SUPER_ADMIN)
        .one(&txn)
        .await
        .expect("query")
        .expect("super-admin 角色必须存在");
    assert_eq!(role.key, PLATFORM_ROLE_SUPER_ADMIN_KEY);
    assert!(role.protected, "super-admin 必须 protected=true");
    assert!(role.grants_all, "super-admin 必须 grants_all=true");

    // 幂等：第二次不应再创建/修正
    let second = ensure_builtin_roles(&txn).await;
    assert!(
        second.created.is_empty(),
        "第二次不应再创建: {:?}",
        second.created
    );
    assert!(
        second.repaired.is_empty(),
        "第二次不应再修正: {:?}",
        second.repaired
    );
    assert!(second.conflicts.is_empty(), "{:?}", second.conflicts);

    txn.rollback().await.expect("rollback");
}

#[tokio::test]
async fn ensure_builtin_roles_repairs_structural_drift_without_touching_name() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let _ = ensure_builtin_roles(&txn).await;

    // 人为破坏结构字段 + 改展示名（模拟手工改库 / 误操作）
    use sea_orm::IntoActiveModel;
    let mut active = platform_roles::Entity::find_by_id(PLATFORM_ROLE_SUPER_ADMIN)
        .one(&txn)
        .await
        .expect("query")
        .expect("exists")
        .into_active_model();
    active.protected = Set(false);
    active.grants_all = Set(false);
    active.name = Set("运维改过的名字".to_string());
    platform_roles::Entity::update(active)
        .exec(&txn)
        .await
        .expect("drift the row");

    let report = ensure_builtin_roles(&txn).await;
    assert!(
        report.repaired.contains(&PLATFORM_ROLE_SUPER_ADMIN_KEY),
        "结构字段被改动后必须被校正: {report:?}"
    );

    let after = platform_roles::Entity::find_by_id(PLATFORM_ROLE_SUPER_ADMIN)
        .one(&txn)
        .await
        .expect("query")
        .expect("exists");
    assert!(after.protected, "protected 必须被校正回 true");
    assert!(after.grants_all, "grants_all 必须被校正回 true");
    assert_eq!(
        after.name, "运维改过的名字",
        "name 是展示字段，启动时不得覆盖运维的修改"
    );
    assert_ne!(
        BUILTIN_ROLES[0].name, after.name,
        "确认上面断言的确实是「被改过的名字」而不是内置默认值"
    );

    txn.rollback().await.expect("rollback");
}

// ============================================================================
// orphan 权限：只报告，不删除
// ============================================================================

#[tokio::test]
async fn orphan_permissions_are_reported_but_not_deleted() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let _ = ensure_builtin_roles(&txn).await;

    let role_id = insert_role(&txn, "orphan-test-role").await;
    platform_role_permissions::Entity::insert(platform_role_permissions::ActiveModel {
        role_id: Set(role_id),
        permission: Set("legacy.removed.permission".into()),
    })
    .exec(&txn)
    .await
    .expect("insert orphan permission");

    let report = ensure_builtin_roles(&txn).await;
    let found = report
        .orphan_permissions
        .iter()
        .any(|(_, permission)| permission == "legacy.removed.permission");
    assert!(
        found,
        "orphan 权限必须被报告: {:?}",
        report.orphan_permissions
    );

    // 关键：**没有被静默删除**
    let still_there = platform_role_permissions::Entity::find()
        .filter(platform_role_permissions::Column::RoleId.eq(role_id))
        .one(&txn)
        .await
        .expect("query")
        .is_some();
    assert!(still_there, "orphan 权限不得被静默删除");

    txn.rollback().await.expect("rollback");
}

// ============================================================================
// 权限解析：并集 + grants_all 短路
// ============================================================================

#[tokio::test]
async fn permissions_of_roles_unions_and_short_circuits_on_grants_all() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let _ = ensure_builtin_roles(&txn).await;

    let role_a = insert_role(&txn, "perm-union-a").await;
    let role_b = insert_role(&txn, "perm-union-b").await;
    grant(&txn, role_a, Permission::EventsRead).await;
    grant(&txn, role_b, Permission::AwdRead).await;

    let (set, unknown) = permissions_of_roles(&txn, &[role_a, role_b])
        .await
        .expect("resolve");
    assert!(unknown.is_empty(), "unknown: {unknown:?}");
    assert!(set.contains(Permission::EventsRead));
    assert!(set.contains(Permission::AwdRead));
    assert_eq!(set.len(), 2, "只应包含两个角色的并集");

    let (empty, _) = permissions_of_roles(&txn, &[])
        .await
        .expect("resolve empty");
    assert!(empty.is_empty());

    let (all, unknown) = permissions_of_roles(&txn, &[role_a, PLATFORM_ROLE_SUPER_ADMIN])
        .await
        .expect("resolve with super-admin");
    assert!(unknown.is_empty());
    assert_eq!(
        all.len(),
        Permission::ALL.len(),
        "grants_all 必须短路为全集"
    );
    assert_eq!(all, PermissionSet::all());

    txn.rollback().await.expect("rollback");
}

/// orphan 权限进入第二个返回值，而**不**让整个解析失败。
#[tokio::test]
async fn permissions_of_roles_reports_unknown_without_failing() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let role_id = insert_role(&txn, "perm-unknown-role").await;
    grant(&txn, role_id, Permission::LogsRead).await;
    platform_role_permissions::Entity::insert(platform_role_permissions::ActiveModel {
        role_id: Set(role_id),
        permission: Set("totally.unknown.permission".into()),
    })
    .exec(&txn)
    .await
    .expect("insert unknown permission");

    let (set, unknown) = permissions_of_roles(&txn, &[role_id])
        .await
        .expect("解析不得因未知权限失败");
    assert!(set.contains(Permission::LogsRead));
    assert_eq!(set.len(), 1, "未知权限不得进入有效集合");
    assert_eq!(unknown, vec!["totally.unknown.permission".to_string()]);

    txn.rollback().await.expect("rollback");
}
