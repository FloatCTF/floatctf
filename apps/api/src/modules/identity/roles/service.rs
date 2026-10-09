//! 角色/权限变更的安全内核（Wave 5）。
//!
//! 本模块集中三件事，因此它们**不可能**被某条变更路径漏掉：
//! 1. [`EscalationGuard`] —— 反提权与自锁死判定（纯函数，可单测）；
//! 2. [`lock_super_admin_role`] —— 「最后一个超管」的串行化锁点；
//! 3. [`assert_not_last_super_admin`] —— 锁内计数断言。

use sea_orm::{ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, Statement};
use uuid::Uuid;

use crate::api::AppError;
use crate::api::extractor::auth::AuthContext;
use crate::core::permission::PermissionSet;
use crate::core::system_ids::PLATFORM_ROLE_SUPER_ADMIN;
use crate::entity::{platform_role_permissions, platform_roles, user_platform_roles};

/// 反提权与自锁死判定。
///
/// 规则（计划 §12 修订 6）：**before 与 after 都必须 ⊆ actor 的权限集**。
/// 只查「新增的权限」会被绕过：攻击者可以先扩充一个自己已持有的角色的权限，
/// 或利用目标原有权限已超出的情况做反向操纵。
pub struct EscalationGuard<'a> {
    actor: &'a AuthContext,
}

impl<'a> EscalationGuard<'a> {
    pub fn new(actor: &'a AuthContext) -> Self {
        Self { actor }
    }

    /// actor 是否全权（`grants_all`）。全权 actor 不受 ⊆ 约束
    /// （它本来就拥有全部权限，唯一的额外限制是自锁死与最后一个超管）。
    fn actor_is_all_powerful(&self) -> bool {
        self.actor.grants_all
    }

    fn assert_subset(&self, subject: &PermissionSet, what: &str) -> Result<(), AppError> {
        if self.actor_is_all_powerful() {
            return Ok(());
        }
        if !subject.is_subset_of(&self.actor.permissions) {
            let exceeded: Vec<String> = subject
                .iter()
                .filter(|p| !self.actor.permissions.contains(*p))
                .map(|p| p.as_str().to_string())
                .collect();
            return Err(AppError::Forbidden(format!(
                "反提权拒绝：{what} 超出你自身的权限（{}）。\n\
                 你不能授予或保留自己没有的权限。",
                exceeded.join(", ")
            )));
        }
        Ok(())
    }

    /// 角色编辑：before 与 after 都必须 ⊆ actor 的权限。
    pub fn check_role_permissions(
        &self,
        before: &PermissionSet,
        after: &PermissionSet,
    ) -> Result<(), AppError> {
        self.assert_subset(before, "该角色**变更前**的权限集")?;
        self.assert_subset(after, "该角色**变更后**的权限集")?;
        Ok(())
    }

    /// 账号角色变更：目标变更前后的有效权限都必须 ⊆ actor 的权限。
    pub fn check_target_permissions(
        &self,
        before: &PermissionSet,
        after: &PermissionSet,
    ) -> Result<(), AppError> {
        self.assert_subset(before, "目标账号**变更前**的有效权限")?;
        self.assert_subset(after, "目标账号**变更后**的有效权限")?;
        Ok(())
    }

    /// `grants_all` 目标保护。
    ///
    /// 非全权 actor **不得**改动全权持有者的角色集合。否则一个只有 `roles.manage`
    /// 的角色就能把超管的角色清空 —— 等价于接管平台。
    pub fn check_target_not_grants_all(&self, target_grants_all: bool) -> Result<(), AppError> {
        if target_grants_all && !self.actor_is_all_powerful() {
            return Err(AppError::Forbidden(
                "你不能修改全权账号（super-admin）的角色集合".into(),
            ));
        }
        Ok(())
    }

    /// 自锁死防护：actor 不得通过本次操作移除自己的管理能力。
    ///
    /// 判据是「操作后是否还留有 `roles.manage`」：这是唯一能再次修改角色的权限，
    /// 一旦失去就无法自助恢复（需要另一个管理员或直接改库）。
    pub fn check_self_lockout(&self, after: &PermissionSet) -> Result<(), AppError> {
        if self.actor_is_all_powerful() {
            // 全权 actor 的自锁死由 `assert_not_last_super_admin` 兜住：
            // 只要还有别的超管，它把自己降权是合法且常见的操作。
            return Ok(());
        }
        if !after.contains(crate::core::permission::Permission::RolesManage) {
            return Err(AppError::Forbidden(
                "自锁死防护：这会让**你自己**失去 roles.manage，之后无法再修改角色/权限。\n\
                 请先由另一位管理员授予你该权限，或保留至少一个持有它的角色。"
                    .into(),
            ));
        }
        Ok(())
    }
}

/// 取「super-admin 角色行」的行锁（**串行化点**）。
///
/// 所有可能影响「是否还存在超管」的操作都必须先调用它，于是这些操作
/// 彼此串行，「计数 → 写入」之间不再有交错窗口。
///
/// 锁在**固定主键的内置角色行**上，而不是「计数结果」：固定行永远存在，
/// 不会出现「锁一个不存在的行」或幻读。
///
/// ## 为什么不用 `BEFORE` 行级触发器
///
/// 计划 §12 修订 3 明确否决了触发器方案：
/// - 触发器无法表达**操作者意图**，会把「先清空再重建」这类合法批量流程一并拒绝；
/// - 触发器抛错时事务已部分执行，错误信息难以区分成因；
/// - 触发器无法返回「你是谁、还剩几个」这类可行动信息，运维体验很差。
///
/// 应用层事务 + 行锁能给出准确、可解释、可测试的失败。
pub async fn lock_super_admin_role<C: ConnectionTrait>(db: &C) -> Result<(), AppError> {
    db.execute(Statement::from_string(
        DbBackend::Postgres,
        format!(
            "SELECT id FROM platform_roles WHERE id = '{PLATFORM_ROLE_SUPER_ADMIN}' FOR UPDATE"
        ),
    ))
    .await?;
    Ok(())
}

/// 当前持有「全权」角色的**账号数**（去重：同一账号可能持有多个全权角色）。
pub async fn count_all_powerful_accounts<C: ConnectionTrait>(db: &C) -> Result<i64, AppError> {
    let row = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(DISTINCT upr.user_id)::bigint AS n
               FROM user_platform_roles upr
               JOIN platform_roles r ON r.id = upr.role_id
              WHERE r.grants_all"
                .to_string(),
        ))
        .await?
        .ok_or_else(|| AppError::Internal("计数查询未返回行".into()))?;
    Ok(row.try_get::<i64>("", "n").unwrap_or(0))
}

/// 断言「写入之后仍然至少存在一个全权账号」。
///
/// **必须在持锁的事务内、写入之后调用**：锁保证没有别的操作能在这个
/// 「写入 → 计数」窗口里并发修改角色授予。
pub async fn assert_not_last_super_admin<C: ConnectionTrait>(db: &C) -> Result<(), AppError> {
    let remaining = count_all_powerful_accounts(db).await?;
    if remaining < 1 {
        return Err(AppError::Forbidden(
            "最后一个超管保护：该操作会让平台不再有任何全权账号，从而无法恢复。\n\
             请先给另一个账号授予 super-admin 角色。"
                .into(),
        ));
    }
    Ok(())
}

/// 某账号的角色列表。
pub async fn roles_of_user<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<platform_roles::Model>, AppError> {
    let role_ids: Vec<Uuid> = user_platform_roles::Entity::find()
        .filter(user_platform_roles::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.role_id)
        .collect();
    if role_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(platform_roles::Entity::find()
        .filter(platform_roles::Column::Id.is_in(role_ids))
        .all(db)
        .await?)
}

/// 某账号当前的**有效权限**（多角色并集；全权短路为全集）。
pub async fn effective_permissions<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<PermissionSet, AppError> {
    let role_ids: Vec<Uuid> = user_platform_roles::Entity::find()
        .filter(user_platform_roles::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.role_id)
        .collect();
    let (set, _unknown) =
        crate::modules::identity::authorization::permissions_of_roles(db, &role_ids).await?;
    Ok(set)
}

/// 某账号是否持有全权角色。
pub async fn user_has_grants_all<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let n = db
        .query_one(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT count(*)::bigint AS n
                   FROM user_platform_roles upr
                   JOIN platform_roles r ON r.id = upr.role_id
                  WHERE r.grants_all AND upr.user_id = '{user_id}'"
            ),
        ))
        .await?
        .ok_or_else(|| AppError::Internal("计数查询未返回行".into()))?;
    Ok(n.try_get::<i64>("", "n").unwrap_or(0) > 0)
}

/// 按一组**将要授予**的角色计算有效权限（不落库）。
///
/// 用于在写入之前就完成反提权判断 —— 若先写后查，一旦判断失败就得回滚，
/// 且「失败前已经写进去」的窗口会让审计日志与实际不符。
pub async fn simulated_effective_permissions<C: ConnectionTrait>(
    db: &C,
    roles: &[platform_roles::Model],
) -> Result<PermissionSet, AppError> {
    if roles.iter().any(|r| r.grants_all) {
        return Ok(PermissionSet::all());
    }
    let role_ids: Vec<Uuid> = roles.iter().map(|r| r.id).collect();
    let permissions = platform_role_permissions::Entity::find()
        .filter(platform_role_permissions::Column::RoleId.is_in(role_ids))
        .all(db)
        .await?;
    let (set, _unknown) =
        PermissionSet::from_strings(permissions.into_iter().map(|p| p.permission));
    Ok(set)
}

/// actor 在「某角色权限集被替换为 `after`」之后的有效权限。
pub async fn permissions_after_role_change<C: ConnectionTrait>(
    db: &C,
    actor_id: Uuid,
    changed_role_id: Uuid,
    after: &PermissionSet,
) -> Result<(PermissionSet, bool), AppError> {
    let roles = roles_of_user(db, actor_id).await?;
    let holds_changed = roles.iter().any(|r| r.id == changed_role_id);
    if !holds_changed {
        let current = effective_permissions(db, actor_id).await?;
        return Ok((current, false));
    }

    // 其它角色贡献的权限
    let mut merged = PermissionSet::new();
    let other_ids: Vec<Uuid> = roles
        .iter()
        .filter(|r| r.id != changed_role_id)
        .map(|r| r.id)
        .collect();
    if !other_ids.is_empty() {
        let (set, _) =
            crate::modules::identity::authorization::permissions_of_roles(db, &other_ids).await?;
        merged.extend(&set);
    }
    merged.extend(after);
    // 被改角色若仍是全权角色，则结果仍是全集
    let still_grants_all = roles
        .iter()
        .find(|r| r.id == changed_role_id)
        .map(|r| r.grants_all)
        .unwrap_or(false);
    if still_grants_all {
        return Ok((PermissionSet::all(), true));
    }
    Ok((merged, false))
}

/// actor 在「某角色被删除」之后的有效权限。
pub async fn permissions_after_role_removal<C: ConnectionTrait>(
    db: &C,
    actor_id: Uuid,
    removed_role_id: Uuid,
) -> Result<(PermissionSet, bool), AppError> {
    let roles = roles_of_user(db, actor_id).await?;
    let other_ids: Vec<Uuid> = roles
        .iter()
        .filter(|r| r.id != removed_role_id)
        .map(|r| r.id)
        .collect();
    if other_ids.is_empty() {
        return Ok((PermissionSet::new(), false));
    }
    let grants_all_left = roles
        .iter()
        .any(|r| r.id != removed_role_id && r.grants_all);
    let (set, _) =
        crate::modules::identity::authorization::permissions_of_roles(db, &other_ids).await?;
    Ok((
        if grants_all_left {
            PermissionSet::all()
        } else {
            set
        },
        grants_all_left,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::permission::Permission;

    fn ctx(permissions: &[Permission], grants_all: bool) -> AuthContext {
        AuthContext {
            user: users_stub(),
            permissions: PermissionSet::from_permissions(permissions.iter().copied()),
            grants_all,
        }
    }

    /// 构造一个最小 `users::Model`：这些单测只读 `grants_all` 与 `permissions`。
    fn users_stub() -> crate::entity::users::Model {
        use crate::entity::sea_orm_active_enums::AccountStatus;
        crate::entity::users::Model {
            id: Uuid::new_v4(),
            username: "actor".into(),
            nickname: "actor".into(),
            password: String::new(),
            email: "actor@example.invalid".into(),
            avatar: None,
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
            status: AccountStatus::Active,
            token_version: 0,
        }
    }

    #[test]
    fn escalation_rejects_permission_the_actor_lacks() {
        let actor = ctx(&[Permission::RolesManage, Permission::UsersRead], false);
        let guard = EscalationGuard::new(&actor);

        let ok = PermissionSet::from_permissions([Permission::UsersRead]);
        let too_much =
            PermissionSet::from_permissions([Permission::UsersRead, Permission::TerminalOpen]);

        assert!(
            guard
                .check_role_permissions(&PermissionSet::new(), &ok)
                .is_ok()
        );
        assert!(
            guard
                .check_role_permissions(&PermissionSet::new(), &too_much)
                .is_err()
        );
    }

    /// **before 也必须是子集**：否则可以借「编辑一个权限超出的既有角色」提权。
    #[test]
    fn escalation_checks_before_as_well_as_after() {
        let actor = ctx(&[Permission::RolesManage], false);
        let guard = EscalationGuard::new(&actor);

        let beyond = PermissionSet::from_permissions([Permission::TerminalOpen]);
        let within = PermissionSet::from_permissions([Permission::RolesManage]);

        // before 超出 → 即使 after 收缩到合法范围也必须拒绝
        assert!(
            guard.check_role_permissions(&beyond, &within).is_err(),
            "before 超出 actor 权限时必须拒绝（否则可借既有超额角色提权）"
        );
        // after 超出 → 拒绝
        assert!(guard.check_role_permissions(&within, &beyond).is_err());
        // 两者都在范围内 → 允许
        assert!(guard.check_role_permissions(&within, &within).is_ok());
    }

    #[test]
    fn all_powerful_actor_is_not_subset_constrained() {
        let actor = ctx(&[], true);
        let guard = EscalationGuard::new(&actor);
        assert!(
            guard
                .check_role_permissions(&PermissionSet::all(), &PermissionSet::all())
                .is_ok()
        );
        assert!(
            guard
                .check_target_permissions(&PermissionSet::all(), &PermissionSet::all())
                .is_ok()
        );
    }

    #[test]
    fn non_all_powerful_actor_cannot_touch_all_powerful_target() {
        let actor = ctx(&[Permission::RolesManage], false);
        let guard = EscalationGuard::new(&actor);
        assert!(
            guard.check_target_not_grants_all(true).is_err(),
            "非全权 actor 不得改动全权账号的角色集合"
        );
        // 全权 actor 可以
        let root = ctx(&[], true);
        assert!(
            EscalationGuard::new(&root)
                .check_target_not_grants_all(true)
                .is_ok()
        );
        // 目标不是全权 → 允许
        assert!(guard.check_target_not_grants_all(false).is_ok());
    }

    #[test]
    fn self_lockout_is_refused_for_non_all_powerful_actor() {
        let actor = ctx(&[Permission::RolesManage], false);
        let guard = EscalationGuard::new(&actor);

        assert!(
            guard
                .check_self_lockout(&PermissionSet::from_permissions([Permission::UsersRead]))
                .is_err(),
            "失去 roles.manage 后无法自助恢复，必须拒绝"
        );
        assert!(
            guard
                .check_self_lockout(&PermissionSet::from_permissions([Permission::RolesManage]))
                .is_ok()
        );
    }

    /// 全权 actor 把自己降权是**合法**的：只要还有别的超管，
    /// 由 `assert_not_last_super_admin` 兜住「最后一个」这一情形。
    #[test]
    fn all_powerful_actor_may_demote_self() {
        let root = ctx(&[], true);
        assert!(
            EscalationGuard::new(&root)
                .check_self_lockout(&PermissionSet::new())
                .is_ok()
        );
    }
}
