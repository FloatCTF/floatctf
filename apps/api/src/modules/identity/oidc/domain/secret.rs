//! 高熵随机值与单向摘要。
//!
//! - 授权码 / access token / SSO 会话 / 浏览器绑定 / CSRF：256-bit `OsRng` 随机，库内只存 SHA-256。
//! - client secret：256-bit 随机，库内存 Argon2 PHC 串（与玩家口令同一实现）。
//! - client_id：128-bit 随机（公开标识，不承担认证）。

use argon2::{
    Argon2, PasswordHash, PasswordVerifier,
    password_hash::{PasswordHasher, SaltString},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

/// 外部传入的令牌类字符串长度上限：超过直接拒绝，避免对超长输入做哈希/查询。
pub const MAX_TOKEN_INPUT_LEN: usize = 512;

/// `bytes` 字节 OS 随机数的 base64url（无填充）。
pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// SHA-256 十六进制摘要（令牌类字段入库形式）。
pub fn sha256_hex(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

pub fn new_client_id() -> String {
    format!("fctf_{}", random_token(16))
}

pub fn new_client_secret() -> String {
    format!("fcs_{}", random_token(32))
}

pub fn new_access_token() -> String {
    format!("fat_{}", random_token(32))
}

pub fn new_authorization_code() -> String {
    random_token(32)
}

/// SSO 会话、浏览器绑定、CSRF 等 256-bit 随机值。
pub fn new_opaque_secret() -> String {
    random_token(32)
}

/// 令牌类输入的形状校验：非空、长度受限、只含 base64url / `_` 前缀字符。
pub fn is_plausible_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TOKEN_INPUT_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
}

pub fn hash_client_secret(secret: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| format!("hash client secret: {e}"))
}

pub fn verify_client_secret(secret: &str, phc: &str) -> bool {
    if secret.is_empty() || secret.len() > MAX_TOKEN_INPUT_LEN {
        return false;
    }
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(secret.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_tokens_are_high_entropy_and_distinct() {
        let a = new_authorization_code();
        let b = new_authorization_code();
        assert_ne!(a, b);
        // 32 字节 → 43 个 base64url 字符（无填充）
        assert_eq!(a.len(), 43);
        assert!(is_plausible_token(&a));
        assert!(new_access_token().starts_with("fat_"));
        assert!(new_client_secret().starts_with("fcs_"));
        let id = new_client_id();
        assert!(id.starts_with("fctf_") && id.len() == 5 + 22);
    }

    #[test]
    fn sha256_hex_is_stable() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn plausible_token_rejects_garbage() {
        assert!(!is_plausible_token(""));
        assert!(!is_plausible_token("has space"));
        assert!(!is_plausible_token("semi;colon"));
        assert!(!is_plausible_token(&"a".repeat(MAX_TOKEN_INPUT_LEN + 1)));
    }

    #[test]
    fn client_secret_hash_roundtrip() {
        let secret = new_client_secret();
        let phc = hash_client_secret(&secret).unwrap();
        assert!(phc.starts_with("$argon2"));
        assert!(!phc.contains(&secret));
        assert!(verify_client_secret(&secret, &phc));
        assert!(!verify_client_secret("fcs_wrong", &phc));
        assert!(!verify_client_secret("", &phc));
        assert!(!verify_client_secret(&secret, "not-a-phc"));
    }
}
