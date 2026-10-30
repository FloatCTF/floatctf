//! OIDC 纯领域逻辑（无 I/O）：参数解析与校验、PKCE、scope、claims、元数据、随机令牌。

pub mod authorize;
pub mod claims;
pub mod client_auth;
pub mod error;
pub mod metadata;
pub mod pkce;
pub mod redirect_uri;
pub mod scope;
pub mod secret;
