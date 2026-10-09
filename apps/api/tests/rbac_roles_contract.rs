//! Wave 1：内置角色 reconcile 与权限解析（DB-gated）。
//!
//! 验证 [`ensure_builtin_roles`] 的幂等性、结构字段校正、orphan 权限**只报告不删除**，
//! 以及 [`permissions_of_roles`] 的并集 / `grants_all` 短路语义。

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use floatctf::core::permission::{Permission, PermissionSet};
use floatctf::core::system_ids::{PLATFORM_ROLE_SUPER_ADMIN, PLATFORM_ROLE_SUPER_ADMIN_KEY};
use floatctf::entity::{platform_role_permissions, platform_roles};
use floatctf::modules::identity::authorization::{
    BUILTIN_ROLES, ensure_builtin_roles, permissions_of_roles,
};

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

/// 测试文件级串行：多个用例共享 dev 库里的同一批内置角色行。
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn ensure_builtin_roles_creates_super_admin_idempotently() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // 与迁移/其它测试共存：先按 key 清掉非固定 id 的干扰行（不动固定 id 那行）
    let _ = platform_roles::Entity::delete_many()
        .filter(platform_roles::Column::Key.eq(PLATFORM_ROLE_SUPER_ADMIN_KEY))
        .filter(platform_roles::Column::Id.ne(PLATFORM_ROLE_SUPER_ADMIN))
        .exec(&db)
        .await;

    let first = ensure_builtin_roles(&db).await;
    assert!(
        first.conflicts.is_empty(),
        "首次 reconcile 不应有冲突: {:?}",
        first.conflicts
    );

    // 固定 id 的行必须存在且结构字段正确
    let role = platform_roles::Entity::find_by_id(PLATFORM_ROLE_SUPER_ADMIN)
        .one(&db)
        .await
        .expect("query")
        .expect("super-admin 角色必须存在");
    assert_eq!(role.key, PLATFORM_ROLE_SUPER_ADMIN_KEY);
    assert!(role.protected, "super-admin 必须 protected=true");
    assert!(role.grants_all, "super-admin 必须 grants_all=true");

    // 幂等：第二次不应报 created / repaired
    let second = ensure_builtin_roles(&db).await;
    assert!(
        second.created.is_empty(),
        "第二次 reconcile 不应再创建: {:?}",
        second.created
    );
    assert!(
        second.repaired.is_empty(),
        "第二次 reconcile 不应再修正: {:?}",
        second.repaired
    );
    assert!(second.conflicts.is_empty(), "{:?}", second.conflicts);
}

#[tokio::test]
async fn ensure_builtin_roles_repairs_structural_drift_without_touching_name() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // 先确保存在
    let _ = ensure_builtin_roles(&db).await;

    // 人为破坏结构字段 + 改展示名（模拟手工改库 / 误操作）
    let mut active = platform_roles::Entity::find_by_id(PLATFORM_ROLE_SUPER_ADMIN)
        .one(&db)
        .await
        .expect("query")
        .expect("exists")
        .into_active_model_for_test();
    active.protected = Set(false);
    active.grants_all = Set(false);
    active.name = Set("运维改过的名字".to_string());
    platform_roles::Entity::update(active)
        .exec(&db)
        .await
        .expect("drift the row");

    let report = ensure_builtin_roles(&db).await;
    assert!(
        report.repaired.contains(&PLATFORM_ROLE_SUPER_ADMIN_KEY),
        "结构字段被改动后必须被校正: {report:?}"
    );

    let after = platform_roles::Entity::find_by_id(PLATFORM_ROLE_SUPER_ADMIN)
        .one(&db)
        .await
        .expect("query")
        .expect("exists");
    assert!(after.protected, "protected 必须被校正回 true");
    assert!(after.grants_all, "grants_all 必须被校正回 true");
    assert_eq!(
        after.name, "运维改过的名字",
        "name 是展示字段，启动时不得覆盖运维的修改"
    );

    // 复原展示名，避免影响其它测试的日志可读性
    let mut restore = after.into_active_model_for_test();
    restore.name = Set(BUILTIN_ROLES[0].name.to_string());
    platform_roles::Entity::update(restore)
        .exec(&db)
        .await
        .expect("restore name");
}

#[tokio::test]
async fn orphan_permissions_are_reported_but_not_deleted() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let _ = ensure_builtin_roles(&db).await;

    let role_id = Uuid::new_v4();
    platform_roles::Entity::insert(platform_roles::ActiveModel {
        id: Set(role_id),
        key: Set(format!("orphan-test-{}", &role_id.to_string()[..8])),
        name: Set("Orphan Test".into()),
        description: Set(String::new()),
        protected: Set(false),
        grants_all: Set(false),
        ..Default::default()
    })
    .exec(&db)
    .await
    .expect("insert temp role");

    platform_role_permissions::Entity::insert(platform_role_permissions::ActiveModel {
        role_id: Set(role_id),
        permission: Set("legacy.removed.permission".into()),
    })
    .exec(&db)
    .await
    .expect("insert orphan permission");

    let report = ensure_builtin_roles(&db).await;
    let found = report
        .orphan_permissions
        .iter()
        .any(|(_, permission)| permission == "legacy.removed.permission");
    assert!(
        found,
        "orphan 权限必须被报告: {:?}",
        report.orphan_permissions
    );

    // 关键：**没有被删除**
    let still_there = platform_role_permissions::Entity::find()
        .filter(platform_role_permissions::Column::RoleId.eq(role_id))
        .one(&db)
        .await
        .expect("query")
        .is_some();
    assert!(still_there, "orphan 权限不得被静默删除");

    // 清理
    let _ = platform_roles::Entity::delete_by_id(role_id)
        .exec(&db)
        .await;
}

#[tokio::test]
async fn permissions_of_roles_unions_and_short_circuits_on_grants_all() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // 两个自定义角色：一个含 events.read，一个含 awd.read
    let role_a = Uuid::new_v4();
    let role_b = Uuid::new_v4();
    for (id, key) in [
        (role_a, format!("perm-a-{}", &role_a.to_string()[..8])),
        (role_b, format!("perm-b-{}", &role_b.to_string()[..8])),
    ] {
        platform_roles::Entity::insert(platform_roles::ActiveModel {
            id: Set(id),
            key: Set(key),
            name: Set("Perm Test".into()),
            description: Set(String::new()),
            protected: Set(false),
            grants_all: Set(false),
            ..Default::default()
        })
        .exec(&db)
        .await
        .expect("insert temp role");
    }
    for (id, permission) in [
        (role_a, Permission::EventsRead.as_str()),
        (role_b, Permission::AwdRead.as_str()),
    ] {
        platform_role_permissions::Entity::insert(platform_role_permissions::ActiveModel {
            role_id: Set(id),
            permission: Set(permission.to_string()),
        })
        .exec(&db)
        .await
        .expect("insert role permission");
    }

    // 并集
    let (set, unknown) = permissions_of_roles(&db, &[role_a, role_b])
        .await
        .expect("resolve");
    assert!(unknown.is_empty(), "unknown: {unknown:?}");
    assert!(set.contains(Permission::EventsRead));
    assert!(set.contains(Permission::AwdRead));
    assert_eq!(set.len(), 2, "只应包含两个角色的并集");

    // 空角色 → 空集合
    let (empty, _) = permissions_of_roles(&db, &[]).await.expect("resolve empty");
    assert!(empty.is_empty());

    // grants_all 短路：并入 super-admin 后必须是全集
    let (all, unknown) = permissions_of_roles(&db, &[role_a, PLATFORM_ROLE_SUPER_ADMIN])
        .await
        .expect("resolve with super-admin");
    assert!(unknown.is_empty());
    assert_eq!(
        all.len(),
        Permission::ALL.len(),
        "grants_all 必须短路为全集"
    );
    assert_eq!(all, PermissionSet::all());

    // 清理
    for id in [role_a, role_b] {
        let _ = platform_roles::Entity::delete_by_id(id).exec(&db).await;
    }
}

/// 让测试能改一行：`Model` → `ActiveModel`（避免为测试改业务代码可见性）。
trait IntoActiveModelForTest {
    fn into_active_model_for_test(self) -> platform_roles::ActiveModel;
}

impl IntoActiveModelForTest for platform_roles::Model {
    fn into_active_model_for_test(self) -> platform_roles::ActiveModel {
        use sea_orm::IntoActiveModel;
        self.into_active_model()
    }
}
