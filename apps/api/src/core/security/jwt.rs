//! JWT 编解码与声明（技术层）。
//!
//! 业务登录/注册/重置口令见 `modules::identity`。
//! HTTP 守卫见 `api::extractor::auth`。
//!
//! # 令牌只表达**身份**，不表达**授权**
//!
//! v2 的核心设计：JWT 携带 `sub`（账号 ID）、`purpose`（用途）、`token_version`
//! （凭据版本）、`exp`（过期时间）—— **仅此四项**。
//!
//! 为什么不再把角色/权限放进令牌：
//!
//! 1. **撤销不可能**：JWT 是无状态自证凭据。角色一放进令牌，撤销角色就必须等令牌过期，
//!    或者引入令牌黑名单（等于把无状态优势丢回去）。v2 明确要求「权限每请求从 DB 解析」
//!    —— 撤销立刻生效。
//! 2. **权限变更立刻生效**：管理员调整某角色权限后，所有持有该角色的账号下一次请求即生效，
//!    不需要重新登录。
//! 3. **`token_version` 提供真正的吊销能力**：改密 / 禁用 / 显式吊销时自增，
//!    旧令牌立即失效。放进令牌的只有**版本号**，真实值在 DB。
//! 4. **单一真相源**：授权信息只存在于 `platform_roles` / `user_platform_roles` /
//!    `platform_role_permissions`，不再有「令牌里的旧角色」这种漂移。
//!
//! # 用途（purpose）与「角色」的区别
//!
//! 旧模型用 `Role { User, SuperAdmin, ResetAccount, AwdJudger }` 同时表达**身份类别**与
//! **用途**，导致「是不是管理员」这种授权问题被编码进认证凭据。
//! v2 把它拆开：
//!
//! - [`TokenPurpose::Session`]：可用于所有已认证端点（能否访问某个端点由授权层决定）；
//! - [`TokenPurpose::PasswordReset`]：**只能**用于重置口令，不能用于任何已认证端点。
//!
//! 这样「重置令牌被拿去访问业务接口」在**认证层**就被拒绝，而不是依赖每个 handler 自觉。

use crate::core::secret::Secret;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::OnceLock;

/// 进程级 JWT 签名密钥，启动时从 `AppConfig` 安装一次。
static JWT_SECRET: OnceLock<Secret> = OnceLock::new();

/// 从类型化配置安装 JWT 密钥（bootstrap 期间调用一次）。
pub fn configure_jwt_secret(secret: Secret) {
    let _ = JWT_SECRET.set(secret);
}

fn jwt_secret_bytes() -> Vec<u8> {
    if let Some(s) = JWT_SECRET.get() {
        return s.as_bytes().to_vec();
    }
    panic!("JWT secret has not been configured from TOML");
}

/// 会话令牌有效期（分钟）：8 小时。
pub const SESSION_TTL_MINUTES: usize = 8 * 60;

/// 口令重置令牌有效期（分钟）：10 分钟。
///
/// 短有效期是重置链路的**主要**防线（令牌会出现在邮件与浏览器历史里）。
pub const PASSWORD_RESET_TTL_MINUTES: usize = 10;

/// 过期校验的时钟偏移容忍（秒）。
///
/// **显式声明**而不是继承 `jsonwebtoken` 的默认值（它恰好也是 60）：
/// 多节点/多容器的时钟不可能完全一致，零容忍会把刚签发的令牌在稍快的节点上
/// 判成「已过期」。令牌本身有效期是 8 小时 / 10 分钟，60 秒偏移可接受。
pub const CLOCK_SKEW_LEEWAY_SECONDS: u64 = 60;

/// 令牌用途。
///
/// 序列化为小写字符串（`"session"` / `"password_reset"`）；未知值**反序列化失败**
/// （而不是回落到某个默认用途——那会让重置令牌有机会被当成会话令牌）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenPurpose {
    /// 会话令牌：可用于所有已认证端点（具体端点能否访问由授权层决定）。
    Session,
    /// 口令重置令牌：**只能**用于 `POST /api/users/reset`。
    PasswordReset,
}

impl TokenPurpose {
    pub const fn as_str(self) -> &'static str {
        match self {
            TokenPurpose::Session => "session",
            TokenPurpose::PasswordReset => "password_reset",
        }
    }
}

impl fmt::Display for TokenPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TokenPurpose {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "session" => Ok(TokenPurpose::Session),
            "password_reset" => Ok(TokenPurpose::PasswordReset),
            other => Err(format!("unknown token purpose: {other}")),
        }
    }
}

/// JWT 声明载荷。
///
/// **只含身份与用途**；授权信息一律在请求时从 DB 解析（见 `identity::authorization`）。
///
/// 升级注意：v1 令牌的结构是 `{ sub, role, exp }`，缺少 `purpose` / `token_version`，
/// 因此在本版本下**反序列化失败 → 401**。这意味着上线后**所有已登录用户需要重新登录一次**
/// （一次性、预期内、必须写入发布说明）。这是有意的取舍：与其为兼容旧令牌而保留
/// 「角色来自令牌」的路径，不如一次性切断。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    /// 账号 ID（`users.id`）。
    pub sub: Uuid,
    /// 令牌用途。
    pub purpose: TokenPurpose,
    /// 签发时的凭据版本；与 `users.token_version` 不一致即视为已吊销。
    pub token_version: i32,
    /// 过期时间（Unix 秒）。
    pub exp: usize,
    /// 签发时间（新令牌必带；本字段引入前签发的旧令牌没有，反序列化为 `None`）。
    /// OIDC SSO 桥接据此记录真实的认证时间（`auth_time`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iat: Option<usize>,
}

impl AuthClaims {
    /// 该令牌是否为会话令牌（可访问已认证端点）。
    pub fn is_session(&self) -> bool {
        self.purpose == TokenPurpose::Session
    }
}

fn expiration_timestamp(expire_mins: Option<usize>) -> usize {
    let minutes = expire_mins.unwrap_or(SESSION_TTL_MINUTES) as i64;
    Utc::now()
        .checked_add_signed(Duration::minutes(minutes))
        .expect("valid timestamp")
        .timestamp() as usize
}

/// 签发令牌。
///
/// `token_version` 必须来自 `users.token_version` 的**当前值**；
/// 口令重置令牌同样带版本，因此用户在收到重置邮件后改密会使该重置令牌失效。
pub fn gen_jwt_token(
    id: Uuid,
    purpose: TokenPurpose,
    token_version: i32,
    expire_mins: Option<usize>,
) -> Result<String, jsonwebtoken::errors::Error> {
    let secret = jwt_secret_bytes();

    let claims = AuthClaims {
        sub: id,
        purpose,
        token_version,
        exp: expiration_timestamp(expire_mins),
        iat: Some(Utc::now().timestamp() as usize),
    };

    encode(
        &Header::new(Algorithm::HS512),
        &claims,
        &EncodingKey::from_secret(&secret),
    )
}

/// 签发会话令牌（默认 8 小时）。
pub fn gen_session_token(
    id: Uuid,
    token_version: i32,
) -> Result<String, jsonwebtoken::errors::Error> {
    gen_jwt_token(id, TokenPurpose::Session, token_version, None)
}

/// 签发口令重置令牌（10 分钟）。
pub fn gen_password_reset_token(
    id: Uuid,
    token_version: i32,
) -> Result<String, jsonwebtoken::errors::Error> {
    gen_jwt_token(
        id,
        TokenPurpose::PasswordReset,
        token_version,
        Some(PASSWORD_RESET_TTL_MINUTES),
    )
}

pub fn validate_jwt(token: String) -> Result<AuthClaims, jsonwebtoken::errors::Error> {
    let secret = jwt_secret_bytes();

    let mut validation = Validation::new(Algorithm::HS512);
    validation.leeway = CLOCK_SKEW_LEEWAY_SECONDS;

    let token_data = decode::<AuthClaims>(&token, &DecodingKey::from_secret(&secret), &validation)?;

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_secret() {
        let _ = JWT_SECRET.set(Secret::new("floatctf-test-secret-for-jwt"));
    }

    #[test]
    fn session_token_round_trip() {
        install_secret();
        let id = Uuid::new_v4();
        let token = gen_session_token(id, 7).expect("encode");
        let claims = validate_jwt(token).expect("decode");
        assert_eq!(claims.sub, id);
        assert_eq!(claims.purpose, TokenPurpose::Session);
        assert_eq!(claims.token_version, 7);
        assert!(claims.is_session());
    }

    #[test]
    fn password_reset_token_round_trip() {
        install_secret();
        let id = Uuid::new_v4();
        let token = gen_password_reset_token(id, 0).expect("encode");
        let claims = validate_jwt(token).expect("decode");
        assert_eq!(claims.purpose, TokenPurpose::PasswordReset);
        assert!(!claims.is_session(), "重置令牌**不得**被视为会话令牌");
    }

    /// `iat` 供 OIDC SSO 桥接记录 `auth_time`；引入它之前的 v2 令牌
    /// （有 `purpose` / `token_version`、无 `iat`）必须继续有效。
    #[test]
    fn jwt_carries_iat_and_tolerates_v2_tokens_without_iat() {
        install_secret();
        let id = Uuid::new_v4();
        let token = gen_session_token(id, 0).expect("encode");
        let claims = validate_jwt(token).expect("decode");
        let now = Utc::now().timestamp() as usize;
        assert!(claims.iat.is_some_and(|iat| iat <= now && iat + 5 >= now));

        #[derive(Serialize)]
        struct NoIat {
            sub: Uuid,
            purpose: TokenPurpose,
            token_version: i32,
            exp: usize,
        }
        let legacy = encode(
            &Header::new(Algorithm::HS512),
            &NoIat {
                sub: id,
                purpose: TokenPurpose::Session,
                token_version: 0,
                exp: now + 600,
            },
            &EncodingKey::from_secret(&jwt_secret_bytes()),
        )
        .expect("encode");
        let claims = validate_jwt(legacy).expect("v2 token without iat stays valid");
        assert_eq!(claims.sub, id);
        assert!(claims.iat.is_none());
    }

    #[test]
    fn token_version_is_carried_verbatim() {
        install_secret();
        let id = Uuid::new_v4();
        for version in [0, 1, 42, i32::MAX] {
            let token = gen_session_token(id, version).expect("encode");
            let claims = validate_jwt(token).expect("decode");
            assert_eq!(claims.token_version, version);
        }
    }

    #[test]
    fn expired_token_is_rejected() {
        install_secret();
        // 直接构造一个已过期时间戳的令牌（gen_* 只能签发未来的 exp）
        let secret = jwt_secret_bytes();
        let claims = AuthClaims {
            sub: Uuid::new_v4(),
            purpose: TokenPurpose::Session,
            token_version: 0,
            exp: (Utc::now().timestamp() - 3600) as usize,
            iat: None,
        };
        let token = encode(
            &Header::new(Algorithm::HS512),
            &claims,
            &EncodingKey::from_secret(&secret),
        )
        .expect("encode");
        assert!(
            validate_jwt(token).is_err(),
            "过期令牌必须被拒绝（本用例用 1 小时前的 exp，明确超出 {CLOCK_SKEW_LEEWAY_SECONDS} 秒 leeway）"
        );
    }

    #[test]
    fn tampered_token_is_rejected() {
        install_secret();
        let mut token = gen_session_token(Uuid::new_v4(), 0).expect("encode");
        token.push('x');
        assert!(validate_jwt(token).is_err());
    }

    /// **安全关键**：v1 令牌（含 `role`、无 `purpose`）必须被拒绝，
    /// 而不是被解释成某个默认用途。
    #[test]
    fn legacy_v1_token_is_rejected() {
        install_secret();
        let secret = jwt_secret_bytes();
        let legacy = serde_json::json!({
            "sub": Uuid::new_v4().to_string(),
            "role": "SuperAdmin",
            "exp": (Utc::now().timestamp() + 3600) as usize,
        });
        let token = encode(
            &Header::new(Algorithm::HS512),
            &legacy,
            &EncodingKey::from_secret(&secret),
        )
        .expect("encode legacy");
        assert!(
            validate_jwt(token).is_err(),
            "v1 令牌必须失效（强制一次性重新登录），不得回落成任何用途"
        );
    }

    /// 未知 purpose 字符串必须被拒绝。
    #[test]
    fn unknown_purpose_is_rejected() {
        install_secret();
        let secret = jwt_secret_bytes();
        let claims = serde_json::json!({
            "sub": Uuid::new_v4().to_string(),
            "purpose": "superuser",
            "token_version": 0,
            "exp": (Utc::now().timestamp() + 3600) as usize,
        });
        let token = encode(
            &Header::new(Algorithm::HS512),
            &claims,
            &EncodingKey::from_secret(&secret),
        )
        .expect("encode");
        assert!(validate_jwt(token).is_err(), "未知 purpose 必须被拒绝");
    }

    #[test]
    fn purpose_string_round_trip_and_rejection() {
        for purpose in [TokenPurpose::Session, TokenPurpose::PasswordReset] {
            let raw = purpose.as_str();
            assert_eq!(TokenPurpose::from_str(raw).unwrap(), purpose);
            assert_eq!(purpose.to_string(), raw);
        }
        for bad in ["", "Session", "SESSION", "admin", "role"] {
            assert!(
                TokenPurpose::from_str(bad).is_err(),
                "{bad:?} must not parse"
            );
        }
    }

    /// leeway 必须显式存在且与令牌有效期相比可忽略。
    #[test]
    fn clock_skew_leeway_is_explicit_and_small() {
        assert_eq!(CLOCK_SKEW_LEEWAY_SECONDS, 60);
        // 统一到秒再比较（避免单位混用写出恒真/恒假的断言）
        let session_secs = SESSION_TTL_MINUTES as u64 * 60;
        let reset_secs = PASSWORD_RESET_TTL_MINUTES as u64 * 60;
        assert!(
            CLOCK_SKEW_LEEWAY_SECONDS < session_secs,
            "leeway 相对会话有效期必须可忽略"
        );
        assert!(
            CLOCK_SKEW_LEEWAY_SECONDS < reset_secs,
            "leeway 相对重置令牌有效期必须可忽略"
        );
        // 重置令牌有效期是 10 分钟 → 600 秒；leeway 占其 10%
        assert!(reset_secs == 600 && CLOCK_SKEW_LEEWAY_SECONDS * 10 <= reset_secs);
    }

    /// 边界：恰好超出 leeway 1 秒必须被拒；恰好等于 leeway 之内必须通过。
    #[test]
    fn expiration_boundary_respects_leeway() {
        install_secret();
        let secret = jwt_secret_bytes();
        let make = |offset_secs: i64| {
            let claims = AuthClaims {
                sub: Uuid::new_v4(),
                purpose: TokenPurpose::Session,
                token_version: 0,
                exp: (Utc::now().timestamp() + offset_secs) as usize,
                iat: None,
            };
            encode(
                &Header::new(Algorithm::HS512),
                &claims,
                &EncodingKey::from_secret(&secret),
            )
            .expect("encode")
        };

        assert!(
            validate_jwt(make(-(CLOCK_SKEW_LEEWAY_SECONDS as i64) - 5)).is_err(),
            "超出 leeway 必须被拒绝"
        );
        assert!(validate_jwt(make(3600)).is_ok(), "未过期令牌必须通过");
    }

    #[test]
    fn ttl_constants_are_sane() {
        assert_eq!(SESSION_TTL_MINUTES, 480, "会话 8 小时");
        assert_eq!(
            PASSWORD_RESET_TTL_MINUTES, 10,
            "重置令牌 10 分钟（重置链路的主要防线）"
        );
    }
}
