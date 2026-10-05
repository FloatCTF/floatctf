//! 身份域——认证、授权、用户、管理员。

pub mod administrator;
pub mod authentication;
pub mod authorization;
pub mod user;

use actix_web::web::{self, ServiceConfig};

/// 选手身份路由（`/api` 下）：
/// - `/users/session`, `/users`, `/users/me`, reset flows
pub fn configure_player_routes(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/users")
            // POST /api/users/session
            .service(authentication::user_login)
            // POST /api/users
            .service(authentication::create_user)
            // GET /api/users/me
            .service(user::get_me)
            // PATCH /api/users/me
            .service(user::patch_me)
            // POST /api/users/reset_password
            .service(authentication::send_reset_email)
            // POST /api/users/reset?token=...
            .service(authentication::reset_password),
    );
}

/// 超管会话路由（`/api` 下）：
/// - POST `/admin/session`
pub fn configure_session_routes(cfg: &mut ServiceConfig) {
    cfg.service(administrator::super_admin_login);
}

/// 管理端身份路由（`/api/admin` 下）：
/// - `/users` CRUD
/// - `/super_admin` CRUD
pub fn configure_admin_routes(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/users")
            // POST /api/admin/users
            .service(user::admin_create_user)
            // DELETE /api/admin/users
            .service(user::admin_delete_user)
            // PATCH /api/admin/users/{user_id}
            .service(user::admin_patch_user)
            // GET /api/admin/users
            .service(user::admin_get_users)
            // GET /api/admin/users/{user_id}
            .service(user::admin_get_user),
    );

    cfg.service(
        web::scope("/super_admin")
            // POST /api/admin/super_admin
            .service(administrator::create_super_admin)
            // DELETE /api/admin/super_admin
            .service(administrator::delete_super_admin)
            // PATCH/POST /api/admin/super_admin/{super_admin_id}
            .service(administrator::patch_super_admin)
            // GET /api/admin/super_admin
            .service(administrator::get_super_admins)
            // GET /api/admin/super_admin/{super_admin_id}
            .service(administrator::get_super_admin),
    );
}

/// 账号字段校验（公开注册与管理端建号/改号共用）。
///
/// 历史问题：零校验会让空用户名/空密码账号落库，而空凭据可以直接登录成功。
pub(crate) fn validate_account_fields(
    username: &str,
    password: &str,
    email: &str,
) -> Result<(), crate::api::AppError> {
    use crate::api::AppError;

    let username = username.trim();
    if username.is_empty() {
        return Err(AppError::Validation("用户名不能为空".into()));
    }
    if username.chars().count() > 64 {
        return Err(AppError::Validation("用户名最长 64 个字符".into()));
    }
    if password.is_empty() {
        return Err(AppError::Validation("密码不能为空".into()));
    }
    if password.chars().count() < 8 {
        return Err(AppError::Validation("密码至少 8 位".into()));
    }
    let email = email.trim();
    if !email.is_empty() && !is_valid_email(email) {
        return Err(AppError::Validation("邮箱格式不正确".into()));
    }
    Ok(())
}

/// 极简邮箱校验：`local@domain.tld`（不追求 RFC 完备，只挡明显非法输入）。
fn is_valid_email(email: &str) -> bool {
    let mut parts = email.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::AppError;

    #[test]
    fn account_fields_reject_empty_username() {
        // 空用户名会落库成可直接登录的空凭据账号（历史缺陷）。
        assert!(matches!(
            validate_account_fields("", "Passw0rd@2025", "a@b.com"),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            validate_account_fields("   ", "Passw0rd@2025", "a@b.com"),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn account_fields_reject_weak_or_empty_password() {
        assert!(matches!(
            validate_account_fields("alice", "", "a@b.com"),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            validate_account_fields("alice", "short", "a@b.com"),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn account_fields_reject_malformed_email() {
        for email in ["not-an-email", "a@b", "a@.com", "@b.com", "a@@b.com"] {
            assert!(
                matches!(
                    validate_account_fields("alice", "Passw0rd@2025", email),
                    Err(AppError::Validation(_))
                ),
                "email {email} should be rejected"
            );
        }
        // 邮箱留空允许（管理端建号可不填邮箱）
        assert!(validate_account_fields("alice", "Passw0rd@2025", "").is_ok());
        assert!(validate_account_fields("alice", "Passw0rd@2025", "a@b.com").is_ok());
    }

    #[test]
    fn unique_violation_detection_matches_postgres_messages() {
        assert!(crate::api::is_unique_violation(
            "error returned from database: duplicate key value violates unique constraint \"users_username_key\""
        ));
        assert!(crate::api::is_unique_violation(
            "unique constraint violated"
        ));
        assert!(crate::api::is_unique_violation("SQLSTATE 23505"));
        assert!(!crate::api::is_unique_violation("connection refused"));
    }
}
