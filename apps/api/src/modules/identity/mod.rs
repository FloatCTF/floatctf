//! Identity module — authentication, authorization, users, administrators.
//!
//! Technical JWT lives in `core::security::jwt`.
//! HTTP guards live in `api::extractor::auth`.

pub mod administrator;
pub mod authentication;
pub mod authorization;
pub mod user;

use actix_web::web::{self, ServiceConfig};

/// Player identity routes under `/api`:
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

/// Super-admin session under `/api`:
/// - POST `/admin/session`
pub fn configure_session_routes(cfg: &mut ServiceConfig) {
    cfg.service(administrator::super_admin_login);
}

/// Admin identity routes under `/api/admin`:
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
