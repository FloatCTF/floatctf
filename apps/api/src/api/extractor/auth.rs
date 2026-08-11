//! 鉴权相关请求提取器与守卫。

use crate::core::security::jwt::validate_jwt;
use crate::entity::{super_admin, users};
use crate::infrastructure::WebDb;
use actix_web::FromRequest;
use sea_orm::EntityTrait;

pub struct UserJwtGuard(users::Model);

impl UserJwtGuard {
    pub fn into_inner(self) -> users::Model {
        self.0
    }
}

pub struct SuperAdminJwtGuard(super_admin::Model);

impl SuperAdminJwtGuard {
    pub fn into_inner(self) -> super_admin::Model {
        self.0
    }
}

impl FromRequest for UserJwtGuard {
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let db = req.app_data::<WebDb>().cloned().unwrap();
        let auth_header = req.headers().get("Authorization").map(|h| h.clone());

        Box::pin(async move {
            if let Some(auth_header) = auth_header {
                let token = auth_header.to_str().unwrap_or("").to_string();
                if token.starts_with("Bearer ") {
                    let jwt = token.trim_start_matches("Bearer ").trim().to_string();
                    if let Ok(claims) = validate_jwt(jwt) {
                        if let Ok(Some(user)) = users::Entity::find_by_id(claims.sub)
                            .one(db.get_ref())
                            .await
                        {
                            return Ok(UserJwtGuard(user));
                        }
                    }
                }
            }

            Err(actix_web::error::ErrorUnauthorized(
                "Invalid or missing token, or contact the admin",
            ))
        })
    }
}

impl FromRequest for SuperAdminJwtGuard {
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let db = req.app_data::<WebDb>().cloned().unwrap();
        let auth_header = req.headers().get("Authorization").map(|h| h.clone());

        Box::pin(async move {
            if let Some(auth_header) = auth_header {
                let token = auth_header.to_str().unwrap_or("").to_string();
                if token.starts_with("Bearer ") {
                    let jwt = token.trim_start_matches("Bearer ").trim().to_string();
                    if let Ok(claims) = validate_jwt(jwt) {
                        if let Ok(Some(super_admin)) = super_admin::Entity::find_by_id(claims.sub)
                            .one(db.get_ref())
                            .await
                        {
                            return Ok(SuperAdminJwtGuard(super_admin));
                        }
                    }
                }
            }

            Err(actix_web::error::ErrorUnauthorized(
                "Invalid or missing token, or contact the admin",
            ))
        })
    }
}
