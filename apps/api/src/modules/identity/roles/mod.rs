//! 平台角色与权限管理 API（Wave 5）。
//!
//! # 端点
//!
//! | 方法 | 路径 | 所需权限 |
//! |---|---|---|
//! | GET | `/api/admin/permissions` | `roles.read` |
//! | GET | `/api/admin/roles` | `roles.read` |
//! | POST | `/api/admin/roles` | `roles.manage` |
//! | PATCH | `/api/admin/roles/{role_id}` | `roles.manage` |
//! | DELETE | `/api/admin/roles/{role_id}` | `roles.manage` |
//! | GET | `/api/admin/users/{user_id}/roles` | `roles.read` |
//! | PUT | `/api/admin/users/{user_id}/roles` | `roles.manage` |
//!
//! # 三条不可协商的安全规则
//!
//! ## 1. 反提权（revision 6：before **与** after 都要 ⊆ actor 的权限）
//!
//! 只检查「请求里新增的权限」是不够的：攻击者可以先给角色加权限、再给自己授角色，
//! 或者利用「目标**原有**权限已经超出 actor」的情况反向操纵。因此：
//!
//! - **角色编辑**：请求前后的权限集都必须 ⊆ `actor.permissions`；
//!   即 actor 不能编辑一个自己都无法完全掌控的角色，也不能把它改成超出自己的样子。
//! - **用户角色变更**：目标变更前后的**有效权限**都必须 ⊆ `actor.permissions`。
//! - **`grants_all` 目标保护**：非全权 actor **不得**改动全权持有者的角色集合
//!   （否则一个只有 `roles.manage` 的角色就能把超管的角色清空，等于接管平台）。
//!
//! ## 2. 最后一个超管：应用层事务 + `SELECT ... FOR UPDATE`
//!
//! 计数检查与写入之间必须有串行化点，否则两个并发请求各自看到「还有 2 个超管」
//! 然后各删一个，结果是 0 个。这里**不**用行级触发器：
//!
//! - 触发器无法表达「操作者的意图」，会把合法的批量重建流程一起拒绝；
//! - 触发器抛错时事务已部分执行，错误信息也难以区分成因；
//! - 触发器无法返回「你是谁、还剩几个」这类可行动信息。
//!
//! 实现：所有可能影响「是否存在超管」的操作，先对**固定主键的内置角色行**
//! `SELECT ... FOR UPDATE` 取锁，于是「计数 → 写入」成为一个串行区间。
//! 锁点选择固定行而不是「计数结果」，避免间隙锁与幻读问题。
//!
//! ## 3. 自锁死防护
//!
//! actor 不得通过任何一次操作移除**自己**的管理能力（撤销自己的最后一个
//! `roles.manage` / 把自己从超管角色里摘掉）。否则一次误操作就能把平台锁在门外。

pub mod service;

use std::str::FromStr;

use actix_web::web::{Json, Path};
use sea_orm::{
    ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use uuid::Uuid;

use crate::api::extractor::auth::RequirePermission;
use crate::api::prelude::*;
use crate::core::permission::guards::{RolesManageGuard, RolesReadGuard};
use crate::core::permission::{Permission, PermissionDomain, PermissionSet};
use crate::entity::{platform_role_permissions, platform_roles, user_platform_roles, users};

use service::{EscalationGuard, assert_not_last_super_admin, lock_super_admin_role};

// ============================================================================
// DTO
// ============================================================================

#[derive(Debug, Serialize)]
pub struct PermissionDto {
    pub key: String,
    pub domain: String,
    pub description: String,
    pub high_risk: bool,
}

#[derive(Debug, Serialize)]
pub struct RoleDto {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    /// 系统内置：不可删除，且 `key` / `grants_all` / 权限集不可改。
    pub protected: bool,
    /// 全权标记（`grants_all`）—— 拥有全部权限，含将来新增的权限。
    pub grants_all: bool,
    /// 显式列出的权限（`grants_all` 角色下为**空**：它的权限来自短路，不是行集合）。
    pub permissions: Vec<String>,
    /// 被授予该角色的账号数（删除前的影响面提示）。
    pub user_count: i64,
}

#[derive(Debug, Serialize)]
pub struct RoleSummaryDto {
    pub id: Uuid,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct UserRolesDto {
    pub user_id: Uuid,
    pub username: String,
    pub roles: Vec<RoleSummaryDto>,
    /// 有效权限（多角色并集；全权角色下为全集）。
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoleRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchRoleRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// `Some(..)` = 替换整个权限集（不是增量）；`None` = 不动。
    #[serde(default)]
    pub permissions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct SetUserRolesRequest {
    pub role_ids: Vec<Uuid>,
}

// ============================================================================
// 权限目录
// ============================================================================

/// GET `/api/admin/permissions`
///
/// 返回**代码**里的 29 条权限目录（含域、说明、是否高危）。
/// 权威在 `core::permission::Permission`，不读 DB —— 因此不存在「目录与代码漂移」。
#[get("")]
pub async fn list_permissions(
    _: RequirePermission<RolesReadGuard>,
) -> UniResult<Vec<PermissionDto>> {
    let items: Vec<PermissionDto> = Permission::ALL
        .iter()
        .map(|p| PermissionDto {
            key: p.as_str().to_string(),
            domain: p.domain().as_str().to_string(),
            description: p.description().to_string(),
            high_risk: p.high_risk(),
        })
        .collect();
    UniResponse::ok(Some(items)).into()
}

/// GET `/api/admin/permissions/domains` —— 域列表（UI 分组用）。
#[get("/domains")]
pub async fn list_permission_domains(
    _: RequirePermission<RolesReadGuard>,
) -> UniResult<Vec<String>> {
    UniResponse::ok(Some(
        PermissionDomain::ALL
            .iter()
            .map(|d| d.as_str().to_string())
            .collect(),
    ))
    .into()
}

// ============================================================================
// 角色
// ============================================================================

/// 把一组权限字符串解析为集合；**未知字符串直接拒绝**（写路径必须校验）。
fn parse_permissions(raw: &[String]) -> Result<PermissionSet, AppError> {
    let (set, unknown) = PermissionSet::from_strings(raw);
    if !unknown.is_empty() {
        return Err(AppError::Validation(format!(
            "包含未知权限: {}（权限目录以代码为准，请核对 /api/admin/permissions）",
            unknown.join(", ")
        )));
    }
    Ok(set)
}

fn validate_role_key(key: &str) -> Result<(), AppError> {
    let key = key.trim();
    if key.is_empty() || key.len() > 64 {
        return Err(AppError::Validation("角色 key 长度必须为 1..=64".into()));
    }
    let mut chars = key.chars();
    let first = chars.next().expect("non-empty");
    if !first.is_ascii_lowercase() {
        return Err(AppError::Validation(
            "角色 key 必须以小写字母开头（^[a-z][a-z0-9-]{1,63}$）".into(),
        ));
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(AppError::Validation(
            "角色 key 只允许小写字母、数字与连字符".into(),
        ));
    }
    if key.len() < 2 {
        return Err(AppError::Validation("角色 key 至少 2 个字符".into()));
    }
    Ok(())
}

/// GET `/api/admin/roles`
#[get("")]
pub async fn list_roles(
    _: RequirePermission<RolesReadGuard>,
    ctx: ReqCtx,
) -> UniResult<Vec<RoleDto>> {
    let roles = platform_roles::Entity::find()
        .order_by_asc(platform_roles::Column::Key)
        .all(ctx.db.get_ref())
        .await?;

    let mut items = Vec::with_capacity(roles.len());
    for role in roles {
        let permissions = platform_role_permissions::Entity::find()
            .filter(platform_role_permissions::Column::RoleId.eq(role.id))
            .all(ctx.db.get_ref())
            .await?
            .into_iter()
            .map(|p| p.permission)
            .collect::<Vec<_>>();
        let mut permissions = permissions;
        permissions.sort();

        let user_count = user_platform_roles::Entity::find()
            .filter(user_platform_roles::Column::RoleId.eq(role.id))
            .count(ctx.db.get_ref())
            .await? as i64;

        items.push(RoleDto {
            id: role.id,
            key: role.key,
            name: role.name,
            description: role.description,
            protected: role.protected,
            grants_all: role.grants_all,
            permissions,
            user_count,
        });
    }

    UniResponse::ok(Some(items)).into()
}

/// POST `/api/admin/roles`
#[post("")]
pub async fn create_role(
    actor: RequirePermission<RolesManageGuard>,
    ctx: ReqCtx,
    body: Json<CreateRoleRequest>,
) -> UniResult<RoleDto> {
    let actor = actor.into_auth();
    let body = body.into_inner();

    validate_role_key(&body.key)?;
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("角色名称不能为空".into()));
    }
    let requested = parse_permissions(&body.permissions)?;

    // 反提权：新建角色的权限集必须 ⊆ actor 的权限集
    EscalationGuard::new(&actor).check_role_permissions(&PermissionSet::new(), &requested)?;

    let txn = ctx.db.get_ref().begin().await?;

    let role_id = Uuid::new_v4();
    platform_roles::Entity::insert(platform_roles::ActiveModel {
        id: Set(role_id),
        key: Set(body.key.trim().to_string()),
        name: Set(body.name.trim().to_string()),
        description: Set(body.description.unwrap_or_default()),
        // 自定义角色**永远**不是 protected、**永远**不是 grants_all：
        // 全权只能由迁移/bootstrap 建立（否则 UI 就能造出第二个全权角色）
        protected: Set(false),
        grants_all: Set(false),
        ..Default::default()
    })
    .exec(&txn)
    .await?;

    for permission in requested.iter() {
        platform_role_permissions::Entity::insert(platform_role_permissions::ActiveModel {
            role_id: Set(role_id),
            permission: Set(permission.as_str().to_string()),
        })
        .exec(&txn)
        .await?;
    }

    txn.commit().await?;

    ctx.log
        .add_log(
            "INFO",
            "ROLES",
            "CREATE",
            format!("{} 创建角色: {}", actor.username(), body.key).as_str(),
            json!({"role_id": role_id, "permissions": requested.to_strings()}),
            None,
            actor.id().into(),
            Some(&ctx.req),
        )
        .await;

    let role = platform_roles::Entity::find_by_id(role_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("{role_id} 不存在")))?;

    UniResponse::ok(Some(RoleDto {
        id: role.id,
        key: role.key,
        name: role.name,
        description: role.description,
        protected: role.protected,
        grants_all: role.grants_all,
        permissions: requested.to_strings(),
        user_count: 0,
    }))
    .into()
}

/// PATCH `/api/admin/roles/{role_id}`
///
/// 内置角色（`protected`）只允许改 `name` / `description`（展示字段）：
/// `key`、`grants_all`、权限集都是**结构性**的，改了就会破坏引导不变式。
#[patch("/{role_id}")]
pub async fn patch_role(
    actor: RequirePermission<RolesManageGuard>,
    ctx: ReqCtx,
    role_id: Path<Uuid>,
    body: Json<PatchRoleRequest>,
) -> UniResult<RoleDto> {
    let actor = actor.into_auth();
    let role_id = role_id.into_inner();
    let body = body.into_inner();

    let txn = ctx.db.get_ref().begin().await?;
    // 与「最后一个超管」共用同一把锁：编辑内置角色也可能影响引导不变式
    lock_super_admin_role(&txn).await?;

    let role = platform_roles::Entity::find_by_id(role_id)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("角色 {role_id} 不存在")))?;

    let before: PermissionSet = platform_role_permissions::Entity::find()
        .filter(platform_role_permissions::Column::RoleId.eq(role_id))
        .all(&txn)
        .await?
        .into_iter()
        .map(|p| p.permission)
        .collect::<Vec<_>>()
        .iter()
        .filter_map(|raw| Permission::from_str(raw).ok())
        .collect();

    let after = match &body.permissions {
        Some(raw) => parse_permissions(raw)?,
        None => before.clone(),
    };

    if role.protected {
        if body.permissions.is_some() {
            return Err(AppError::Forbidden(
                "内置角色的权限集不可修改（它是引导不变式的一部分）".into(),
            ));
        }
        if role.grants_all {
            // 全权角色的展示名可改，但没有任何权限维度的可改项
        }
    }

    // 反提权：before 与 after 都必须 ⊆ actor 的权限
    EscalationGuard::new(&actor).check_role_permissions(&before, &after)?;

    let mut active = role.into_active_model();
    if let Some(name) = body.name {
        if name.trim().is_empty() {
            return Err(AppError::Validation("角色名称不能为空".into()));
        }
        active.name = Set(name.trim().to_string());
    }
    if let Some(description) = body.description {
        active.description = Set(description);
    }
    platform_roles::Entity::update(active).exec(&txn).await?;

    if body.permissions.is_some() {
        platform_role_permissions::Entity::delete_many()
            .filter(platform_role_permissions::Column::RoleId.eq(role_id))
            .exec(&txn)
            .await?;
        for permission in after.iter() {
            platform_role_permissions::Entity::insert(platform_role_permissions::ActiveModel {
                role_id: Set(role_id),
                permission: Set(permission.as_str().to_string()),
            })
            .exec(&txn)
            .await?;
        }
    }

    // 自锁死：如果 actor 持有这个角色，改完后 actor 自己的权限不能变空
    let actor_holds_role = user_platform_roles::Entity::find()
        .filter(user_platform_roles::Column::UserId.eq(actor.id()))
        .filter(user_platform_roles::Column::RoleId.eq(role_id))
        .one(&txn)
        .await?
        .is_some();
    if actor_holds_role && body.permissions.is_some() {
        let (actor_after, _) =
            service::permissions_after_role_change(&txn, actor.id(), role_id, &after).await?;
        EscalationGuard::new(&actor).check_self_lockout(&actor_after)?;
    }

    assert_not_last_super_admin(&txn).await?;
    txn.commit().await?;

    ctx.log
        .add_log(
            "INFO",
            "ROLES",
            "UPDATE",
            format!("{} 更新角色: {}", actor.username(), role_id).as_str(),
            json!({"role_id": role_id, "permissions": after.to_strings()}),
            None,
            actor.id().into(),
            Some(&ctx.req),
        )
        .await;

    let role = platform_roles::Entity::find_by_id(role_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("角色 {role_id} 不存在")))?;
    let user_count = user_platform_roles::Entity::find()
        .filter(user_platform_roles::Column::RoleId.eq(role_id))
        .count(ctx.db.get_ref())
        .await? as i64;

    UniResponse::ok(Some(RoleDto {
        id: role.id,
        key: role.key,
        name: role.name,
        description: role.description,
        protected: role.protected,
        grants_all: role.grants_all,
        permissions: after.to_strings(),
        user_count,
    }))
    .into()
}

/// DELETE `/api/admin/roles/{role_id}`
#[delete("/{role_id}")]
pub async fn delete_role(
    actor: RequirePermission<RolesManageGuard>,
    ctx: ReqCtx,
    role_id: Path<Uuid>,
) -> UniResult<()> {
    let actor = actor.into_auth();
    let role_id = role_id.into_inner();

    let txn = ctx.db.get_ref().begin().await?;
    lock_super_admin_role(&txn).await?;

    let role = platform_roles::Entity::find_by_id(role_id)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("角色 {role_id} 不存在")))?;

    if role.protected {
        return Err(AppError::Forbidden("系统内置角色不可删除".into()));
    }

    // 反提权：删除一个权限超出 actor 的角色同样是提权路径
    // （否则可以删掉「能管我」的角色，再重建一个权限更低的同名角色）
    let before: PermissionSet = platform_role_permissions::Entity::find()
        .filter(platform_role_permissions::Column::RoleId.eq(role_id))
        .all(&txn)
        .await?
        .into_iter()
        .map(|p| p.permission)
        .collect::<Vec<_>>()
        .iter()
        .filter_map(|raw| Permission::from_str(raw).ok())
        .collect();
    EscalationGuard::new(&actor).check_role_permissions(&before, &PermissionSet::new())?;

    // 自锁死：删掉自己持有的角色会让 actor 失去管理能力
    let actor_holds_role = user_platform_roles::Entity::find()
        .filter(user_platform_roles::Column::UserId.eq(actor.id()))
        .filter(user_platform_roles::Column::RoleId.eq(role_id))
        .one(&txn)
        .await?
        .is_some();
    if actor_holds_role {
        let (actor_after, _) =
            service::permissions_after_role_removal(&txn, actor.id(), role_id).await?;
        EscalationGuard::new(&actor).check_self_lockout(&actor_after)?;
    }

    platform_roles::Entity::delete_by_id(role_id)
        .exec(&txn)
        .await?;

    assert_not_last_super_admin(&txn).await?;
    txn.commit().await?;

    ctx.log
        .add_log(
            "WARN",
            "ROLES",
            "DELETE",
            format!("{} 删除角色: {}", actor.username(), role.key).as_str(),
            json!({"role_id": role_id, "role_key": role.key}),
            None,
            actor.id().into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

// ============================================================================
// 账号 ↔ 角色
// ============================================================================

/// GET `/api/admin/users/{user_id}/roles`
#[get("/{user_id}/roles")]
pub async fn get_user_roles(
    _: RequirePermission<RolesReadGuard>,
    ctx: ReqCtx,
    user_id: Path<Uuid>,
) -> UniResult<UserRolesDto> {
    let user_id = user_id.into_inner();
    let user = users::Entity::find_by_id(user_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("账号 {user_id} 不存在")))?;

    let roles = service::roles_of_user(ctx.db.get_ref(), user_id).await?;
    let permissions = service::effective_permissions(ctx.db.get_ref(), user_id).await?;

    UniResponse::ok(Some(UserRolesDto {
        user_id,
        username: user.username,
        roles: roles
            .into_iter()
            .map(|r| RoleSummaryDto {
                id: r.id,
                key: r.key,
                name: r.name,
            })
            .collect(),
        permissions: permissions.to_strings(),
    }))
    .into()
}

/// PUT `/api/admin/users/{user_id}/roles`
///
/// **整体替换**目标账号的角色集合（不是增量）：增量语义下「先加后删」与
/// 「先删后加」会得到不同结果，容易造出短暂的权限空洞或超额授权窗口。
#[put("/{user_id}/roles")]
pub async fn set_user_roles(
    actor: RequirePermission<RolesManageGuard>,
    ctx: ReqCtx,
    user_id: Path<Uuid>,
    body: Json<SetUserRolesRequest>,
) -> UniResult<UserRolesDto> {
    let actor = actor.into_auth();
    let user_id = user_id.into_inner();
    let body = body.into_inner();

    let txn = ctx.db.get_ref().begin().await?;
    // ★ 串行化点：一切可能移除最后一个超管的操作都先取这把锁
    lock_super_admin_role(&txn).await?;

    let target = users::Entity::find_by_id(user_id)
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("账号 {user_id} 不存在")))?;

    // 目标角色必须都存在（避免创建出悬挂授予）
    let mut requested_roles = Vec::new();
    for role_id in &body.role_ids {
        let role = platform_roles::Entity::find_by_id(*role_id)
            .one(&txn)
            .await?
            .ok_or_else(|| AppError::Validation(format!("角色 {role_id} 不存在")))?;
        requested_roles.push(role);
    }

    // 变更前的有效权限
    let before = service::effective_permissions(&txn, user_id).await?;
    let before_grants_all =
        requested_roles.is_empty() && service::user_has_grants_all(&txn, user_id).await?;

    let guard = EscalationGuard::new(&actor);

    // grants_all 目标保护：非全权 actor 不得改动全权持有者的角色集合
    guard.check_target_not_grants_all(before_grants_all)?;

    // 变更后的有效权限（按请求计算，不落库）
    let after = service::simulated_effective_permissions(&txn, &requested_roles).await?;

    // 反提权：before 与 after 都必须 ⊆ actor 的权限
    guard.check_target_permissions(&before, &after)?;

    // 自锁死：actor 不得把自己改到失去管理能力
    if user_id == actor.id() {
        guard.check_self_lockout(&after)?;
    }

    // 写入：整体替换
    user_platform_roles::Entity::delete_many()
        .filter(user_platform_roles::Column::UserId.eq(user_id))
        .exec(&txn)
        .await?;
    for role in &requested_roles {
        user_platform_roles::Entity::insert(user_platform_roles::ActiveModel {
            user_id: Set(user_id),
            role_id: Set(role.id),
            granted_by: Set(Some(actor.id())),
            ..Default::default()
        })
        .exec(&txn)
        .await?;
    }

    // ★ 最后一个超管保护（在锁内计数）
    assert_not_last_super_admin(&txn).await?;

    txn.commit().await?;

    ctx.log
        .add_log(
            "WARN",
            "ROLES",
            "SET_USER_ROLES",
            format!(
                "{} 设置账号 {} 的角色: {:?}",
                actor.username(),
                target.username,
                requested_roles
                    .iter()
                    .map(|r| r.key.clone())
                    .collect::<Vec<_>>()
            )
            .as_str(),
            json!({
                "user_id": user_id,
                "roles": requested_roles.iter().map(|r| r.key.clone()).collect::<Vec<_>>(),
                "permissions_before": before.to_strings(),
                "permissions_after": after.to_strings(),
            }),
            None,
            actor.id().into(),
            Some(&ctx.req),
        )
        .await;

    let roles = service::roles_of_user(ctx.db.get_ref(), user_id).await?;
    let permissions = service::effective_permissions(ctx.db.get_ref(), user_id).await?;

    UniResponse::ok(Some(UserRolesDto {
        user_id,
        username: target.username,
        roles: roles
            .into_iter()
            .map(|r| RoleSummaryDto {
                id: r.id,
                key: r.key,
                name: r.name,
            })
            .collect(),
        permissions: permissions.to_strings(),
    }))
    .into()
}

// ============================================================================
// 路由注册
// ============================================================================
//
// 三个 configure 函数对应三个 scope：`/api/admin/roles`、`/api/admin/permissions`、
// `/api/admin/users`。必须分开注册 —— `#[get("")]` 是**相对路径**，
// 把 list_roles 与 list_permissions 放进同一个 scope 会造成路由重复定义。

/// 挂在 `/api/admin/roles` 下。
pub fn configure_role_routes(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.service(list_roles);
    cfg.service(create_role);
    cfg.service(patch_role);
    cfg.service(delete_role);
}

/// 挂在 `/api/admin/permissions` 下。
pub fn configure_permission_routes(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.service(list_permissions);
    cfg.service(list_permission_domains);
}

/// 挂在 `/api/admin/users` 下（账号 ↔ 角色）。
pub fn configure_user_role_routes(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.service(get_user_roles);
    cfg.service(set_user_roles);
}
