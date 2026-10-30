//! PKCE（RFC 7636）——只支持 `S256`。

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub const METHOD_S256: &str = "S256";

/// `code_verifier = 43*128unreserved`（§4.1）。
pub fn is_valid_code_verifier(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len()) && verifier.bytes().all(is_unreserved)
}

/// S256 的 `code_challenge` 恰为 SHA-256 的 base64url（无填充）= 43 字符。
pub fn is_valid_s256_challenge(challenge: &str) -> bool {
    challenge.len() == 43
        && challenge
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `BASE64URL(SHA256(ASCII(code_verifier))) == code_challenge`（常量时间比较）。
pub fn verify_s256(verifier: &str, challenge: &str) -> bool {
    if !is_valid_code_verifier(verifier) || !is_valid_s256_challenge(challenge) {
        return false;
    }
    let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    computed.as_bytes().ct_eq(challenge.as_bytes()).into()
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7636 Appendix B 的官方测试向量。
    const RFC_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const RFC_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    #[test]
    fn rfc7636_appendix_b_vector() {
        assert!(verify_s256(RFC_VERIFIER, RFC_CHALLENGE));
    }

    #[test]
    fn wrong_verifier_fails() {
        let mut wrong = RFC_VERIFIER.to_string();
        wrong.replace_range(0..1, "e");
        assert!(!verify_s256(&wrong, RFC_CHALLENGE));
    }

    #[test]
    fn plain_method_style_input_fails() {
        // plain 方法下 challenge == verifier；S256 校验必须拒绝（防降级）。
        assert!(!verify_s256(RFC_VERIFIER, RFC_VERIFIER));
    }

    #[test]
    fn verifier_length_and_charset_bounds() {
        assert!(!is_valid_code_verifier(&"a".repeat(42)));
        assert!(is_valid_code_verifier(&"a".repeat(43)));
        assert!(is_valid_code_verifier(&"a".repeat(128)));
        assert!(!is_valid_code_verifier(&"a".repeat(129)));
        assert!(is_valid_code_verifier(&format!("{}-._~", "a".repeat(40))));
        assert!(!is_valid_code_verifier(&format!("{}+/=", "a".repeat(40))));
        assert!(!is_valid_code_verifier(&format!("{} ", "a".repeat(43))));
    }

    #[test]
    fn challenge_must_be_unpadded_base64url_sha256() {
        assert!(is_valid_s256_challenge(RFC_CHALLENGE));
        assert!(!is_valid_s256_challenge(&format!("{RFC_CHALLENGE}=")));
        assert!(!is_valid_s256_challenge(&RFC_CHALLENGE.replace('-', "+")));
        assert!(!is_valid_s256_challenge(&RFC_CHALLENGE[..42]));
    }
}
