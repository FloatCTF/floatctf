//! OIDC HTTP 端点。

pub mod assets;
pub mod developer;
pub mod discovery;
pub mod logout;
pub mod pages;
pub mod protocol;
pub mod session;
pub mod web_session;

use actix_web::HttpResponse;

/// `[oidc].enabled = false`：协议端点不存在。
pub(crate) fn not_enabled() -> HttpResponse {
    HttpResponse::NotFound().json(crate::api::UniResponse::<()>::err(
        404,
        "接口不存在".to_string(),
    ))
}
