//! Wave 3：管理动作主体外键统一指向 `users(id)`（DB-gated）。
//!
//! 核心断言：
//! 1. 6 个 actor 外键的目标表是 `users`，删除行为全部是 `SET NULL`；
//! 2. `announcements.publisher_id` 可空 —— **删除发布者账号后公告必须留存**，
//!    `publisher_id` 变 NULL 而 `publisher` 展示名快照保持不变；
//! 3. 各 actor 列都接受 `users.id`（外键已真正指向统一身份空间）。

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use uuid::Uuid;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

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
            eprintln!("skip rbac_actor_fk_contract (DB unreachable): {e}");
            None
        }
    }
}

/// 本迁移负责的 6 个 actor 外键（`logs.superadmin_id` 由 M5 处理，不在此列）。
const ACTOR_FKS: &[(&str, &str)] = &[
    ("announcements_publisher_id_fkey", "announcements"),
    (
        "awd_internal_token_rotations_rotated_by_fkey",
        "awd_internal_token_rotations",
    ),
    ("awd_score_events_created_by_fkey", "awd_score_events"),
    ("awd_team_bans_banned_by_fkey", "awd_team_bans"),
    ("awd_team_bans_unbanned_by_fkey", "awd_team_bans"),
    (
        "awd_reset_records_requested_by_admin_fkey",
        "awd_reset_records",
    ),
];

#[tokio::test]
async fn actor_fks_point_at_users_with_set_null() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };

    for (conname, table) in ACTOR_FKS {
        let row = db
            .query_one(Statement::from_string(
                DbBackend::Postgres,
                format!(
                    "SELECT c.confrelid::regclass::text AS ref, c.confdeltype::text AS del
                       FROM pg_constraint c
                      WHERE c.conname = '{conname}' AND c.contype = 'f'"
                ),
            ))
            .await
            .unwrap_or_else(|e| panic!("query {conname}: {e}"))
            .unwrap_or_else(|| panic!("外键 {conname} 必须存在（表 {table}）"));

        assert_eq!(
            row.try_get::<String>("", "ref").unwrap(),
            "users",
            "{conname} 必须指向 users"
        );
        // 'n' = ON DELETE SET NULL
        assert_eq!(
            row.try_get::<String>("", "del").unwrap(),
            "n",
            "{conname} 必须是 ON DELETE SET NULL（审计主体注销后保留流水）"
        );
    }
}

/// **终点不变式**：`super_admin` 表已不再是任何外键的目标。
///
/// 历史：M4（Wave 3）把 6 个 actor 外键指向 `users`，但**有意豁免**
/// `logs.superadmin_id_fkey`（它由 M5 处理）。当时本测试断言「只剩 logs 一个」。
/// M5（Wave 6）把 `logs` 的双主体列合并为 `actor_user_id` 之后，
/// 该豁免随之消失 —— 因此断言升级为「一个都不剩」。
///
/// 这条断言的演进本身就是证据：它记录了「过渡态」到「终态」的切换，
/// 而不是一句永不失效的空话。
#[tokio::test]
async fn no_fk_targets_super_admin_anymore() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let remaining = db
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            "SELECT c.conname, c.conrelid::regclass::text AS tbl
               FROM pg_constraint c
              WHERE c.contype = 'f' AND c.confrelid = 'public.super_admin'::regclass"
                .to_string(),
        ))
        .await
        .expect("query super_admin FKs");

    let names: Vec<String> = remaining
        .into_iter()
        .map(|r| {
            format!(
                "{}.{}",
                r.try_get::<String>("", "tbl").unwrap(),
                r.try_get::<String>("", "conname").unwrap()
            )
        })
        .collect();
    assert!(
        names.is_empty(),
        "★ M5 之后不应再有外键指向 super_admin（Wave 6 已把 logs 的主体列统一）。\
         仍存在: {names:?}"
    );
}

/// 关键业务断言：**删除发布者账号不得删除平台公告**。
#[tokio::test]
async fn deleting_publisher_keeps_announcement_and_clears_publisher_id() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let txn = begin_exclusive(&db).await;

    let user_id = Uuid::new_v4();
    let uniq = &user_id.to_string()[..12];
    txn.execute_unprepared(&format!(
        "INSERT INTO users (id, username, nickname, password, email)
         VALUES ('{user_id}', 'pub-{uniq}', 'pubn-{uniq}', 'hash', 'pub-{uniq}@example.invalid')"
    ))
    .await
    .expect("insert publisher account");

    let announcement_id = Uuid::new_v4();
    txn.execute_unprepared(&format!(
        "INSERT INTO announcements (id, title, content, publisher_id, publisher)
         VALUES ('{announcement_id}', 'Wave3 公告', '内容', '{user_id}', 'pub-{uniq}')"
    ))
    .await
    .expect("insert announcement");

    // 删除账号 —— 唯一会触发 FK 动作的方式
    txn.execute_unprepared(&format!("DELETE FROM users WHERE id = '{user_id}'"))
        .await
        .expect("delete publisher account");

    let row = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT publisher_id, publisher, title FROM announcements WHERE id = '{announcement_id}'"
            ),
        ))
        .await
        .expect("query announcement")
        .expect("★ 公告必须仍然存在（删账号不应删除平台内容）");

    assert!(
        row.try_get::<Option<Uuid>>("", "publisher_id")
            .unwrap()
            .is_none(),
        "删账号后 publisher_id 必须被置为 NULL"
    );
    assert_eq!(
        row.try_get::<String>("", "publisher").unwrap(),
        format!("pub-{uniq}"),
        "★ publisher 展示名快照必须保留（前端据此展示「已注销」）"
    );

    txn.rollback().await.expect("rollback");
}

/// `publisher_id` 必须可空（否则 SET NULL 会在删账号时报错）。
#[tokio::test]
async fn announcements_publisher_id_is_nullable() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let row = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            "SELECT is_nullable FROM information_schema.columns
              WHERE table_schema='public' AND table_name='announcements'
                AND column_name='publisher_id'"
                .to_string(),
        ))
        .await
        .expect("query column")
        .expect("publisher_id 必须存在");
    assert_eq!(
        row.try_get::<String>("", "is_nullable").unwrap(),
        "YES",
        "announcements.publisher_id 必须可空（ON DELETE SET NULL 的前提）"
    );
}

/// 逐**列**断言 FK 目标表 —— 比按约束名断言更强：
///
/// 约束名可能被重建（改名 / 重加），而「这一列指向哪张表」才是授权语义所在。
/// 顺带固化 `DATABASE.md` 的 `event_id` 同名陷阱：AWD 表里的 `event_id`
/// 指向的是 `awd_events(event_id)` 而不是 `events(id)`，因此本测试不去插入
/// AWD 行（那需要先有 awd_events），只做结构与目标表断言。
#[tokio::test]
async fn actor_columns_target_users_table() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // (表, 列, 期望目标表)
    const ACTOR_COLUMNS: &[(&str, &str, &str)] = &[
        ("announcements", "publisher_id", "users"),
        ("awd_internal_token_rotations", "rotated_by", "users"),
        ("awd_score_events", "created_by", "users"),
        ("awd_team_bans", "banned_by", "users"),
        ("awd_team_bans", "unbanned_by", "users"),
        ("awd_reset_records", "requested_by_admin", "users"),
    ];

    for (table, column, expected) in ACTOR_COLUMNS {
        let row = db
            .query_one(Statement::from_string(
                DbBackend::Postgres,
                format!(
                    "SELECT c.confrelid::regclass::text AS ref
                       FROM pg_constraint c
                       JOIN pg_attribute a
                         ON a.attrelid = c.conrelid AND a.attnum = ANY (c.conkey)
                      WHERE c.contype = 'f'
                        AND c.conrelid = 'public.{table}'::regclass
                        AND a.attname = '{column}'"
                ),
            ))
            .await
            .unwrap_or_else(|e| panic!("query {table}.{column}: {e}"))
            .unwrap_or_else(|| panic!("{table}.{column} 必须有外键约束"));

        assert_eq!(
            row.try_get::<String>("", "ref").unwrap().as_str(),
            *expected,
            "{table}.{column} 必须指向 {expected}"
        );
    }
}

/// `awd_reset_records_requester_check` 必须仍在：统一主体后语义从
/// 「玩家 vs 管理员」变成「赛事参与主体 vs 平台操作主体」，两者仍然互斥。
#[tokio::test]
async fn awd_reset_requester_check_is_preserved() {
    let _guard = serialize();
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let row = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM pg_constraint
              WHERE contype = 'c' AND conname = 'awd_reset_records_requester_check'"
                .to_string(),
        ))
        .await
        .expect("query check constraint")
        .expect("row");
    assert_eq!(
        row.try_get::<i64>("", "n").unwrap(),
        1,
        "requested_by / requested_by_admin 的互斥 CHECK 必须保留"
    );
}
