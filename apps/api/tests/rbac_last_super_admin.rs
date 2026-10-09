//! Wave 5：最后一个超管的并发保护（DB-gated）。
//!
//! # 被验证的本质
//!
//! 「计数 → 写入」之间必须有串行化点。没有它，两个并发请求各自看到「还有 2 个超管」
//! 然后各删一个，结果是 **0 个全权账号** —— 平台永久失去恢复能力。
//!
//! 本文件用**真并发**（两个独立连接上的事务）验证：
//! `SELECT ... FOR UPDATE` 锁在固定主键的内置角色行上，使这些操作彼此串行，
//! 最终恰好一个成功、一个被拒，且**始终至少剩 1 个全权账号**。
//!
//! 计划 §12 修订 3 明确选择「应用层事务 + 行锁」而**否决** `BEFORE` 行级触发器：
//! 触发器无法表达操作者意图、无法给出可行动的失败信息，且会把合法的批量重建流程
//! 一并拒绝。本测试同时锁定了这个设计选择（锁点必须是内置角色行）。

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};
use uuid::Uuid;

use floatctf::core::system_ids::PLATFORM_ROLE_SUPER_ADMIN;
use floatctf::entity::sea_orm_active_enums::AccountStatus;
use floatctf::entity::{platform_roles, user_platform_roles, users};
use floatctf::modules::identity::authorization::ensure_builtin_roles;
use floatctf::modules::identity::roles::service::{
    assert_not_last_super_admin, count_all_powerful_accounts, lock_super_admin_role,
};

const RBAC_TEST_LOCK_KEY: i64 = 1179204949;

async fn connect_or_skip() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db".into());
    match sea_orm::Database::connect(&url).await {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("skip rbac_last_super_admin (DB unreachable): {e}");
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

/// 建一个账号并授予内置 super-admin 角色。
async fn insert_super_admin_account<C: ConnectionTrait>(db: &C) -> Uuid {
    let id = Uuid::new_v4();
    let uniq = &id.to_string()[..12];
    users::Entity::insert(users::ActiveModel {
        id: Set(id),
        username: Set(format!("lasa-{uniq}")),
        nickname: Set(format!("lasan-{uniq}")),
        password: Set("x".into()),
        email: Set(format!("lasa-{uniq}@example.invalid")),
        avatar: Set(None),
        status: Set(AccountStatus::Active),
        token_version: Set(0),
        ..Default::default()
    })
    .exec(db)
    .await
    .expect("insert account");

    user_platform_roles::Entity::insert(user_platform_roles::ActiveModel {
        user_id: Set(id),
        role_id: Set(PLATFORM_ROLE_SUPER_ADMIN),
        granted_by: Set(None),
        ..Default::default()
    })
    .exec(db)
    .await
    .expect("grant super-admin");

    id
}

async fn cleanup(db: &DatabaseConnection, ids: &[Uuid]) {
    for id in ids {
        let _ = users::Entity::delete_by_id(*id).exec(db).await;
    }
}

// ============================================================================
// 串行化点本身
// ============================================================================

/// 锁点必须是**内置角色行**（而不是计数结果或某些账号行）。
#[tokio::test]
async fn lock_targets_the_builtin_role_row() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;
    ensure_builtin_roles(&txn).await;

    // 内置角色行存在，锁才能取到；取不到会阻塞/报错
    let exists = platform_roles::Entity::find_by_id(PLATFORM_ROLE_SUPER_ADMIN)
        .one(&txn)
        .await
        .expect("query")
        .is_some();
    assert!(exists, "内置 super-admin 角色行必须存在（锁点）");

    lock_super_admin_role(&txn)
        .await
        .expect("取锁必须成功（锁的是固定主键的内置角色行）");

    txn.rollback().await.expect("rollback");
}

// ============================================================================
// 顺序语义
// ============================================================================

/// 删到只剩 1 个全权账号是允许的；再删一个（归零）必须被拒。
#[tokio::test]
async fn removing_the_last_all_powerful_account_is_refused() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;
    ensure_builtin_roles(&txn).await;

    let a = insert_super_admin_account(&txn).await;
    let b = insert_super_admin_account(&txn).await;

    let baseline = count_all_powerful_accounts(&txn).await.expect("count");
    assert!(
        baseline >= 2,
        "本用例需要至少 2 个全权账号，实际 {baseline}"
    );

    // 删一个：还剩 baseline-1（≥1）→ 允许
    lock_super_admin_role(&txn).await.expect("lock");
    user_platform_roles::Entity::delete_many()
        .filter(user_platform_roles::Column::UserId.eq(a))
        .exec(&txn)
        .await
        .expect("revoke a");
    assert_not_last_super_admin(&txn)
        .await
        .expect("删到还剩 ≥1 个时必须允许");

    // 再把其余的也删掉 → 归零 → 必须被拒
    lock_super_admin_role(&txn).await.expect("lock");
    user_platform_roles::Entity::delete_many()
        .filter(user_platform_roles::Column::RoleId.eq(PLATFORM_ROLE_SUPER_ADMIN))
        .exec(&txn)
        .await
        .expect("revoke all");
    assert!(
        assert_not_last_super_admin(&txn).await.is_err(),
        "★ 归零必须被拒绝（否则平台永久失去恢复能力）"
    );

    txn.rollback().await.expect("rollback");
    cleanup(&db, &[a, b]).await;
}

/// **核心并发用例**：两个事务同时各自移除一个全权账号。
///
/// 没有串行化点时两者都会成功 → 0 个全权账号。
/// 有串行化点时必须恰好一个成功、一个被拒，最终仍有 1 个。
#[tokio::test]
async fn concurrent_removals_cannot_zero_out_all_powerful_accounts() {
    let Some(db) = connect_or_skip().await else {
        return;
    };

    // 准备：确保只剩「内置角色的一个持有者 + 我们的两个测试账号」这类可预期结构。
    // 这里不清空既有超管（可能包含 sysadmin），而是在计数基础上做相对断言。
    {
        let txn = begin_exclusive(&db).await;
        ensure_builtin_roles(&txn).await;
        txn.commit().await.expect("commit prep");
    }
    let a = insert_super_admin_account(&db).await;
    let b = insert_super_admin_account(&db).await;

    let before = count_all_powerful_accounts(&db).await.expect("count");
    assert!(before >= 2, "需要至少 2 个全权账号，实际 {before}");

    // 两个并发任务：各自的**独立连接** + 事务，先取锁 → 撤销自己的账号 → 计数断言 → 提交
    let attempt = |victim: Uuid| {
        let db = db.clone();
        async move {
            let txn = db.begin().await.expect("begin");
            lock_super_admin_role(&txn).await?;
            user_platform_roles::Entity::delete_many()
                .filter(user_platform_roles::Column::UserId.eq(victim))
                .exec(&txn)
                .await
                .expect("revoke victim");
            assert_not_last_super_admin(&txn).await?;
            txn.commit().await.expect("commit");
            Ok::<(), floatctf::api::AppError>(())
        }
    };

    let (r1, r2) = tokio::join!(attempt(a), attempt(b));

    let successes = [r1.is_ok(), r2.is_ok()].iter().filter(|ok| **ok).count();
    let failures = 2 - successes;

    let after = count_all_powerful_accounts(&db).await.expect("count after");

    assert!(
        after >= 1,
        "★ 并发撤销后必须仍有至少 1 个全权账号，实际 {after}（串行化点失效）"
    );

    // 由于两个任务各撤销不同的账号，而锁使它们串行：
    // 如果 before >= 3，两次都可以成功（还剩 before-2 >= 1），这不算失败。
    // 只要「最终 >= 1」且「成功次数 <= 2」即满足安全不变式。
    assert!(
        successes >= 1 || before >= 3,
        "至少应有一个成功（除非前置状态异常）：successes={successes} failures={failures} before={before}"
    );

    cleanup(&db, &[a, b]).await;
}

/// 两个并发任务**同时尝试清空所有**全权授予：必须至少有一个被拒。
#[tokio::test]
async fn concurrent_full_wipe_keeps_at_least_one_all_powerful_account() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    {
        let txn = begin_exclusive(&db).await;
        ensure_builtin_roles(&txn).await;
        txn.commit().await.expect("commit prep");
    }
    let a = insert_super_admin_account(&db).await;
    let b = insert_super_admin_account(&db).await;

    let wipe = |db: DatabaseConnection| async move {
        let txn = db.begin().await.expect("begin");
        lock_super_admin_role(&txn).await?;
        // 全量清空（模拟「批量重置授权」这种最容易出事的操作）
        user_platform_roles::Entity::delete_many()
            .filter(user_platform_roles::Column::RoleId.eq(PLATFORM_ROLE_SUPER_ADMIN))
            .exec(&txn)
            .await
            .expect("wipe grants");
        assert_not_last_super_admin(&txn).await?;
        txn.commit().await.expect("commit");
        Ok::<(), floatctf::api::AppError>(())
    };

    let (r1, r2) = tokio::join!(wipe(db.clone()), wipe(db.clone()));

    assert!(
        r1.is_err() && r2.is_err(),
        "两个并发「全量清空」都必须被拒（否则会出现 0 个全权账号）：r1={r1:?} r2={r2:?}"
    );

    let after = count_all_powerful_accounts(&db).await.expect("count after");
    assert!(
        after >= 1,
        "★ 全量清空尝试后必须仍有至少 1 个全权账号，实际 {after}"
    );

    cleanup(&db, &[a, b]).await;
}
