//! Wave 1 的 M1 schema 契约测试（DB-gated）。
//!
//! 验证 `platform_roles` / `user_platform_roles` / `platform_role_permissions`
//! 的**结构约束与引用行为**，而不是「表能建出来」这种弱断言。
//!
//! DB 不可达时软跳过（与仓库既有 DB-gated 测试一致）。

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use uuid::Uuid;

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

async fn scalar_i64(db: &DatabaseConnection, sql: &str) -> i64 {
    let row = db
        .query_one(Statement::from_string(DbBackend::Postgres, sql.to_string()))
        .await
        .expect("query must succeed")
        .expect("query must return a row");
    row.try_get::<i64>("", "n").expect("column n")
}

async fn exec(db: &DatabaseConnection, sql: &str) -> Result<(), String> {
    db.execute(Statement::from_string(DbBackend::Postgres, sql.to_string()))
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tokio::test]
async fn m1_tables_and_primary_keys_exist() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // 三张表都在
    let tables = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n FROM pg_tables
          WHERE schemaname = 'public'
            AND tablename IN ('platform_roles','user_platform_roles','platform_role_permissions')",
    )
    .await;
    assert_eq!(tables, 3, "M1 必须建出三张 RBAC 表");

    // 复合主键：两处必须是 (user_id, role_id) / (role_id, permission)
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

    // platform_roles.key 唯一
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
}

#[tokio::test]
async fn m1_check_constraints_reject_invalid_values() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // key 格式：大写 / 含空格 / 单字符 都必须被拒
    for bad in ["Super-Admin", "has space", "a", "9lead"] {
        let sql = format!("INSERT INTO platform_roles (key, name) VALUES ('{bad}', 'x')");
        assert!(
            exec(&db, &sql).await.is_err(),
            "platform_roles.key={bad:?} 必须被 CHECK 拒绝"
        );
    }

    // name 不能为空白
    assert!(
        exec(
            &db,
            "INSERT INTO platform_roles (key, name) VALUES ('blank-name-test', '   ')"
        )
        .await
        .is_err(),
        "空白 name 必须被 CHECK 拒绝"
    );

    // permission 格式
    let role_id = Uuid::new_v4();
    let setup = format!(
        "INSERT INTO platform_roles (id, key, name) VALUES ('{role_id}', 'fmt-check-role', 'Fmt')"
    );
    exec(&db, &setup).await.expect("insert temp role");

    for bad in ["USERS.READ", "users", "users.", ".users", "users read"] {
        let sql = format!(
            "INSERT INTO platform_role_permissions (role_id, permission) VALUES ('{role_id}', '{bad}')"
        );
        assert!(
            exec(&db, &sql).await.is_err(),
            "permission={bad:?} 必须被 CHECK 拒绝"
        );
    }

    // 合法值必须通过（含三段形态）
    for good in ["users.read", "events.participants.manage", "terminal.open"] {
        let sql = format!(
            "INSERT INTO platform_role_permissions (role_id, permission) VALUES ('{role_id}', '{good}')"
        );
        exec(&db, &sql)
            .await
            .unwrap_or_else(|e| panic!("permission={good:?} 必须通过 CHECK: {e}"));
    }

    // 同一 (role_id, permission) 重复插入必须被主键拒绝
    assert!(
        exec(
            &db,
            &format!(
                "INSERT INTO platform_role_permissions (role_id, permission) VALUES ('{role_id}', 'users.read')"
            )
        )
        .await
        .is_err(),
        "重复 (role_id, permission) 必须被复合主键拒绝"
    );

    let _ = exec(
        &db,
        &format!("DELETE FROM platform_roles WHERE id = '{role_id}'"),
    )
    .await;
}

#[tokio::test]
async fn m1_defaults_are_fail_closed() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let role_id = Uuid::new_v4();
    exec(
        &db,
        &format!("INSERT INTO platform_roles (id, key, name) VALUES ('{role_id}', 'defaults-test', 'Defaults')"),
    )
    .await
    .expect("insert temp role");

    let row = db
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
        "自定义角色默认 grants_all=false（全权必须显式、只能由迁移/bootstrap 建立）"
    );
    assert_eq!(row.try_get::<String>("", "description").unwrap(), "");

    let _ = exec(
        &db,
        &format!("DELETE FROM platform_roles WHERE id = '{role_id}'"),
    )
    .await;
}

#[tokio::test]
async fn m1_foreign_keys_cascade_as_designed() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // 需要一个真实 users 行（user_platform_roles.user_id → users.id）
    let user_row = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            // 建一个临时账号；nickname/email 唯一约束用随机后缀避开
            "INSERT INTO users (username, nickname, password, email)
             VALUES ('rbac-fk-' || substr(md5(random()::text), 1, 12),
                     'rbac-fk-' || substr(md5(random()::text), 1, 12),
                     '$argon2id$v=19$m=19456,t=2,p=1$fakesalt$fakehash',
                     'rbac-fk-' || substr(md5(random()::text), 1, 12) || '@example.invalid')
             RETURNING id"
                .to_string(),
        ))
        .await
        .expect("insert temp user")
        .expect("returning row");
    let user_id: Uuid = user_row.try_get("", "id").expect("id");

    let role_id = Uuid::new_v4();
    exec(
        &db,
        &format!("INSERT INTO platform_roles (id, key, name) VALUES ('{role_id}', 'cascade-test', 'Cascade')"),
    )
    .await
    .expect("insert temp role");

    exec(
        &db,
        &format!(
            "INSERT INTO user_platform_roles (user_id, role_id, granted_by)
             VALUES ('{user_id}', '{role_id}', '{user_id}')"
        ),
    )
    .await
    .expect("insert assignment");

    // 删除角色 → 授予行级联删除
    exec(
        &db,
        &format!("DELETE FROM platform_roles WHERE id = '{role_id}'"),
    )
    .await
    .expect("delete role");
    let remaining = scalar_i64(
        &db,
        &format!(
            "SELECT count(*)::bigint AS n FROM user_platform_roles WHERE user_id = '{user_id}'"
        ),
    )
    .await;
    assert_eq!(remaining, 0, "删除角色必须级联删除授予行");

    // 再建一次授予，然后删除账号 → 授予行级联删除
    exec(
        &db,
        &format!("INSERT INTO platform_roles (id, key, name) VALUES ('{role_id}', 'cascade-test2', 'Cascade2')"),
    )
    .await
    .expect("re-insert role");
    exec(
        &db,
        &format!(
            "INSERT INTO user_platform_roles (user_id, role_id, granted_by)
             VALUES ('{user_id}', '{role_id}', '{user_id}')"
        ),
    )
    .await
    .expect("re-insert assignment");

    exec(&db, &format!("DELETE FROM users WHERE id = '{user_id}'"))
        .await
        .expect("delete user");
    let assignments = scalar_i64(
        &db,
        &format!(
            "SELECT count(*)::bigint AS n FROM user_platform_roles WHERE role_id = '{role_id}'"
        ),
    )
    .await;
    assert_eq!(assignments, 0, "删除账号必须级联删除其角色授予");

    let _ = exec(
        &db,
        &format!("DELETE FROM platform_roles WHERE id = '{role_id}'"),
    )
    .await;
}

#[tokio::test]
async fn m1_updated_at_trigger_is_installed() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let triggers = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n
           FROM pg_trigger
          WHERE tgname = 'trg_platform_roles_updated_at'
            AND NOT tgisinternal",
    )
    .await;
    assert_eq!(triggers, 1, "platform_roles 必须有 updated_at 触发器");
}

/// `granted_by` 是**可空**的：迁移产生的内置授予没有人类授予人。
#[tokio::test]
async fn m1_granted_by_is_nullable() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let nullable = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n
           FROM information_schema.columns
          WHERE table_schema = 'public'
            AND table_name = 'user_platform_roles'
            AND column_name = 'granted_by'
            AND is_nullable = 'YES'",
    )
    .await;
    assert_eq!(nullable, 1, "granted_by 必须可空（迁移授予无授予人）");
}
