//! Core primitives shared across modules: config, secrets, security, errors (later).

pub mod config;
pub mod secret;
pub mod security;

pub use config::AppConfig;
pub use secret::Secret;
