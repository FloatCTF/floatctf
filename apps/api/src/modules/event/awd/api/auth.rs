//! AWD API 鉴权辅助。

use crate::api::prelude::*;
use actix_web::{FromRequest, HttpRequest, web};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::future::Future;
use std::pin::Pin;

use crate::entity::awd_events;
use crate::infrastructure::WebDb;
use crate::modules::event::awd::crypto::AwdCrypto;

/// 标识发起请求的内部服务身份。
#[derive(Debug, Clone)]
pub enum AwdInternalPrincipal {
    FlagServer { event_id: uuid::Uuid },
    JudgeServer { event_id: uuid::Uuid },
}

/// 校验 AWD 内部服务认证的提取器。
///
/// 处理器中用法：
/// ```ignore
/// pub async fn my_handler(
///     auth: AwdInternalAuth,
///     // ...
/// ) -> UniResult<T> {
///     let event_id = auth.principal.event_id();
///     // ...
/// }
/// ```
pub struct AwdInternalAuth {
    pub principal: AwdInternalPrincipal,
}

impl AwdInternalPrincipal {
    pub fn event_id(&self) -> uuid::Uuid {
        match self {
            AwdInternalPrincipal::FlagServer { event_id } => *event_id,
            AwdInternalPrincipal::JudgeServer { event_id } => *event_id,
        }
    }
}

#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken,
    EventNotFound,
    CryptoError(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingToken => write!(f, "Missing or invalid Authorization header"),
            AuthError::InvalidToken => write!(f, "Unauthorized"),
            AuthError::EventNotFound => write!(f, "Unauthorized"),
            AuthError::CryptoError(_) => write!(f, "Unauthorized"),
        }
    }
}

impl actix_web::ResponseError for AuthError {
    fn error_response(&self) -> actix_web::HttpResponse {
        let body = serde_json::json!({
            "code": 401,
            "message": self.to_string(),
        });
        actix_web::HttpResponse::Unauthorized().json(body)
    }

    fn status_code(&self) -> actix_web::http::StatusCode {
        actix_web::http::StatusCode::UNAUTHORIZED
    }
}

impl FromRequest for AwdInternalAuth {
    type Error = AuthError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let db = req.app_data::<WebDb>().cloned();
        let path_event_id = req
            .match_info()
            .get("event_id")
            .and_then(|s| uuid::Uuid::parse_str(s).ok());

        // Extract bearer token before entering async block to avoid lifetime issues
        let token_bytes = extract_bearer_token(req);

        Box::pin(async move {
            let token_bytes = token_bytes?;
            let db = db.ok_or(AuthError::CryptoError("DB not available".into()))?;
            let path_event_id =
                path_event_id.ok_or(AuthError::CryptoError("Invalid event_id in path".into()))?;

            // Look up event
            let awd_event = awd_events::Entity::find()
                .filter(awd_events::Column::EventId.eq(path_event_id))
                .one(db.get_ref())
                .await
                .map_err(|e| AuthError::CryptoError(format!("DB error: {}", e)))?
                .ok_or(AuthError::EventNotFound)?;

            // Initialize crypto from the TOML-loaded application secret
            let crypto = AwdCrypto::from_config_secret()
                .map_err(|e| AuthError::CryptoError(e.to_string()))?;

            // Try flagserver token
            if let (Some(ciphertext), Some(nonce)) = (
                &awd_event.flagserver_token_ciphertext,
                &awd_event.flagserver_token_nonce,
            ) {
                if !ciphertext.is_empty()
                    && crypto
                        .is_valid_token(
                            &token_bytes,
                            ciphertext,
                            nonce,
                            path_event_id,
                            awd_event.key_version,
                        )
                        .map_err(|e| AuthError::CryptoError(e.to_string()))?
                {
                    return Ok(AwdInternalAuth {
                        principal: AwdInternalPrincipal::FlagServer {
                            event_id: path_event_id,
                        },
                    });
                }
            }

            // Try judgeserver token
            if let (Some(ciphertext), Some(nonce)) = (
                &awd_event.judgeserver_token_ciphertext,
                &awd_event.judgeserver_token_nonce,
            ) {
                if !ciphertext.is_empty()
                    && crypto
                        .is_valid_token(
                            &token_bytes,
                            ciphertext,
                            nonce,
                            path_event_id,
                            awd_event.key_version,
                        )
                        .map_err(|e| AuthError::CryptoError(e.to_string()))?
                {
                    return Ok(AwdInternalAuth {
                        principal: AwdInternalPrincipal::JudgeServer {
                            event_id: path_event_id,
                        },
                    });
                }
            }

            Err(AuthError::InvalidToken)
        })
    }
}

/// 从 Authorization: Bearer 头提取原始 token 字节。
fn extract_bearer_token(req: &HttpRequest) -> Result<Vec<u8>, AuthError> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .ok_or(AuthError::MissingToken)?
        .to_str()
        .map_err(|_| AuthError::MissingToken)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::MissingToken)?;

    if token.is_empty() {
        return Err(AuthError::MissingToken);
    }

    Ok(token.as_bytes().to_vec())
}
