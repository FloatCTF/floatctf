//! 授权与权限校验。
//!
//! # 分层
//!
//! ```text
//! core::permission         权限目录（代码为权威）+ PermissionSet
//! identity::authorization  ← 本模块：角色 reconcile、权限解析原语
//! api::extractor::auth     请求侧守卫（AccountGuard / RequirePermission）
//! ```
//!
//! # 为什么内置角色要在启动时 reconcile
//!
//! `super-admin` 是**引导不变式**：系统必须永远存在至少一个能做任何事的账号，
//! 否则不可恢复。因此它的结构性字段（`protected` / `grants_all`）由代码在每次启动时
//! 校正回来，而不是只依赖一次 migration 插入——否则有人手工改库就会静默破坏不变式。
//!
//! `name` / `description` 是**展示字段**，允许运维修改，启动时**不**覆盖。
//!
//! # orphan 权限
//!
//! DB 里的权限字符串无法解析为 [`Permission`] 时（权限被重命名 / 回退版本 / 手工改库），
//! 本模块只 **WARN 并列出**，**不静默删除**：删除权限是安全相关动作，
//! 静默做等于「升级后某个角色悄悄少了权限」。

pub use crate::core::security::jwt::{AuthClaims, TokenPurpose};

use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, Set,
};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::core::permission::PermissionSet;
use crate::core::system_ids::{PLATFORM_ROLE_SUPER_ADMIN, PLATFORM_ROLE_SUPER_ADMIN_KEY};
use crate::entity::{platform_role_permissions, platform_roles};

/// 内置角色定义（**代码为权威**）。
#[derive(Debug, Clone, Copy)]
pub struct BuiltinRole {
    /// 固定主键（`core::system_ids` 常量），迁移与并发锁点都依赖它。
    pub id: Uuid,
    /// 稳定语义键。
    pub key: &'static str,
    /// 默认展示名（运维可改，启动时不覆盖）。
    pub name: &'static str,
    /// 默认说明（运维可改，启动时不覆盖）。
    pub description: &'static str,
    /// 是否全权（短路返回 [`crate::core::permission::Permission::ALL`]）。
    pub grants_all: bool,
}

/// 第一版**只内置 `super-admin`**。
///
/// 其它角色（event-manager / challenge-manager / user-manager / ops-manager / auditor）
/// 是**推荐模板**，由 UI 以普通角色创建（`protected = false`，可自由增删权限），
/// **不在 DB 里 seed**——内置意味着「不可删 + 权限集不可改」的永久义务，
/// 而组织分工因部署而异。
pub const BUILTIN_ROLES: &[BuiltinRole] = &[BuiltinRole {
    id: PLATFORM_ROLE_SUPER_ADMIN,
    key: PLATFORM_ROLE_SUPER_ADMIN_KEY,
    name: "Super Admin",
    description: "平台全权角色：拥有全部权限（含将来新增的权限）。系统内置，不可删除。",
    grants_all: true,
}];

/// 内置角色 reconcile 的结果（供启动日志与测试断言）。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BuiltinRoleSync {
    /// 本次**新建**的内置角色键。
    pub created: Vec<&'static str>,
    /// 已存在、本次未改动结构字段的内置角色键。
    pub unchanged: Vec<&'static str>,
    /// 本次**修正**了结构字段（protected / grants_all）的内置角色键。
    pub repaired: Vec<&'static str>,
    /// 无法解析的权限字符串：`(role_key, permission)`。**只报告，不删除。**
    pub orphan_permissions: Vec<(String, String)>,
    /// 键冲突等无法自动修复的问题（需要人工介入）。
    pub conflicts: Vec<String>,
}

impl BuiltinRoleSync {
    /// 是否存在需要人工介入的问题。
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }
}

/// 幂等 reconcile 内置角色，并报告 orphan 权限。
///
/// 语义：
/// 1. 缺失 → 插入（`protected = true`，`grants_all` 按定义）；
/// 2. 存在但结构字段被改动 → 校正回 `protected = true` 与定义的 `grants_all`；
/// 3. 存在且结构字段正确 → **完全不动**（连 `updated_at` 都不碰，避免每次启动都写库）；
/// 4. 同 key 但不同 id → 记入 `conflicts`（**不**覆盖、不删除，交人工决策）；
/// 5. `name` / `description` **从不**被覆盖（展示字段允许运维修改）。
pub async fn ensure_builtin_roles<C: ConnectionTrait>(db: &C) -> BuiltinRoleSync {
    let mut report = BuiltinRoleSync::default();

    for role in BUILTIN_ROLES {
        // 同 key 不同 id = 主键冲突，无法自动修复
        match platform_roles::Entity::find()
            .filter(platform_roles::Column::Key.eq(role.key))
            .one(db)
            .await
        {
            Ok(Some(existing)) if existing.id != role.id => {
                report.conflicts.push(format!(
                    "内置角色 key '{}' 已被 id={} 占用（期望 id={}）；请人工处理，未自动覆盖",
                    role.key, existing.id, role.id
                ));
                continue;
            }
            Ok(_) => {}
            Err(e) => {
                error!(error = %e, key = role.key, "查询内置平台角色失败");
                report
                    .conflicts
                    .push(format!("查询角色 {} 失败: {e}", role.key));
                continue;
            }
        }

        let existing = platform_roles::Entity::find_by_id(role.id).one(db).await;

        let existing = match existing {
            Ok(value) => value,
            Err(e) => {
                error!(error = %e, key = role.key, "查询内置平台角色失败");
                report
                    .conflicts
                    .push(format!("查询角色 {} 失败: {e}", role.key));
                continue;
            }
        };

        match existing {
            None => {
                let insert = platform_roles::Entity::insert(platform_roles::ActiveModel {
                    id: Set(role.id),
                    key: Set(role.key.to_string()),
                    name: Set(role.name.to_string()),
                    description: Set(role.description.to_string()),
                    protected: Set(true),
                    grants_all: Set(role.grants_all),
                    ..Default::default()
                })
                .on_conflict_do_nothing()
                .exec(db)
                .await;

                match insert {
                    Ok(_) => {
                        report.created.push(role.key);
                        info!(key = role.key, "已创建内置平台角色");
                    }
                    Err(e) => {
                        error!(error = %e, key = role.key, "创建内置平台角色失败");
                        report
                            .conflicts
                            .push(format!("创建角色 {} 失败: {e}", role.key));
                    }
                }
            }
            Some(model) => {
                let structural_ok = model.protected && model.grants_all == role.grants_all;
                if structural_ok {
                    report.unchanged.push(role.key);
                } else {
                    // 只修结构字段；name/description 保持运维的改动。
                    // 加 filter 让「值本来就对」的情况下不产生 UPDATE（避免 updated_at 抖动）。
                    let repaired = platform_roles::Entity::update_many()
                        .col_expr(platform_roles::Column::Protected, Expr::value(true))
                        .col_expr(
                            platform_roles::Column::GrantsAll,
                            Expr::value(role.grants_all),
                        )
                        .filter(platform_roles::Column::Id.eq(role.id))
                        .filter(
                            Condition::any()
                                .add(platform_roles::Column::Protected.ne(true))
                                .add(platform_roles::Column::GrantsAll.ne(role.grants_all)),
                        )
                        .exec(db)
                        .await;

                    match repaired {
                        Ok(_) => {
                            report.repaired.push(role.key);
                            warn!(
                                key = role.key,
                                "内置平台角色的结构字段被改动，已在启动时校正（protected/grants_all）"
                            );
                        }
                        Err(e) => {
                            error!(error = %e, key = role.key, "校正内置平台角色失败");
                            report
                                .conflicts
                                .push(format!("校正角色 {} 失败: {e}", role.key));
                        }
                    }
                }
            }
        }
    }

    report.orphan_permissions = detect_orphan_permissions(db).await;
    if !report.orphan_permissions.is_empty() {
        warn!(
            count = report.orphan_permissions.len(),
            "检测到无法解析的权限字符串（不自动删除；请人工确认后清理或恢复对应代码）"
        );
        for (role_key, permission) in &report.orphan_permissions {
            warn!(role = %role_key, permission = %permission, "orphan permission");
        }
    }
    for conflict in &report.conflicts {
        error!(%conflict, "内置平台角色 reconcile 需要人工介入");
    }

    report
}

/// 扫描 `platform_role_permissions`，返回无法解析为 [`crate::core::permission::Permission`]
/// 的 `(role_key, permission)` 列表（按 role_key 排序，输出确定性）。
pub async fn detect_orphan_permissions<C: ConnectionTrait>(db: &C) -> Vec<(String, String)> {
    let roles = match platform_roles::Entity::find().all(db).await {
        Ok(rows) => rows,
        Err(e) => {
            error!(error = %e, "扫描 orphan 权限时读取 platform_roles 失败");
            return Vec::new();
        }
    };
    let key_by_id: std::collections::HashMap<Uuid, String> =
        roles.into_iter().map(|r| (r.id, r.key)).collect();

    let rows = match platform_role_permissions::Entity::find().all(db).await {
        Ok(rows) => rows,
        Err(e) => {
            error!(error = %e, "扫描 orphan 权限时读取 platform_role_permissions 失败");
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for row in rows {
        if crate::core::permission::Permission::parse(&row.permission).is_none() {
            let role_key = key_by_id
                .get(&row.role_id)
                .cloned()
                .unwrap_or_else(|| row.role_id.to_string());
            out.push((role_key, row.permission));
        }
    }
    out.sort();
    out
}

/// 解析一组角色为**有效权限集合**（多角色取并集）。
///
/// 返回 `(集合, orphan 权限字符串)`：
/// - 任一角色带 `grants_all` → 直接返回全集（短路，不再读权限行）；
/// - 否则读 `platform_role_permissions` 并取并集；
/// - 无法解析的字符串进入第二个返回值（调用方决定 WARN 还是拒绝）。
///
/// 这是「权限每请求从 DB 解析」的唯一入口；第一版**不做缓存**。
pub async fn permissions_of_roles<C: ConnectionTrait>(
    db: &C,
    role_ids: &[Uuid],
) -> Result<(PermissionSet, Vec<String>), sea_orm::DbErr> {
    if role_ids.is_empty() {
        return Ok((PermissionSet::new(), Vec::new()));
    }

    // 短路：任一 grants_all → 全集
    let grants_all_count = platform_roles::Entity::find()
        .filter(platform_roles::Column::Id.is_in(role_ids.to_vec()))
        .filter(platform_roles::Column::GrantsAll.eq(true))
        .count(db)
        .await?;
    if grants_all_count > 0 {
        return Ok((PermissionSet::all(), Vec::new()));
    }

    let rows = platform_role_permissions::Entity::find()
        .filter(platform_role_permissions::Column::RoleId.is_in(role_ids.to_vec()))
        .all(db)
        .await?;

    let (set, unknown) = PermissionSet::from_strings(rows.into_iter().map(|r| r.permission));
    Ok((set, unknown))
}

/// 读取某账号的角色 id 列表（`user_platform_roles`）。
pub async fn role_ids_of_user<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<Uuid>, sea_orm::DbErr> {
    use crate::entity::user_platform_roles;
    let rows = user_platform_roles::Entity::find()
        .filter(user_platform_roles::Column::UserId.eq(user_id))
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|r| r.role_id).collect())
}
