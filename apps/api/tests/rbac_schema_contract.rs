//! Wave 1 的 M1 schema 契约测试（DB-gated）。
//!
//! 验证 `platform_roles` / `user_platform_roles` / `platform_role_permissions`
//! 的**结构约束与引用行为**，而不是「表能建出来」这种弱断言。
//!
//! DB 不可达时软跳过（与仓库既有 DB-gated 测试一致）。
//!
//! # 并发与事务
//!
//! cargo 为每个测试文件起独立进程，`std::sync::Mutex` 只能串行单个二进制；
//! 这些用例会在同一批表上开事务，跨文件并行会互相阻塞。因此用 Postgres
//! **事务级 advisory lock** 串行（与 `migrate.sh` 同一思路），事务结束自动释放。
//!
//! 「预期失败」的断言必须用 `SAVEPOINT` 隔离：Postgres 事务一旦有语句报错就进入
//! aborted 状态（25P02），后续语句全部被拒。

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use uuid::Uuid;

const RBAC_TEST_LOCK_KEY: i64 = 1179204949;

async fn connect_or_skip() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into());
    match sea_orm::Database::connect(&url).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip rbac_schema_contract (DB unreachable): {e}");
            None
        }
    }
}

/// 开一个持有跨进程排他锁的事务（结束即释放）。
async fn begin_exclusive(db: &DatabaseConnection) -> sea_orm::DatabaseTransaction {
    let txn = db.begin().await.expect("begin exclusive txn");
    txn.execute_unprepared(&format!(
        "SELECT pg_advisory_xact_lock({RBAC_TEST_LOCK_KEY})"
    ))
    .await
    .expect("acquire advisory xact lock");
    txn
}

async fn scalar_i64<C: ConnectionTrait>(db: &C, sql: &str) -> i64 {
    let row = db
        .query_one(Statement::from_string(DbBackend::Postgres, sql.to_string()))
        .await
        .expect("query must succeed")
        .expect("query must return a row");
    row.try_get::<i64>("", "n").expect("column n")
}

async fn exec<C: ConnectionTrait>(db: &C, sql: &str) -> Result<(), String> {
    db.execute(Statement::from_string(DbBackend::Postgres, sql.to_string()))
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// 断言一条语句被**拒绝**，并用 SAVEPOINT 保护外层事务不被 abort。
async fn expect_rejected<C: ConnectionTrait>(txn: &C, sql: &str, why: &str) {
    txn.execute_unprepared("SAVEPOINT expected_failure")
        .await
        .expect("create savepoint");
    let result = exec(txn, sql).await;
    txn.execute_unprepared("ROLLBACK TO SAVEPOINT expected_failure")
        .await
        .expect("rollback to savepoint");
    assert!(result.is_err(), "{why}");
}

/// 断言一条语句被**接受**（同样用 SAVEPOINT 保护，便于后续继续断言）。
async fn expect_accepted<C: ConnectionTrait>(txn: &C, sql: &str, why: &str) {
    txn.execute_unprepared("SAVEPOINT expected_success")
        .await
        .expect("create savepoint");
    match exec(txn, sql).await {
        Ok(()) => {
            txn.execute_unprepared("RELEASE SAVEPOINT expected_success")
                .await
                .expect("release savepoint");
        }
        Err(e) => panic!("{why}: {e}"),
    }
}

// ============================================================================
// 结构
// ============================================================================

#[tokio::test]
async fn m1_tables_and_primary_keys_exist() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let tables = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n FROM pg_tables
          WHERE schemaname = 'public'
            AND tablename IN ('platform_roles','user_platform_roles','platform_role_permissions')",
    )
    .await;
    assert_eq!(tables, 3, "M1 必须建出三张 RBAC 表");

    let composite_pks = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n
           FROM pg_constraint c
          WHERE c.contype = 'p'
            AND c.conrelid IN ('public.user_platform_roles'::regclass,
                               'public.platform_role_permissions'::regclass)
            AND array_length(c.conkey, 1) = 2",
    )
    .await;
    assert_eq!(composite_pks, 2, "两张关联表必须是复合主键（天然去重）");

    let unique_key = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n
           FROM pg_constraint
          WHERE contype = 'u'
            AND conrelid = 'public.platform_roles'::regclass
            AND conname = 'platform_roles_key_key'",
    )
    .await;
    assert_eq!(unique_key, 1, "platform_roles.key 必须唯一");

    let triggers = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n FROM pg_trigger
          WHERE tgname = 'trg_platform_roles_updated_at' AND NOT tgisinternal",
    )
    .await;
    assert_eq!(triggers, 1, "platform_roles 必须有 updated_at 触发器");

    let nullable = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n FROM information_schema.columns
          WHERE table_schema='public' AND table_name='user_platform_roles'
            AND column_name='granted_by' AND is_nullable='YES'",
    )
    .await;
    assert_eq!(nullable, 1, "granted_by 必须可空（迁移授予无授予人）");
}

// ============================================================================
// CHECK 约束
// ============================================================================

#[tokio::test]
async fn m1_check_constraints_reject_invalid_values() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    for bad in ["Super-Admin", "has space", "a", "9lead"] {
        expect_rejected(
            &txn,
            &format!("INSERT INTO platform_roles (key, name) VALUES ('{bad}', 'x')"),
            &format!("platform_roles.key={bad:?} 必须被 CHECK 拒绝"),
        )
        .await;
    }

    expect_rejected(
        &txn,
        "INSERT INTO platform_roles (key, name) VALUES ('blank-name-test', '   ')",
        "空白 name 必须被 CHECK 拒绝",
    )
    .await;

    let role_id = Uuid::new_v4();
    expect_accepted(
        &txn,
        &format!(
            "INSERT INTO platform_roles (id, key, name) VALUES ('{role_id}', 'fmt-check-role', 'Fmt')"
        ),
        "临时角色必须可插入",
    )
    .await;

    for bad in ["USERS.READ", "users", "users.", ".users", "users read"] {
        expect_rejected(
            &txn,
            &format!(
                "INSERT INTO platform_role_permissions (role_id, permission) VALUES ('{role_id}', '{bad}')"
            ),
            &format!("permission={bad:?} 必须被 CHECK 拒绝"),
        )
        .await;
    }

    for good in ["users.read", "events.participants.manage", "terminal.open"] {
        expect_accepted(
            &txn,
            &format!(
                "INSERT INTO platform_role_permissions (role_id, permission) VALUES ('{role_id}', '{good}')"
            ),
            &format!("permission={good:?} 必须通过 CHECK"),
        )
        .await;
    }

    expect_rejected(
        &txn,
        &format!(
            "INSERT INTO platform_role_permissions (role_id, permission) VALUES ('{role_id}', 'users.read')"
        ),
        "重复 (role_id, permission) 必须被复合主键拒绝",
    )
    .await;

    txn.rollback().await.expect("rollback");
}

// ============================================================================
// 默认值 fail closed
// ============================================================================

#[tokio::test]
async fn m1_defaults_are_fail_closed() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let role_id = Uuid::new_v4();
    expect_accepted(
        &txn,
        &format!(
            "INSERT INTO platform_roles (id, key, name) VALUES ('{role_id}', 'defaults-test', 'Defaults')"
        ),
        "临时角色必须可插入",
    )
    .await;

    let row = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT protected, grants_all, description FROM platform_roles WHERE id = '{role_id}'"
            ),
        ))
        .await
        .expect("query ok")
        .expect("row exists");

    assert!(
        !row.try_get::<bool>("", "protected").unwrap(),
        "自定义角色默认 protected=false"
    );
    assert!(
        !row.try_get::<bool>("", "grants_all").unwrap(),
        "自定义角色默认 grants_all=false（全权只能由迁移/bootstrap 建立）"
    );
    assert_eq!(row.try_get::<String>("", "description").unwrap(), "");

    txn.rollback().await.expect("rollback");
}

// ============================================================================
// 外键引用行为
// ============================================================================

#[tokio::test]
async fn m1_foreign_keys_cascade_as_designed() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let uniq = Uuid::new_v4().to_string();
    txn.execute_unprepared(&format!(
        "INSERT INTO users (username, nickname, password, email)
         VALUES ('rbac-fk-{uniq}', 'rbac-fkn-{uniq}', 'hash', 'rbac-fk-{uniq}@example.invalid')"
    ))
    .await
    .expect("insert temp user");
    let user_id: Uuid = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT id FROM users WHERE username = 'rbac-fk-{uniq}'"),
        ))
        .await
        .expect("query user")
        .expect("user row")
        .try_get("", "id")
        .expect("id");

    let role_id = Uuid::new_v4();
    txn.execute_unprepared(&format!(
        "INSERT INTO platform_roles (id, key, name) VALUES ('{role_id}', 'cascade-test', 'Cascade')"
    ))
    .await
    .expect("insert temp role");

    txn.execute_unprepared(&format!(
        "INSERT INTO user_platform_roles (user_id, role_id, granted_by)
         VALUES ('{user_id}', '{role_id}', '{user_id}')"
    ))
    .await
    .expect("insert assignment");

    txn.execute_unprepared(&format!(
        "DELETE FROM platform_roles WHERE id = '{role_id}'"
    ))
    .await
    .expect("delete role");
    assert_eq!(
        scalar_i64(
            &txn,
            &format!(
                "SELECT count(*)::bigint AS n FROM user_platform_roles WHERE user_id = '{user_id}'"
            )
        )
        .await,
        0,
        "删除角色必须级联删除授予行"
    );

    txn.execute_unprepared(&format!(
        "INSERT INTO platform_roles (id, key, name) VALUES ('{role_id}', 'cascade-test2', 'Cascade2')"
    ))
    .await
    .expect("re-insert role");
    txn.execute_unprepared(&format!(
        "INSERT INTO user_platform_roles (user_id, role_id, granted_by)
         VALUES ('{user_id}', '{role_id}', '{user_id}')"
    ))
    .await
    .expect("re-insert assignment");

    txn.execute_unprepared(&format!("DELETE FROM users WHERE id = '{user_id}'"))
        .await
        .expect("delete user");
    assert_eq!(
        scalar_i64(
            &txn,
            &format!(
                "SELECT count(*)::bigint AS n FROM user_platform_roles WHERE role_id = '{role_id}'"
            )
        )
        .await,
        0,
        "删除账号必须级联删除其角色授予"
    );

    txn.rollback().await.expect("rollback");
}

/// `granted_by` 在授予人被删除时**置空**（而不是级联删除整行授予）。
#[tokio::test]
async fn m1_granted_by_set_null_on_grantor_delete() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let uniq = Uuid::new_v4().to_string();
    txn.execute_unprepared(&format!(
        "INSERT INTO users (username, nickname, password, email)
         VALUES ('grantor-{uniq}', 'grantorn-{uniq}', 'hash', 'grantor-{uniq}@example.invalid'),
                ('grantee-{uniq}', 'granteen-{uniq}', 'hash', 'grantee-{uniq}@example.invalid')"
    ))
    .await
    .expect("insert two users");

    let ids: Vec<Uuid> = txn
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT id FROM users WHERE username LIKE '%-{uniq}' ORDER BY username"),
        ))
        .await
        .expect("query users")
        .into_iter()
        .map(|r| r.try_get::<Uuid>("", "id").unwrap())
        .collect();
    assert_eq!(ids.len(), 2);
    let (grantor, grantee) = (ids[0], ids[1]);

    let role_id = Uuid::new_v4();
    txn.execute_unprepared(&format!(
        "INSERT INTO platform_roles (id, key, name) VALUES ('{role_id}', 'grantor-test', 'Grantor')"
    ))
    .await
    .expect("insert role");
    txn.execute_unprepared(&format!(
        "INSERT INTO user_platform_roles (user_id, role_id, granted_by)
         VALUES ('{grantee}', '{role_id}', '{grantor}')"
    ))
    .await
    .expect("insert assignment");

    txn.execute_unprepared(&format!("DELETE FROM users WHERE id = '{grantor}'"))
        .await
        .expect("delete grantor");

    let row = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT granted_by FROM user_platform_roles WHERE user_id = '{grantee}' AND role_id = '{role_id}'"
            ),
        ))
        .await
        .expect("query assignment")
        .expect("★ 删授予人不得删除授予行");
    assert!(
        row.try_get::<Option<Uuid>>("", "granted_by")
            .unwrap()
            .is_none(),
        "granted_by 必须被置空而不是级联删除"
    );

    txn.rollback().await.expect("rollback");
}
