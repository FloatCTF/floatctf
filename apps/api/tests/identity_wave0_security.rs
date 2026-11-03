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

/// 源码守卫（**全仓**）：任何 `derive(Serialize)` 的响应结构体都不得持有
/// 「带敏感列的生成实体」。
///
/// # 为什么需要这条，而不是只保留上面那条
///
/// 上面那条只扫 `modules/identity`。历史缺陷正好发生在这个范围之外：
/// `modules/event/common/api/event_users.rs` 的
/// `EventUserResult { user: users::Model, .. }` 派生了 `Serialize`，
/// 而 `users::Model` 是生成的 DB 列镜像、`password` 没有
/// `#[serde(skip_serializing)]` —— 于是
/// `GET /api/admin/events/{id}/users`（仅需 `events.read`）
/// 把参与者的 argon2id 哈希回传给了客户端，且**没有任何测试会发现它**。
///
/// 根因是「守卫的作用域比不变式的作用域窄」：不变式是**全平台**的
/// （响应 DTO 永不含 password），守卫却只覆盖一个模块。
/// 因此这条守卫扫描整个 `apps/api/src`。
///
/// `entity/` 被排除：那是生成物，它自己就是 Model 的定义处，
/// 不是「把 Model 放进响应」的地方。
#[test]
fn no_serialized_response_struct_holds_a_sensitive_entity_model() {
    // 带敏感列的生成实体（与 `apps/api/src/entity/*.rs` 的敏感列扫描一致）。
    const SENSITIVE: &[&str] = &[
        "users::Model",
        "awdp_evaluation_proofs::Model",
        "awd_wireguard_peers::Model",
    ];

    let src = repo_root().join("apps/api/src");
    let entity_dir = src.join("entity");
    let mut offenders = Vec::new();

    fn scan(dir: &Path, entity_dir: &Path, offenders: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path == entity_dir {
                    continue; // 生成物：Model 的定义处，不是响应体
                }
                scan(&path, entity_dir, offenders);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };

            // 逐行状态机：记住"当前是否处在一个 derive(Serialize) 的结构体内"。
            let mut in_serialized_struct = false;
            let mut depth: i32 = 0;
            let mut pending_derive = String::new();

            for (lineno, raw) in source.lines().enumerate() {
                let line = raw.trim();
                // 注释与文档注释一律不看 —— 否则解释性文字会被当成代码。
                if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
                    continue;
                }

                if line.starts_with("#[derive(") {
                    pending_derive.push_str(line);
                    continue;
                }
                if !pending_derive.is_empty() && !line.is_empty() {
                    // derive 结束：判断接下来是不是 Serialize 结构体
                    if line.starts_with("pub struct ") || line.starts_with("pub(crate) struct ") {
                        in_serialized_struct = pending_derive.contains("Serialize");
                    }
                    pending_derive.clear();
                }

                if in_serialized_struct {
                    depth += line.matches('{').count() as i32;
                    depth -= line.matches('}').count() as i32;

                    // 只看字段声明行：`pub field: Type,`
                    if line.starts_with("pub ") && line.contains(':') {
                        for ty in SENSITIVE {
                            // 必须做**标识符边界**匹配：`event_users::Model`
                            // 以 `users::Model` 结尾，朴素 `contains` 会误报。
                            let mut from = 0usize;
                            while let Some(rel) = line[from..].find(ty) {
                                let at = from + rel;
                                let prev_is_ident = line[..at]
                                    .chars()
                                    .next_back()
                                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
                                let next_is_ident = line[at + ty.len()..]
                                    .chars()
                                    .next()
                                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
                                if !prev_is_ident && !next_is_ident {
                                    offenders.push(format!(
                                        "{}:{}: {}",
                                        path.display(),
                                        lineno + 1,
                                        line
                                    ));
                                    break;
                                }
                                from = at + ty.len();
                            }
                        }
                    }

                    if depth <= 0 {
                        in_serialized_struct = false;
                        depth = 0;
                    }
                }
            }
        }
    }

    scan(&src, &entity_dir, &mut offenders);

    assert!(
        offenders.is_empty(),
        "这些响应结构体派生了 Serialize，却直接持有带敏感列的生成实体 —— \
         这会把密码哈希 / 私钥等回传给客户端。请改用对应的响应 DTO：\n  {}",
        offenders
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// 运行时回归：`EventUserResult` 的序列化结果**不含** password。
///
/// 上面那条源码守卫防的是"类型写错"；这条防的是"即使类型对了，
/// 序列化仍然把密码带出去"（例如将来有人给 DTO 加回字段）。
#[test]
fn event_user_result_serializes_without_password() {
    use floatctf::modules::event::common::api::event_users::EventUserResult;

    // 造一个"密码列里确实是 argon2 哈希"的实体：
    // 如果转换漏了 password，下面的断言会直接看到 `$argon2`。
    let now = Utc::now().into();
    let user = users::Model {
        id: Uuid::new_v4(),
        username: "bob".to_string(),
        nickname: "Bob".to_string(),
        password: "$argon2id$v=19$m=19456,t=2,p=1$FAKEFAKEFAKE$FAKEFAKEFAKE".to_string(),
        email: "bob@example.com".to_string(),
        created_at: now,
        updated_at: now,
        avatar: None,
        status: floatctf::entity::sea_orm_active_enums::AccountStatus::Active,
        token_version: 7,
    };

    let result = EventUserResult {
        id: user.id,
        user: user.into(),
        event_user: floatctf::entity::event_users::Model {
            event_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            points: 0.0,
            banned: false,
            joined_at: now,
        },
    };

    let value = serde_json::to_value(&result).expect("serialize EventUserResult");
    let encoded = value.to_string();

    assert!(
        value["user"].get("password").is_none(),
        "EventUserResult.user must not expose a password field: {encoded}"
    );
    assert!(
        !encoded.contains("$argon2"),
        "EventUserResult must never carry an argon2 hash: {encoded}"
    );
    assert!(
        !encoded.contains("token_version"),
        "token_version is a credential-rotation counter and must not leak: {encoded}"
    );
    // 正向断言：管理界面真正需要的字段仍在。
    for key in ["id", "username", "nickname", "email"] {
        assert!(
            value["user"].get(key).is_some(),
            "EventUserResult.user must still expose `{key}`: {encoded}"
        );
    }
    assert!(
        value.get("id").is_some(),
        "EventUserResult must expose a row `id`: {encoded}"
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

// ============================================================================
// 4) 授权错误必须早于 body 解析错误（403 优先于 400）
// ============================================================================

/// 源码守卫：`/admin` scope 必须挂 `pre_resolve_auth` 中间件。
///
/// # 这条守卫防的是什么
///
/// actix-web 的元组提取器（`actix_web::extract` 的 `tuple_from_req`）会**立刻创建
/// 所有提取器的 future**，然后在一趟 poll 里按参数顺序逐个 poll，遇到
/// **第一个 `Ready(Err)` 就返回** —— `Pending` 不阻断后面的提取器：
///
/// ```ignore
/// match fut.poll(cx) {
///     Poll::Ready(Err(err)) => return Poll::Ready(Err(err.into())),
///     Poll::Pending => ready = false,     // ← 继续 poll 后面的
/// }
/// ```
///
/// 于是「守卫写在第一个参数」并不能保证它先失败：`RequirePermission` 需要 await DB
/// （首轮 `Pending`），而 `Json<T>` 的反序列化若已就绪并返回 400，**400 会抢先**。
/// 实测 62 个「`RequirePermission` + `Json` body」的 handler 里 55 个守卫都已在
/// 第一位，照样发生。
///
/// 后果：未授权调用方能靠 **400 vs 403** 的差异探测端点是否存在、以及它期望什么
/// body schema。授权本身没被绕过（合法 body 下仍 403），但这是不必要的暴露，
/// 也让"未授权"在客户端看起来像"请求格式错误"。
///
/// 修法是 `pre_resolve_auth` 中间件提前把 `AuthContext` 放进请求 extensions，
/// 守卫命中缓存后返回 `std::future::ready(...)`，**首轮 poll 就就绪**。
///
/// **如果有人"清理"掉这个看似多余的中间件，本测试会红** —— 这就是它存在的意义。
/// 行为级验证（403/401 优先于 400）见 `scripts/` 的 HTTP e2e 与交付说明。
#[test]
fn admin_scope_pre_resolves_auth_so_403_beats_400() {
    let routes = std::fs::read_to_string(repo_root().join("apps/api/src/bootstrap/routes.rs"))
        .expect("read routes.rs");

    // 只看**非注释行**：注释里解释这套机制是正常的，不能算作"已挂载"。
    let code_lines: Vec<&str> = routes
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect();
    let joined = code_lines.join("\n");
    let admin_scope_has_middleware = joined.contains("pre_resolve_auth")
        && joined.contains("from_fn")
        && joined.contains("/admin");

    assert!(
        admin_scope_has_middleware,
        "/admin scope 必须挂 `pre_resolve_auth` 中间件。\n\
         去掉它会让授权错误(403/401)重新被 body 解析错误(400)抢先，\n\
         从而允许未授权调用方探测端点存在性与 body schema。\n\
         详见 api::extractor::auth::pre_resolve_auth 的说明。"
    );

    // 守卫这一侧也要有快路径：中间件放了缓存，守卫必须读它。
    let auth = std::fs::read_to_string(repo_root().join("apps/api/src/api/extractor/auth.rs"))
        .expect("read auth.rs");
    assert!(
        auth.contains("extensions().get::<AuthContext>()"),
        "`RequirePermission` / `RequireAny` 必须优先读取中间件放进 extensions 的 \
         `AuthContext`（快路径），否则它会退回 await DB 的慢路径，403 又会输给 400。"
    );
    assert!(
        auth.contains("std::future::ready"),
        "快路径必须返回 `std::future::ready(...)` —— 只有**同步就绪**的 future 才能让\
         授权错误抢在 body 解析错误之前。"
    );
}

// ============================================================================
// 5) 「未参加赛事 / 未加入队伍」必须是 **403**，不能是 400 或 404
// ============================================================================
//
// 同一个语义（已认证，但不是这场赛事的参与者）历史上在三个赛制里有三种答案：
//
// | 系统              | 曾经 | 现在 |
// |-------------------|------|------|
// | AWDP              | 403  | 403  |
// | AWD / 战队         | 404  | 403  |
// | common / jeopardy | 400  | 403  |
//
// 判定标准是 `api::extractor::auth` 开头那张表：**403 = 已认证但不允许**，
// 「对象级授权失败」明确列在其中。400 是「请求参数有误」、404 是「资源不存在」，
// 两者都会让客户端把授权问题归错类 —— 尤其前端的 `onForbidden` 分支
// （保留令牌 + 提示无权限）会永远收不到它。
//
// 与「赛事尚未开始」的区别：同一函数里紧挨着的两条检查必须保持不同 ——
// 状态问题 400，授权问题 403。因此本测试既守 403，也守那条 400 没被一起改坏。
#[test]
fn not_joined_participant_is_forbidden_not_bad_request() {
    let root = repo_root();

    let player_service = std::fs::read_to_string(
        root.join("apps/api/src/modules/event/common/application/player_service.rs"),
    )
    .expect("read player_service.rs");

    assert!(
        player_service.contains(r#"AppError::Forbidden("你尚未加入本赛事".to_string())"#),
        "「未参加赛事」必须返回 403（AppError::Forbidden）"
    );
    assert!(
        !player_service.contains(r#"AppError::BadRequest("你尚未加入本赛事".to_string())"#),
        "「未参加赛事」不许退回 400 —— 它是授权问题，不是请求参数问题"
    );
    assert!(
        player_service.contains(r#"AppError::BadRequest("赛事尚未开始".to_string())"#),
        "「赛事尚未开始」是状态问题，必须保持 400（别把两类问题一起改了）"
    );

    for (path, needle) in [
        (
            "apps/api/src/modules/event/awd/api/player.rs",
            "你尚未加入本赛事的队伍",
        ),
        (
            "apps/api/src/modules/event/common/application/team_service.rs",
            "你尚未加入本赛事的队伍",
        ),
    ] {
        let src = std::fs::read_to_string(root.join(path)).expect(path);
        assert!(
            src.contains(&format!(r#"Forbidden("{needle}"#)),
            "{path}: 「{needle}」必须是 403"
        );
        assert!(
            !src.contains(&format!(r#"NotFound("{needle}"#)),
            "{path}: 「{needle}」不许是 404 —— 赛事对已认证用户并非「不存在」，只是他不在其中"
        );
    }

    let ctx = std::fs::read_to_string(
        root.join("apps/api/src/modules/event/jeopardy/application/context.rs"),
    )
    .expect("read context.rs");
    assert!(
        ctx.contains(r#"anyhow!(AppError::Forbidden("你尚未加入本赛事".to_string()))"#),
        "jeopardy 的 should_user_joined 必须把 Forbidden 包进 anyhow —— \
         否则它会在 API 边界被一刀切的 map_err 压成 400"
    );

    let app_error = std::fs::read_to_string(root.join("apps/api/src/api/app_error.rs"))
        .expect("read app_error.rs");
    assert!(
        app_error.contains("pub fn app_err(") && app_error.contains("e.downcast::<AppError>()"),
        "API 边界必须有 app_err（downcast 取回 anyhow 里的 AppError）—— \
         否则「未参赛」的 403 会在最后一跳被抹成 400"
    );
}
