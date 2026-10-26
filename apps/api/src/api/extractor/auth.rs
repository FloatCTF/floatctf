//! 鉴权相关请求提取器与守卫。

use crate::core::security::jwt::{Role, validate_jwt};
use crate::entity::{super_admin, users};
use crate::infrastructure::WebDb;
use actix_web::FromRequest;
use actix_web::ResponseError;
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
        // 主站统一 Cookie 会话（`[oidc].unified_web_session`）：仅当请求**没有** Authorization 头时启用。
        // Bearer 与 Cookie 两种机制从不混用；Cookie 通道强制第一方来源 + CSRF，且只能得到普通用户身份。
        let web_session = req
            .app_data::<crate::modules::identity::oidc::WebOidc>()
            .filter(|oidc| oidc.config.unified_web_session)
            .cloned()
            .filter(|_| auth_header.is_none())
            .map(|oidc| (oidc, req.clone()));

        Box::pin(async move {
            if let Some(auth_header) = auth_header {
                let token = auth_header.to_str().unwrap_or("").to_string();
                if token.starts_with("Bearer ") {
                    let jwt = token.trim_start_matches("Bearer ").trim().to_string();
                    if let Ok(claims) = validate_jwt(jwt) {
                        if claims.role == Role::User {
                            if let Ok(Some(user)) = users::Entity::find_by_id(claims.sub)
                                .one(db.get_ref())
                                .await
                            {
                                return Ok(UserJwtGuard(user));
                            }
                        }
                    }
                }
            } else if let Some((oidc, req)) = web_session {
                match crate::modules::identity::oidc::accounts::resolve_web_user(
                    db.get_ref(),
                    &oidc,
                    &req,
                )
                .await
                {
                    Ok(Ok(user)) => return Ok(UserJwtGuard(user)),
                    Ok(Err(reason)) => {
                        tracing::debug!(?reason, path = %req.path(), "web session rejected")
                    }
                    Err(e) => tracing::error!(error = %e, "web session lookup failed"),
                }
            }

            // 统一走平台 JSON 信封（Actix 的 ErrorUnauthorized 只会返回纯文本）。
            Err(actix_web::error::InternalError::from_response(
                "unauthorized",
                crate::api::AppError::Unauthorized.error_response(),
            )
            .into())
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
                        if claims.role == Role::SuperAdmin {
                            if let Ok(Some(super_admin)) =
                                super_admin::Entity::find_by_id(claims.sub)
                                    .one(db.get_ref())
                                    .await
                            {
                                return Ok(SuperAdminJwtGuard(super_admin));
                            }
                        }
                    }
                }
            }

            // 统一走平台 JSON 信封（Actix 的 ErrorUnauthorized 只会返回纯文本）。
            Err(actix_web::error::InternalError::from_response(
                "unauthorized",
                crate::api::AppError::Unauthorized.error_response(),
            )
            .into())
        })
    }
}
