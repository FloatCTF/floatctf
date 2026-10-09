//! Wave 0 安全基线回归。
//!
//! 覆盖三项 Wave 0 必修安全项：
//! 1. **响应 DTO 永不包含密码哈希**（安全不变量 #1）；
//! 2. **`/api/admin/download` 对象级授权**（安全不变量 #14）——key 必须能解析为
//!    平台已知资源，否则 404；形状闸门拒绝穿越/绝对路径/未知前缀；
//! 3. SDK 侧不再有把账号列表打到 console 的调试输出（源码守卫）。
//!
//! 纯函数/序列化部分**总是运行**；需要数据库的部分软跳过（与仓库既有 DB-gated
//! 测试一致：DB 不可达时打印 skip 而不是失败）。

use std::path::Path;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, TransactionTrait,
};
use serde_json::json;
use uuid::Uuid;

use floatctf::api::AppError;
use floatctf::entity::{event_writeup, events, users};
use floatctf::modules::identity::administrator::dto::SuperAdminDto;
use floatctf::modules::identity::user::dto::UsersDto;
use floatctf::modules::platform::files::download::resolve_private_artifact;

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
}

// ============================================================================
// 1) 响应 DTO 无密码哈希
// ============================================================================

#[test]
fn users_dto_serializes_without_password() {
    let dto = UsersDto {
        id: Uuid::new_v4(),
        username: "alice".to_string(),
        nickname: "Alice".to_string(),
        email: "alice@example.com".to_string(),
        created_at: Utc::now().into(),
        updated_at: Utc::now().into(),
        avatar: None,
    };

    let value = serde_json::to_value(&dto).expect("serialize UsersDto");
    let encoded = value.to_string();

    assert!(
        value.get("password").is_none(),
        "UsersDto must not expose a password field: {encoded}"
    );
    assert!(
        value.get("password_hash").is_none(),
        "UsersDto must not expose a password_hash field: {encoded}"
    );
    assert!(
        !encoded.contains("$argon2"),
        "UsersDto must never carry an argon2 hash: {encoded}"
    );
    // 正向断言：真正需要暴露的字段仍在
    for key in [
        "id",
        "username",
        "nickname",
        "email",
        "created_at",
        "updated_at",
        "avatar",
    ] {
        assert!(
            value.get(key).is_some(),
            "UsersDto lost required field {key}"
        );
    }
}

#[test]
fn super_admin_dto_serializes_without_password() {
    let dto = SuperAdminDto {
        id: Uuid::new_v4(),
        username: "sysadmin".to_string(),
        email: "sysadmin@system.com".to_string(),
        created_at: Utc::now().into(),
        updated_at: Utc::now().into(),
    };

    let value = serde_json::to_value(&dto).expect("serialize SuperAdminDto");
    let encoded = value.to_string();

    assert!(
        value.get("password").is_none(),
        "SuperAdminDto leaked password: {encoded}"
    );
    assert!(
        value.get("password_hash").is_none(),
        "SuperAdminDto leaked password_hash: {encoded}"
    );
    assert!(
        !encoded.contains("$argon2"),
        "SuperAdminDto carried a hash: {encoded}"
    );
}

/// 源码守卫：identity 域**任何**响应 DTO 都不得声明 `password` 字段。
///
/// 序列化测试只能证明「当前这两个 DTO 干净」；本守卫防止将来有人在新增
/// response DTO 时又把 `password` 加回来。
#[test]
fn identity_dtos_declare_no_password_field() {
    let identity_dir = repo_root().join("apps/api/src/modules/identity");
    let mut offenders = Vec::new();

    fn scan(dir: &Path, offenders: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan(&path, offenders);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (lineno, line) in source.lines().enumerate() {
                let trimmed = line.trim();
                // 只看字段声明：`pub password: ...` / `pub(crate) password_hash: ...`
                if trimmed.starts_with("pub ")
                    && trimmed.contains("password")
                    && trimmed.contains(':')
                {
                    offenders.push(format!("{}:{}: {}", path.display(), lineno + 1, trimmed));
                }
            }
        }
    }

    scan(&identity_dir, &mut offenders);
    // 请求 DTO 里的 password 是合法的：它们不在 `dto.rs` 的响应结构里，
    // 而是 handler 模块内的 `*Request` 结构 → 下面按「是否在请求结构内」区分。
    // 这里只对 `dto.rs`（响应 DTO 的归属文件）做强断言。
    let dto_offenders: Vec<&String> = offenders
        .iter()
        .filter(|o| o.contains("/dto.rs:"))
        .collect();
    assert!(
        dto_offenders.is_empty(),
        "response DTO files must not declare a password field:\n  {}",
        dto_offenders
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// ============================================================================
// 2) SDK 无调试输出泄漏
// ============================================================================

/// SDK 不得在 `src/api/**` 里向 console 打印响应数据。
///
/// 历史缺陷：`api/admin/users.ts` 的 `console.log(res.data)` 会把整份账号列表
/// （当时含 password 哈希）写进浏览器控制台。
#[test]
fn sdk_api_modules_do_not_log_responses() {
    let dir = repo_root().join("packages/sdk/src/api");
    let mut offenders = Vec::new();

    fn scan(dir: &Path, offenders: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan(&path, offenders);
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (lineno, line) in source.lines().enumerate() {
                let trimmed = line.trim();
                // 注释里提到 `console.log` 是合法的（例如解释历史缺陷），只查真实代码。
                let is_comment = trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with('*');
                if !is_comment && line.contains("console.log") {
                    offenders.push(format!("{}:{}: {}", path.display(), lineno + 1, trimmed));
                }
            }
        }
    }

    scan(&dir, &mut offenders);
    assert!(
        offenders.is_empty(),
        "SDK API modules must not use console.log:\n  {}",
        offenders.join("\n  ")
    );
}

// ============================================================================
// 3) /api/admin/download 对象级授权
// ============================================================================

/// 跨**进程**串行：cargo 会为每个测试文件启动独立进程，`std::sync::Mutex` 只能在
/// 单个二进制内串行；而这些用例会在同一批表上开事务、甚至执行 DDL
/// （`ALTER TABLE ... ADD COLUMN IF NOT EXISTS` 会取 AccessExclusiveLock），
/// 跨文件并行会互相阻塞直至超时。
///
/// 用 Postgres 事务级 advisory lock 串行（与 `migrate.sh` 同一思路），
/// 事务结束自动释放，无需手工 unlock。
const RBAC_TEST_LOCK_KEY: i64 = 1179204949;

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
            eprintln!("skip identity_wave0_security (DB unreachable): {e}");
            None
        }
    }
}

async fn resolve<C: sea_orm::ConnectionTrait>(db: &C, key: &str) -> Result<Uuid, AppError> {
    resolve_private_artifact(db, key).await
}

#[tokio::test]
async fn download_rejects_path_traversal_and_unknown_prefixes() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    // 形状闸门：这些在触库之前就应被拒
    for key in [
        "/etc/passwd",
        "../writeups/a.pdf",
        "writeups/../../etc/passwd",
        "writeups\\evil.pdf",
        "writeups//a.pdf",
        "",
    ] {
        let err = resolve(&txn, key).await.expect_err("must be rejected");
        assert!(
            matches!(err, AppError::NotFound(_)),
            "key {key:?} must yield NotFound, got {err:?}"
        );
    }

    // 合法形状但无锚点
    for key in [
        "writeups/definitely-not-a-real-file.pdf",
        // 族 B 形态 + 随机 UUID（events 不存在该行）→ 必须被存在性检查拒掉
        &format!("writeups/{}/report.zip", Uuid::new_v4()),
        // AWDP source.tar.gz 前缀不属于本端点可达内容
        &format!("gameboxes/{}/awdp/deadbeef/source.tar.gz", Uuid::new_v4()),
    ] {
        let err = resolve(&txn, key).await.expect_err("must be rejected");
        assert!(
            matches!(err, AppError::NotFound(_)),
            "key {key:?} must yield NotFound, got {err:?}"
        );
    }
}

#[tokio::test]
async fn download_resolves_db_anchored_writeup_key() {
    let Some(db) = connect_or_skip().await else {
        return;
    };
    let txn = begin_exclusive(&db).await;

    // 取任一既有赛事与用户作为锚点；没有就跳过（空库环境）。
    let Some(event) = events::Entity::find().one(&txn).await.ok().flatten() else {
        eprintln!("skip download_resolves_db_anchored_writeup_key: no events in DB");
        return;
    };
    let Some(user) = users::Entity::find().one(&txn).await.ok().flatten() else {
        eprintln!("skip download_resolves_db_anchored_writeup_key: no users in DB");
        return;
    };

    // 唯一 key，避免与既有数据冲突
    let key = format!("writeups/wave0-test-{}.pdf", Uuid::new_v4());

    // 清理可能的历史残留（同一 key 不会重复，但保持幂等）
    let _ = event_writeup::Entity::delete_many()
        .filter(event_writeup::Column::FileUrl.eq(key.clone()))
        .exec(&txn)
        .await;

    event_writeup::ActiveModel {
        event_id: Set(event.id),
        user_id: Set(user.id),
        team_id: Set(None),
        file_url: Set(key.clone()),
        created_at: Set(Utc::now().into()),
    }
    .insert(&txn)
    .await
    .expect("insert test writeup row");

    let resolved = resolve(&txn, &key).await.expect("known key must resolve");
    assert_eq!(
        resolved, event.id,
        "DB-anchored key must resolve to its owning event"
    );

    // 清理
    event_writeup::Entity::delete_many()
        .filter(event_writeup::Column::FileUrl.eq(key.clone()))
        .exec(&txn)
        .await
        .expect("cleanup test writeup row");

    // 清理后同一 key 必须重新变成 NotFound（证明锚点是 DB 行，而不是字符串形状）
    let err = resolve(&txn, &key)
        .await
        .expect_err("deleted anchor must not resolve");
    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");

    let _ = json!({ "checked": key });
}
