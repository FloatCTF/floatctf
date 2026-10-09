//! Wave 6：审计主体统一（M5）契约测试。
//!
//! 覆盖两类断言：
//! 1. **DB 层**（DB-gated）：新列/外键/索引的形状，以及最关键的
//!    「**删除账号不得删除审计流水**」；
//! 2. **源码守卫**（无需 DB）：防止重新引入「双主体列」或其残影。

use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, Statement,
    TransactionTrait,
};
use uuid::Uuid;

use floatctf::entity::sea_orm_active_enums::AccountStatus;
use floatctf::entity::{logs, users};

const RBAC_TEST_LOCK_KEY: i64 = 1179204949;

async fn connect_or_skip() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into());
    match sea_orm::Database::connect(&url).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip audit_actor_unification_contract (DB unreachable): {e}");
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

async fn scalar_i64<C: ConnectionTrait>(db: &C, sql: &str) -> i64 {
    db.query_one(Statement::from_string(DbBackend::Postgres, sql.to_string()))
        .await
        .expect("query")
        .expect("row")
        .try_get::<i64>("", "n")
        .expect("column n")
}

// ============================================================================
// DB 层
// ============================================================================

/// `logs` 必须有且仅有 `actor_user_id` 一个主体列；旧的两列必须消失。
#[tokio::test]
async fn logs_has_exactly_one_actor_column() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let actor = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n FROM information_schema.columns
          WHERE table_schema='public' AND table_name='logs' AND column_name='actor_user_id'",
    )
    .await;
    assert_eq!(actor, 1, "logs.actor_user_id 必须存在且唯一");

    let legacy = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n FROM information_schema.columns
          WHERE table_schema='public' AND table_name='logs'
            AND column_name IN ('user_id','superadmin_id')",
    )
    .await;
    assert_eq!(
        legacy, 0,
        "★ 旧的双主体列必须已被删除（双列会让审计查询静默漏掉管理动作）"
    );
}

/// 外键必须是 `ON DELETE SET NULL`（不是 CASCADE）。
#[tokio::test]
async fn actor_fk_is_set_null_and_indexed() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    let row = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            "SELECT c.confrelid::regclass::text AS ref, c.confdeltype::text AS del
               FROM pg_constraint c
              WHERE c.conname = 'logs_actor_user_id_fkey' AND c.contype = 'f'"
                .to_string(),
        ))
        .await
        .expect("query constraint")
        .expect("logs_actor_user_id_fkey 必须存在");
    assert_eq!(row.try_get::<String>("", "ref").unwrap(), "users");
    assert_eq!(
        row.try_get::<String>("", "del").unwrap(),
        "n",
        "必须是 ON DELETE SET NULL —— 删账号绝不能删审计流水"
    );

    let idx = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n FROM pg_indexes
          WHERE schemaname='public' AND tablename='logs' AND indexname='idx_sys_logs_actor'",
    )
    .await;
    assert_eq!(idx, 1, "按主体筛选是高频审计查询，必须有索引");
}

/// **最关键的业务断言**：删除账号后审计流水必须留存，主体置空。
#[tokio::test]
async fn deleting_actor_preserves_audit_row() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let actor_id = Uuid::new_v4();
    let uniq = &actor_id.to_string()[..12];
    users::Entity::insert(users::ActiveModel {
        id: sea_orm::Set(actor_id),
        username: sea_orm::Set(format!("audit-{uniq}")),
        nickname: sea_orm::Set(format!("auditn-{uniq}")),
        password: sea_orm::Set("x".into()),
        email: sea_orm::Set(format!("audit-{uniq}@example.invalid")),
        avatar: sea_orm::Set(None),
        status: sea_orm::Set(AccountStatus::Active),
        token_version: sea_orm::Set(0),
        ..Default::default()
    })
    .exec(&txn)
    .await
    .expect("insert actor");

    txn.execute_unprepared(&format!(
        "INSERT INTO logs (category, action, level, message, details, actor_user_id)
         VALUES ('ROLES', 'SET_USER_ROLES', 'WARN', 'wave6 审计留存测试', '{{}}'::jsonb, '{actor_id}')"
    ))
    .await
    .expect("insert audit row");

    // 硬删除账号 —— 唯一会触发外键动作的方式
    txn.execute_unprepared(&format!("DELETE FROM users WHERE id = '{actor_id}'"))
        .await
        .expect("delete actor");

    let row = txn
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            "SELECT actor_user_id, message, action FROM logs
              WHERE message = 'wave6 审计留存测试'"
                .to_string(),
        ))
        .await
        .expect("query logs")
        .expect("★ 审计流水必须仍然存在（删账号不得删除审计）");

    assert!(
        row.try_get::<Option<Uuid>>("", "actor_user_id")
            .unwrap()
            .is_none(),
        "主体注销后 actor_user_id 必须置空"
    );
    assert_eq!(
        row.try_get::<String>("", "message").unwrap(),
        "wave6 审计留存测试",
        "行为内容必须保留（审计价值在于「发生过什么」）"
    );

    txn.rollback().await.expect("rollback");
}

/// 新写入的审计行必须能带主体，且实体字段名与迁移一致。
#[tokio::test]
async fn audit_rows_carry_a_single_actor() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    let actor_id = Uuid::new_v4();
    let uniq = &actor_id.to_string()[..12];
    users::Entity::insert(users::ActiveModel {
        id: sea_orm::Set(actor_id),
        username: sea_orm::Set(format!("audit2-{uniq}")),
        nickname: sea_orm::Set(format!("audit2n-{uniq}")),
        password: sea_orm::Set("x".into()),
        email: sea_orm::Set(format!("audit2-{uniq}@example.invalid")),
        avatar: sea_orm::Set(None),
        status: sea_orm::Set(AccountStatus::Active),
        token_version: sea_orm::Set(0),
        ..Default::default()
    })
    .exec(&txn)
    .await
    .expect("insert actor");

    logs::ActiveModel {
        category: sea_orm::Set("AUTH".into()),
        action: sea_orm::Set("LOGIN".into()),
        level: sea_orm::Set("INFO".into()),
        message: sea_orm::Set("wave6 单主体写入".into()),
        details: sea_orm::Set(serde_json::json!({})),
        actor_user_id: sea_orm::Set(Some(actor_id)),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .expect("实体必须只有 actor_user_id 字段（编译期已保证）");

    let n = scalar_i64(
        &txn,
        "SELECT count(*)::bigint AS n FROM logs WHERE actor_user_id IS NOT NULL
           AND message = 'wave6 单主体写入'",
    )
    .await;
    assert_eq!(n, 1);

    txn.rollback().await.expect("rollback");
}

/// 回填后的数据完整性：任何非空主体都必须在 `users` 里存在。
///
/// 迁移自带这项自检，这里在**当前**数据上再断言一次（新写入路径同样受约束）。
#[tokio::test]
async fn no_orphan_actor_references() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let n = scalar_i64(
        &db,
        "SELECT count(*)::bigint AS n FROM logs l
          WHERE l.actor_user_id IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM users u WHERE u.id = l.actor_user_id)",
    )
    .await;
    assert_eq!(n, 0, "不允许存在指向不存在账号的审计主体");
}

// ============================================================================
// 源码守卫
// ============================================================================

/// `add_log` 必须只接受**一个**主体参数。
///
/// 这是防止回退到双主体列的第一道闸门：一旦有人加回第二个参数，
/// 调用点就会重新出现「只填一个」的机会，审计漏记随之回归。
#[test]
fn add_log_takes_exactly_one_actor_parameter() {
    let src = include_str!("../src/infrastructure/logging.rs");
    let sig = src
        .split("pub async fn add_log(")
        .nth(1)
        .expect("add_log 必须存在")
        .split(") {")
        .next()
        .expect("签名结束");
    assert!(
        sig.contains("actor_user_id: Option<Uuid>"),
        "add_log 必须接受单个 actor_user_id"
    );
    // 逐行取参数名：不能用 `contains("user_id: ...")`，
    // 因为 `actor_user_id: Option<Uuid>` 本身就包含子串 `user_id: Option<Uuid>`。
    let names: Vec<String> = sig
        .lines()
        .filter_map(|l| {
            let t = l.trim().trim_end_matches(',');
            let (name, _) = t.split_once(':')?;
            let name = name.trim().trim_start_matches('&');
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect();
    for forbidden in ["user_id", "superadmin_id"] {
        assert!(
            !names.iter().any(|n| n == forbidden),
            "add_log 不得再接受参数 {forbidden}（Wave 6 已把双主体合并为单一 actor）；实际参数: {names:?}"
        );
    }
    // `&self` 没有冒号，单独断言
    assert!(sig.contains("&self"), "add_log 必须是 &self 方法");
    assert_eq!(
        names,
        vec![
            "level",
            "category",
            "action",
            "message",
            "details",
            "actor_user_id",
            "request"
        ],
        "add_log 的参数表必须恰好是这 7 项（actor 只允许一个）"
    );
}

/// `AuditService::record` 同样只能有一个主体参数。
#[test]
fn audit_record_takes_exactly_one_actor_parameter() {
    let src = include_str!("../src/infrastructure/audit/service.rs");
    let sig = src
        .split("pub async fn record(")
        .nth(1)
        .expect("record 必须存在")
        .split(") {")
        .next()
        .expect("签名结束");
    assert!(sig.contains("actor_user_id: Option<Uuid>"));
    assert!(
        !sig.contains("actor_superadmin_id"),
        "record 不得再有 actor_superadmin_id"
    );
}

/// 全仓不得再引用 `logs` 的已删除主体列。
///
/// 只看非注释行，避免命中说明文字（此前踩过这个坑）。
#[test]
fn no_code_references_removed_log_actor_columns() {
    use std::path::{Path, PathBuf};

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                rust_files(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);

    let mut offenders = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for needle in ["logs::Column::UserId", "logs::Column::SuperadminId"] {
                // 注意 `event_logs::Column::UserId` 是**另一张表**（赛事级参与审计），
                // 它有自己的 user_id 语义，不属于本次统一范围。
                // 用「前一个字符不是标识符字符」来避免这个假阳性。
                for (idx, _) in line.match_indices(needle) {
                    let prev = line[..idx].chars().next_back();
                    let is_own_table = !prev
                        .map(|c| c.is_ascii_alphanumeric() || c == '_')
                        .unwrap_or(false);
                    if is_own_table {
                        offenders.push(format!(
                            "{}:{}: {}",
                            f.strip_prefix(&root).unwrap_or(&f).display(),
                            i + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "以下位置仍引用 logs 的已删除主体列：\n{}",
        offenders.join("\n")
    );
}

/// 审计日志过滤器必须用 `actor_user_id`（单一主体筛选）。
#[test]
fn log_filter_key_is_actor_user_id() {
    let src = include_str!("../src/modules/platform/operations/logs.rs");
    assert!(
        src.contains("key: \"actor_user_id\"") && src.contains("logs::Column::ActorUserId"),
        "审计过滤器必须按 actor_user_id 筛选"
    );
    for forbidden in ["key: \"user_id\"", "key: \"superadmin_id\""] {
        assert!(
            !src.contains(forbidden),
            "不得保留 {forbidden} 过滤器（分列会让按主体筛选漏掉一半记录）"
        );
    }
}

/// 审计 DTO 必须只暴露单一主体字段。
#[test]
fn logs_dto_exposes_single_actor_field() {
    let src = include_str!("../src/modules/platform/operations/logs_dto.rs");
    assert!(src.contains("pub actor_user_id: Option<Uuid>"));
    for forbidden in [
        "pub user_id: Option<Uuid>",
        "pub superadmin_id: Option<Uuid>",
    ] {
        assert!(!src.contains(forbidden), "logs DTO 不得再暴露 {forbidden}");
    }
}
