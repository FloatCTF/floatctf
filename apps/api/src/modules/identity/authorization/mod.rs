//! Authorization helpers and role vocabulary for identity.
//!
//! JWT `Role` encoding lives in `core::security::jwt`; this module re-exports
//! it for identity code and can host future policy checks.

pub use crate::core::security::jwt::{AuthClaims, Role};
