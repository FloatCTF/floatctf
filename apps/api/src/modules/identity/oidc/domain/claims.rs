//! ID Token 与 UserInfo claims（OIDC Core §2、§5.1、§5.4）。
//!
//! 只按已授予 scope 披露：`openid` → `sub`；`profile` → 昵称/用户名/头像/更新时间；
//! `email` → `email` + `email_verified=false`（平台无邮箱确认流程，绝不伪造 true）。
//! 永不包含口令散列、角色、管理员身份、战队或赛事信息。

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::scope;

/// 生成 claims 所需的最小用户快照（与持久化解耦）。
#[derive(Debug, Clone)]
pub struct UserSnapshot {
    pub id: uuid::Uuid,
    pub username: String,
    pub nickname: String,
    pub email: String,
    pub avatar: Option<String>,
    pub updated_at: i64,
}

/// ID Token claims：只含身份与认证会话信息。
///
/// profile / email 等资料 claims **只经 UserInfo 返回**（OIDC Core §5.4：签发了 access token 时，
/// scope 对应的 claims 由 UserInfo 提供）。ID Token 会在 RP 间传递、被记录，最小披露更安全；
/// OIDF 一致性测试 `EnsureIdTokenDoesNotContainEmailForScopeEmail` 同样要求如此。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub azp: String,
    pub exp: i64,
    pub iat: i64,
    pub auth_time: i64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub at_hash: Option<String>,
    /// 身份会话标识（Back-Channel Logout 1.0 §2.1）；部署开启标准退出且令牌来自浏览器会话时出现。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sid: Option<String>,
}

/// `at_hash`：access token ASCII 的 SHA-256 左半部分 base64url（RS256 → SHA-256，§3.1.3.6）。
pub fn at_hash(access_token: &str) -> String {
    let digest = Sha256::digest(access_token.as_bytes());
    URL_SAFE_NO_PAD.encode(&digest[..16])
}

/// 按 scope 生成用户 claims（不含 `sub`）。`issuer` 用于把站内相对头像路径变成绝对 URL。
fn scoped_user_claims(user: &UserSnapshot, scopes: &[String], issuer: &str) -> Map<String, Value> {
    let mut claims = Map::new();
    if scopes.iter().any(|s| s == scope::PROFILE) {
        claims.insert(
            "preferred_username".into(),
            Value::from(user.username.clone()),
        );
        claims.insert("nickname".into(), Value::from(user.nickname.clone()));
        claims.insert("name".into(), Value::from(user.nickname.clone()));
        if let Some(picture) = user
            .avatar
            .as_deref()
            .and_then(|a| absolute_picture(a, issuer))
        {
            claims.insert("picture".into(), Value::from(picture));
        }
        claims.insert("updated_at".into(), Value::from(user.updated_at));
    }
    if scopes.iter().any(|s| s == scope::EMAIL) {
        let email = user.email.trim();
        if !email.is_empty() {
            claims.insert("email".into(), Value::from(email.to_string()));
            claims.insert("email_verified".into(), Value::Bool(false));
        }
    }
    claims
}

/// UserInfo 响应体：`sub` + 按 scope 的 claims。
pub fn userinfo(user: &UserSnapshot, scopes: &[String], issuer: &str) -> Map<String, Value> {
    let mut body = Map::new();
    body.insert("sub".into(), Value::from(user.id.to_string()));
    body.extend(scoped_user_claims(user, scopes, issuer));
    body
}

fn absolute_picture(avatar: &str, issuer: &str) -> Option<String> {
    let avatar = avatar.trim();
    if avatar.starts_with("https://") || avatar.starts_with("http://") {
        Some(avatar.to_string())
    } else if avatar.starts_with('/') && !avatar.starts_with("//") {
        Some(format!("{issuer}{avatar}"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> UserSnapshot {
        UserSnapshot {
            id: uuid::Uuid::nil(),
            username: "alice".into(),
            nickname: "Alice".into(),
            email: "alice@example.com".into(),
            avatar: Some("/public/avatars/a.png".into()),
            updated_at: 1_700_000_000,
        }
    }

    fn scopes(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn openid_only_discloses_sub_only() {
        let body = userinfo(&user(), &scopes(&["openid"]), "https://ctf.example.com");
        assert_eq!(body.len(), 1);
        assert_eq!(body["sub"], "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn profile_and_email_are_scoped_and_email_not_verified() {
        let body = userinfo(
            &user(),
            &scopes(&["openid", "profile"]),
            "https://ctf.example.com",
        );
        assert_eq!(body["preferred_username"], "alice");
        assert_eq!(
            body["picture"],
            "https://ctf.example.com/public/avatars/a.png"
        );
        assert!(!body.contains_key("email"));

        let body = userinfo(
            &user(),
            &scopes(&["openid", "email"]),
            "https://ctf.example.com",
        );
        assert_eq!(body["email"], "alice@example.com");
        assert_eq!(body["email_verified"], false);
        assert!(!body.contains_key("preferred_username"));
    }

    #[test]
    fn never_leaks_sensitive_fields() {
        let body = userinfo(
            &user(),
            &scopes(&["openid", "profile", "email"]),
            "https://x",
        );
        for forbidden in ["password", "role", "admin", "team", "phone"] {
            assert!(body.keys().all(|k| !k.contains(forbidden)), "{forbidden}");
        }
    }

    #[test]
    fn empty_email_is_omitted_and_protocol_relative_avatar_ignored() {
        let mut u = user();
        u.email = "  ".into();
        u.avatar = Some("//evil.example/x.png".into());
        let body = userinfo(&u, &scopes(&["openid", "profile", "email"]), "https://x");
        assert!(!body.contains_key("email"));
        assert!(!body.contains_key("email_verified"));
        assert!(!body.contains_key("picture"));
    }

    #[test]
    fn at_hash_matches_oidc_definition() {
        // 与 OIDC Core 示例同法计算：SHA-256 左 128 bit 的 base64url。
        let h = at_hash("jHkWEdUXMU1BwAsC4vtUsZwnNvTIxEl0z9K3vx5KF0Y");
        assert_eq!(h.len(), 22);
        assert_eq!(h, at_hash("jHkWEdUXMU1BwAsC4vtUsZwnNvTIxEl0z9K3vx5KF0Y"));
        assert_ne!(h, at_hash("other"));
    }
}
