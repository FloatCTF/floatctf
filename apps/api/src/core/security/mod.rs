//! 安全原语：JWT，以及（预留）口令哈希等辅助。

pub mod jwt;

pub use jwt::{
    AuthClaims, PASSWORD_RESET_TTL_MINUTES, SESSION_TTL_MINUTES, TokenPurpose,
    configure_jwt_secret, gen_jwt_token, gen_password_reset_token, gen_session_token, validate_jwt,
};
