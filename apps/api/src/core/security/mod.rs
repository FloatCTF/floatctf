//! Security primitives: JWT, (future) password hashing helpers, etc.

pub mod jwt;

pub use jwt::{AuthClaims, Role, configure_jwt_secret, gen_jwt_token, validate_jwt};
