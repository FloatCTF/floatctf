//! Discovery / RFC 8414 元数据 / JWKS。

use actix_web::{HttpResponse, get, http::header};

use super::not_enabled;
use crate::modules::identity::oidc::{WebOidc, domain::metadata};

/// 元数据与 JWKS 可被 RP 缓存；轮换 key 时新旧公钥会并存，缓存 5 分钟足够过渡。
const METADATA_CACHE: &str = "public, max-age=300";

/// GET /.well-known/openid-configuration
#[get("/.well-known/openid-configuration")]
pub async fn openid_configuration(oidc: Option<WebOidc>) -> HttpResponse {
    let Some(oidc) = oidc else {
        return not_enabled();
    };
    HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, METADATA_CACHE))
        .json(metadata::openid_configuration_with(
            oidc.issuer(),
            oidc.config.backchannel_logout,
        ))
}

/// GET /.well-known/oauth-authorization-server（RFC 8414）
#[get("/.well-known/oauth-authorization-server")]
pub async fn oauth_authorization_server(oidc: Option<WebOidc>) -> HttpResponse {
    let Some(oidc) = oidc else {
        return not_enabled();
    };
    HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, METADATA_CACHE))
        .json(metadata::oauth_authorization_server(
            oidc.issuer(),
            oidc.config.backchannel_logout,
        ))
}

/// GET /.well-known/jwks.json
#[get("/.well-known/jwks.json")]
pub async fn jwks(oidc: Option<WebOidc>) -> HttpResponse {
    let Some(oidc) = oidc else {
        return not_enabled();
    };
    HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, METADATA_CACHE))
        .json(oidc.keys.jwks())
}
