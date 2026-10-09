//! Wave 2：统一账户迁移（M2 / M3）的契约与冲突预检测试（DB-gated）。
//!
//! # 方法
//!
//! 直接执行**迁移文件里的真实 SQL 文本**（`include_str!`），在**可回滚事务**内构造
//! 冲突 fixture，断言：
//!   * 冲突 → `RAISE EXCEPTION` 且消息带对应的 `IDENTITY_MIGRATION_PREFLIGHT[<tag>]`
//!     （fail closed，**绝不自动 rename**）；
//!   * 正常路径 → 复制成功，且 **password 哈希 / created_at / updated_at 逐字节保留**。
//!
//! 为让「后面的检查」（id / nickname）可达，测试先在一个事务内删掉已被 M3 复制过的
//! nil-UUID 账号，使 `super_admin` 与 `users` 在 id 上**不相交**；全部改动随事务回滚。
//!
//! `scripts/test-rust.sh` 会为 Rust 测试建一个**全新的一次性数据库**并把
//! `DATABASE_URL` 指过去，因此本文件不会污染开发库。

use chrono::{DateTime, FixedOffset};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use uuid::Uuid;

/// M2 / M3 的真实迁移 SQL（`include_str!` 相对本测试文件）。
const M2_SQL: &str =
    include_str!("../src/sql/migrations/20261009143746-identity-account-columns-and-preflight.sql");
const M3_SQL: &str =
    include_str!("../src/sql/migrations/20261009143806-migrate-super-admin-to-users.sql");

/// 文件级串行：所有用例都在 `users` / `super_admin` / `platform_roles` 上开事务并
/// 执行 DDL（`ALTER TABLE ... ADD COLUMN IF NOT EXISTS` 也会取 AccessExclusiveLock），
/// 并行运行会互相死锁。
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

const ROLE_SUPER_ADMIN: &str = "00000000-0000-0000-0000-000000000000";
const NIL_USER: &str = "00000000-0000-0000-0000-000000000000";

const RBAC_TEST_LOCK_KEY: i64 = 1179204949;

/// 跨**进程**串行：cargo 为每个测试文件起独立进程，而这些用例会在同一批表上
/// 开事务（甚至执行 DDL，取 AccessExclusiveLock），跨文件并行会互相阻塞直至超时。
/// 用 Postgres 事务级 advisory lock 串行（与 `migrate.sh` 同一思路），事务结束自动释放。
async fn begin_exclusive(db: &DatabaseConnection) -> sea_orm::DatabaseTransaction {
    let txn = db.begin().await.expect("begin exclusive txn");
    txn.execute_unprepared(&format!(
        "SELECT pg_advisory_xact_lock({RBAC_TEST_LOCK_KEY})"
    ))
    .await
    .expect("acquire advisory xact lock");
    txn
}

async fn connect_or_skip() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into());
    match sea_orm::Database::connect(&url).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip identity_migration_contract (DB unreachable): {e}");
            None
        }
    }
}

/// 在一个事务里构造「干净起点」：删除 M3 已复制的 nil-UUID 账号，
/// 让 `super_admin` 与 `users` 在 id 上不相交，从而能命中后续检查。
async fn begin_disjoint_txn(db: &DatabaseConnection) -> sea_orm::DatabaseTransaction {
    let txn = begin_exclusive(&db).await;
    txn.execute_unprepared(&format!("DELETE FROM users WHERE id = '{NIL_USER}'"))
        .await
        .expect("delete merged nil-uuid account inside txn");
    txn
}

/// 插入一个临时 super_admin 行。
async fn insert_super_admin(
    txn: &sea_orm::DatabaseTransaction,
    id: Uuid,
    username: &str,
    email: &str,
) {
    txn.execute_unprepared(&format!(
        "INSERT INTO super_admin (id, username, password, email, created_at, updated_at)
         VALUES ('{id}', '{username}', 'plain-super-admin-hash', '{email}',
                 '2025-01-02 03:04:05.678901+00', '2025-06-07 08:09:10.111213+00')"
    ))
    .await
    .unwrap_or_else(|e| panic!("insert super_admin fixture {username}: {e}"));
}

async fn run_m3(txn: &sea_orm::DatabaseTransaction) -> Result<(), String> {
    txn.execute_unprepared(M3_SQL)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

async fn assert_preflight_tag(txn: &sea_orm::DatabaseTransaction, tag: &str) {
    let err = run_m3(txn).await.expect_err("M3 必须 fail closed");
    assert!(
        err.contains(&format!("IDENTITY_MIGRATION_PREFLIGHT[{tag}]")),
        "期望 preflight tag [{tag}]，实际错误为: {err}"
    );
}

/// 任取一个既有玩家账号（用于制造冲突）。
async fn any_existing_user(
    txn: &sea_orm::DatabaseTransaction,
) -> Option<(Uuid, String, String, String)> {
    let row = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            "SELECT id, username, nickname, email FROM users ORDER BY username LIMIT 1".to_string(),
        ))
        .await
        .expect("query users")?;
    Some((
        row.try_get("", "id").unwrap(),
        row.try_get("", "username").unwrap(),
        row.try_get("", "nickname").unwrap(),
        row.try_get("", "email").unwrap(),
    ))
}

// ============================================================================
// M2：列与默认值
// ============================================================================

/// M2 的 DDL 与预检都必须可重复执行（`ADD COLUMN IF NOT EXISTS` / `DO` 判存）。
///
/// 用「不相交起点」让预检通过，从而验证幂等性而不是冲突路径。
#[tokio::test]
async fn m2_is_idempotent() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_disjoint_txn(&db).await;
    txn.execute_unprepared(M2_SQL)
        .await
        .expect("重复执行 M2 必须成功（幂等）");
    txn.rollback().await.expect("rollback");
}

#[tokio::test]
async fn m2_columns_exist_with_fail_closed_defaults() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // 两列都必须 NOT NULL 且有默认值
    for column in ["status", "token_version"] {
        let row = db
            .query_one(Statement::from_string(
                DbBackend::Postgres,
                format!(
                    "SELECT is_nullable, column_default FROM information_schema.columns
                      WHERE table_schema='public' AND table_name='users' AND column_name='{column}'"
                ),
            ))
            .await
            .expect("query column")
            .expect("column must exist");
        assert_eq!(
            row.try_get::<String>("", "is_nullable").unwrap(),
            "NO",
            "users.{column} 必须 NOT NULL"
        );
        assert!(
            row.try_get::<Option<String>>("", "column_default")
                .unwrap()
                .is_some(),
            "users.{column} 必须有默认值（fail closed）"
        );
    }

    // account_status 枚举恰好两个值
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            "SELECT enumlabel FROM pg_enum e JOIN pg_type t ON t.oid = e.enumtypid
              WHERE t.typname = 'account_status' ORDER BY enumsortorder"
                .to_string(),
        ))
        .await
        .expect("query enum labels");
    let labels: Vec<String> = rows
        .into_iter()
        .map(|r| r.try_get::<String>("", "enumlabel").unwrap())
        .collect();
    assert_eq!(
        labels,
        vec!["active", "disabled"],
        "account_status 只应有 active / disabled 两值（不做过度设计）"
    );

    // 新建账号的默认值必须是「可用」的 active / 0
    let txn = begin_exclusive(&db).await;
    let uniq = Uuid::new_v4().to_string();
    txn.execute_unprepared(&format!(
        "INSERT INTO users (username, nickname, password, email)
         VALUES ('m2-{uniq}', 'm2n-{uniq}', 'h', 'm2-{uniq}@example.invalid')"
    ))
    .await
    .expect("insert user with defaults");
    let row = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT status::text AS s, token_version FROM users WHERE username = 'm2-{uniq}'"
            ),
        ))
        .await
        .expect("query")
        .expect("row");
    assert_eq!(row.try_get::<String>("", "s").unwrap(), "active");
    assert_eq!(row.try_get::<i32>("", "token_version").unwrap(), 0);
    txn.rollback().await.expect("rollback");
}

// ============================================================================
// M3：冲突预检（fail closed）
// ============================================================================

#[tokio::test]
async fn m3_rejects_username_collision() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_disjoint_txn(&db).await;
    let Some((_, username, _, _)) = any_existing_user(&txn).await else {
        eprintln!("skip: no existing users to collide with");
        return;
    };

    insert_super_admin(
        &txn,
        Uuid::new_v4(),
        &username,
        "brand-new-admin@example.invalid",
    )
    .await;
    assert_preflight_tag(&txn, "username").await;
    txn.rollback().await.expect("rollback");
}

#[tokio::test]
async fn m3_rejects_email_collision() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_disjoint_txn(&db).await;
    let Some((_, _, _, email)) = any_existing_user(&txn).await else {
        eprintln!("skip: no existing users to collide with");
        return;
    };

    let uniq = Uuid::new_v4();
    insert_super_admin(&txn, uniq, &format!("email-collide-{uniq}"), &email).await;
    assert_preflight_tag(&txn, "email").await;
    txn.rollback().await.expect("rollback");
}

#[tokio::test]
async fn m3_rejects_duplicate_email_within_super_admin() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_disjoint_txn(&db).await;

    let shared = format!("dup-{}@example.invalid", Uuid::new_v4());
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    insert_super_admin(&txn, a, &format!("dup-a-{a}"), &shared).await;
    insert_super_admin(&txn, b, &format!("dup-b-{b}"), &shared).await;

    assert_preflight_tag(&txn, "email-internal").await;
    txn.rollback().await.expect("rollback");
}

#[tokio::test]
async fn m3_rejects_uuid_collision() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_disjoint_txn(&db).await;
    let Some((id, _, _, _)) = any_existing_user(&txn).await else {
        eprintln!("skip: no existing users to collide with");
        return;
    };

    // super_admin.id 与既有 users.id 相同 —— 合并会产生身份歧义
    insert_super_admin(
        &txn,
        id,
        &format!("uuid-collide-{id}"),
        "uuid-collide@example.invalid",
    )
    .await;
    assert_preflight_tag(&txn, "id").await;
    txn.rollback().await.expect("rollback");
}

#[tokio::test]
async fn m3_rejects_nickname_collision() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_disjoint_txn(&db).await;
    let Some((_, _, nickname, _)) = any_existing_user(&txn).await else {
        eprintln!("skip: no existing users to collide with");
        return;
    };

    // nickname 由 username 合成 → 管理员用户名撞上玩家昵称即冲突
    insert_super_admin(
        &txn,
        Uuid::new_v4(),
        &nickname,
        "nickname-collide@example.invalid",
    )
    .await;
    assert_preflight_tag(&txn, "nickname").await;
    txn.rollback().await.expect("rollback");
}

// ============================================================================
// M3：正常路径必须逐字节保留凭据与时间戳
// ============================================================================

#[tokio::test]
async fn m3_happy_path_preserves_hash_and_timestamps() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_disjoint_txn(&db).await;

    let id = Uuid::new_v4();
    let username = format!("wave2-admin-{}", &id.to_string()[..8]);
    let email = format!("wave2-{}@example.invalid", &id.to_string()[..8]);
    // 故意用一个不像 argon2 的串：本测试只断言「逐字节不变」，不做哈希校验
    let password_hash = "$argon2id$v=19$m=19456,t=2,p=1$AAABBBCCC$DDDEEEFFF-wave2-fixture";
    let created =
        DateTime::parse_from_rfc3339("2024-03-04T05:06:07.123456+00:00").expect("parse created");
    let updated =
        DateTime::parse_from_rfc3339("2024-09-10T11:12:13.654321+00:00").expect("parse updated");

    txn.execute_unprepared(&format!(
        "INSERT INTO super_admin (id, username, password, email, created_at, updated_at)
         VALUES ('{id}', '{username}', '{password_hash}', '{email}',
                 '{}', '{}')",
        created.to_rfc3339(),
        updated.to_rfc3339()
    ))
    .await
    .expect("insert super_admin fixture");

    run_m3(&txn).await.expect("M3 正常路径必须成功");

    let row = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT username, nickname, password, email, avatar,
                        status::text AS s, token_version, created_at, updated_at
                   FROM users WHERE id = '{id}'"
            ),
        ))
        .await
        .expect("query migrated user")
        .expect("migrated user must exist");

    assert_eq!(row.try_get::<String>("", "username").unwrap(), username);
    assert_eq!(
        row.try_get::<String>("", "nickname").unwrap(),
        username,
        "nickname 由 username 确定性合成"
    );
    assert_eq!(
        row.try_get::<String>("", "password").unwrap(),
        password_hash,
        "★ password 哈希必须逐字节保留（不得 rehash）"
    );
    assert_eq!(row.try_get::<String>("", "email").unwrap(), email);
    assert!(
        row.try_get::<Option<String>>("", "avatar")
            .unwrap()
            .is_none(),
        "super_admin 无 avatar → NULL"
    );
    assert_eq!(row.try_get::<String>("", "s").unwrap(), "active");
    assert_eq!(row.try_get::<i32>("", "token_version").unwrap(), 0);

    let migrated_created: DateTime<FixedOffset> = row.try_get("", "created_at").unwrap();
    let migrated_updated: DateTime<FixedOffset> = row.try_get("", "updated_at").unwrap();
    assert_eq!(migrated_created, created, "★ created_at 必须保留原值");
    assert_eq!(migrated_updated, updated, "★ updated_at 必须保留原值");

    // 角色授予：必须指内置 super-admin，且 granted_by 为 NULL（迁移非人类授予）
    let grant = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT granted_by, role_id::text AS role_id
                   FROM user_platform_roles WHERE user_id = '{id}'"
            ),
        ))
        .await
        .expect("query grant")
        .expect("grant must exist");
    assert_eq!(
        grant.try_get::<String>("", "role_id").unwrap(),
        ROLE_SUPER_ADMIN
    );
    assert!(
        grant
            .try_get::<Option<Uuid>>("", "granted_by")
            .unwrap()
            .is_none(),
        "迁移产生的授予不得伪造授予人"
    );

    // 重跑语义（**重要、有意为之**）：M3 成功后，它的前置条件被有意破坏
    // （super_admin 与 users 的身份空间已经合并，username/email/id 必然重疊），
    // 因此再次执行会被 preflight **拒绝**，而不是静默重复复制。
    //
    // 这在实践中不是问题：`migrate.sh` 以 `schema_migrations` 为门禁，
    // **永远不会重跑已应用的迁移**；而 M3 自身在事务内执行，
    // 失败即整体回滚（身份空间回到未合并状态），重跑依然可行。
    // 换句话说：这里选择的是「失败得响亮」而不是「静默双写」。
    // 授予恰好一行（记录在案，便于下面对比重跑失败后「没有多写」）
    let count = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT count(*)::bigint AS n FROM user_platform_roles WHERE user_id = '{id}'"),
        ))
        .await
        .expect("count")
        .expect("row")
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(count, 1, "正常路径必须恰好产生一行授予");

    let rerun = run_m3(&txn)
        .await
        .expect_err("M3 重跑必须被 preflight 拒绝");
    assert!(
        rerun.contains("IDENTITY_MIGRATION_PREFLIGHT[username]"),
        "重跑错误应指出原因，实际: {rerun}"
    );

    // 注意：Postgres 事务一旦有语句报错即进入 aborted 状态，后续语句会被拒绝
    // （25P02），因此这里**不能**再查询计数。上面已确认写入恰好一行，
    // 而重跑是被 preflight 在写入之前拒绝的，故不存在重复写入。

    txn.rollback().await.expect("rollback");
}

/// `super-admin` 角色必须存在且为 protected + grants_all。
#[tokio::test]
async fn m3_seeds_protected_grants_all_super_admin_role() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            "SELECT key, protected, grants_all FROM platform_roles".to_string(),
        ))
        .await
        .expect("query platform_roles");
    assert_eq!(
        rows.len(),
        1,
        "只应内建 super-admin 一个角色（其余为 UI 模板，不 seed）"
    );
    let row = &rows[0];
    assert_eq!(row.try_get::<String>("", "key").unwrap(), "super-admin");
    assert!(row.try_get::<bool>("", "protected").unwrap());
    assert!(row.try_get::<bool>("", "grants_all").unwrap());
}
