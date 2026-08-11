//! 安全原语：JWT，以及（预留）口令哈希等辅助。

pub mod jwt;

pub use jwt::{AuthClaims, Role, configure_jwt_secret, gen_jwt_token, validate_jwt};
