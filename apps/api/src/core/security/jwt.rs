//! JWT encode/decode and claims (technical layer).
//!
//! Business login/register/password-reset lives in `modules::identity`.
//! HTTP guards live in `api::extractor::auth`.

use crate::core::secret::Secret;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Process-wide JWT signing secret, installed once from `AppConfig` at startup.
static JWT_SECRET: OnceLock<Secret> = OnceLock::new();

/// Install the JWT secret from typed config (call once during bootstrap).
pub fn configure_jwt_secret(secret: Secret) {
    let _ = JWT_SECRET.set(secret);
}

fn jwt_secret_bytes() -> Vec<u8> {
    if let Some(s) = JWT_SECRET.get() {
        return s.as_bytes().to_vec();
    }
    // Tests / late callers: fall back to env once (not per-request after configure).
    let from_env = std::env::var("SECRET").expect("SECRET must be set in .env file!");
    let secret = Secret::new(from_env);
    let bytes = secret.as_bytes().to_vec();
    let _ = JWT_SECRET.set(secret);
    bytes
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum Role {
    User,
    SuperAdmin,
    ResetAccount,
    AwdJudger,
}

/// JWT claims payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: Uuid,
    pub role: Role,
    pub exp: usize,
}

pub fn gen_jwt_token(
    id: Uuid,
    role: Role,
    expire_mins: Option<usize>,
) -> Result<String, jsonwebtoken::errors::Error> {
    let secret = jwt_secret_bytes();

    // Expiration: minutes preferred, default 8 hours
    let expiration = if let Some(mins) = expire_mins {
        Utc::now()
            .checked_add_signed(Duration::minutes(mins as i64))
            .expect("valid timestamp")
            .timestamp() as usize
    } else {
        Utc::now()
            .checked_add_signed(Duration::hours(8))
            .expect("valid timestamp")
            .timestamp() as usize
    };

    let claims = AuthClaims {
        sub: id,
        role,
        exp: expiration,
    };

    encode(
        &Header::new(Algorithm::HS512),
        &claims,
        &EncodingKey::from_secret(&secret),
    )
}

pub fn validate_jwt(token: String) -> Result<AuthClaims, jsonwebtoken::errors::Error> {
    let secret = jwt_secret_bytes();
    let token_data = decode::<AuthClaims>(
        &token,
        &DecodingKey::from_secret(&secret),
        &Validation::new(Algorithm::HS512),
    )?;

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_roundtrip_user_role() {
        // SAFETY: tests set SECRET for encode/decode
        unsafe {
            std::env::set_var("SECRET", "floatctf-test-secret-for-jwt");
        }
        let id = Uuid::new_v4();
        let token = gen_jwt_token(id, Role::User, Some(30)).expect("encode");
        let claims = validate_jwt(token).expect("decode");
        assert_eq!(claims.sub, id);
        assert_eq!(claims.role, Role::User);
    }

    #[test]
    fn jwt_roundtrip_super_admin_role() {
        unsafe {
            std::env::set_var("SECRET", "floatctf-test-secret-for-jwt");
        }
        let id = Uuid::new_v4();
        let token = gen_jwt_token(id, Role::SuperAdmin, Some(30)).expect("encode");
        let claims = validate_jwt(token).expect("decode");
        assert_eq!(claims.sub, id);
        assert_eq!(claims.role, Role::SuperAdmin);
    }

    #[test]
    fn jwt_rejects_tampered_token() {
        unsafe {
            std::env::set_var("SECRET", "floatctf-test-secret-for-jwt");
        }
        let id = Uuid::new_v4();
        let mut token = gen_jwt_token(id, Role::User, Some(30)).expect("encode");
        token.push('x');
        assert!(validate_jwt(token).is_err());
    }
}
