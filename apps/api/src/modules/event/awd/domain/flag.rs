//! AWD Flag 领域模型。

use sha2::{Digest, Sha256};

/// 使用 HMAC-SHA256 生成确定性 Flag。
///
/// Flag 派生自：
/// - `event_secret`: per-event secret key
/// - `event_id`: UUID of the event
/// - `round_id`: UUID of the round
/// - `gamebox_instance_id`: UUID of the GameBox instance
/// - `prefix`: flag prefix (e.g., "flag")
pub fn generate_flag(
    event_secret: &[u8],
    event_id: &str,
    round_id: &str,
    gamebox_instance_id: &str,
    prefix: &str,
) -> String {
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(event_secret).expect("HMAC can take key of any size");

    let input = format!("{}||{}||{}", event_id, round_id, gamebox_instance_id);
    mac.update(input.as_bytes());

    let result = mac.finalize();
    let code_bytes = result.into_bytes();
    let flag_body = hex::encode(&code_bytes[..16]); // take first 16 bytes = 32 hex chars

    format!("{}{{{}}}", prefix, flag_body)
}

/// 对 Flag 做存储哈希（SHA-256）。
pub fn hash_flag(flag: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(flag.as_bytes());
    hex::encode(hasher.finalize())
}

/// 用已存哈希校验提交的 Flag。
pub fn verify_flag(flag: &str, stored_hash: &str) -> bool {
    hash_flag(flag) == stored_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_flag_generation() {
        let secret = b"test-event-secret-32bytes!!";
        let flag1 = generate_flag(secret, "evt-1", "rnd-1", "gbx-1", "flag");
        let flag2 = generate_flag(secret, "evt-1", "rnd-1", "gbx-1", "flag");
        assert_eq!(flag1, flag2, "Same inputs must produce same flag");
    }

    #[test]
    fn test_different_inputs_produce_different_flags() {
        let secret = b"test-event-secret-32bytes!!";
        let flag1 = generate_flag(secret, "evt-1", "rnd-1", "gbx-1", "flag");
        let flag2 = generate_flag(secret, "evt-1", "rnd-1", "gbx-2", "flag");
        assert_ne!(flag1, flag2);
    }

    #[test]
    fn test_different_secrets_produce_different_flags() {
        let flag1 = generate_flag(b"secret-a", "evt-1", "rnd-1", "gbx-1", "flag");
        let flag2 = generate_flag(b"secret-b", "evt-1", "rnd-1", "gbx-1", "flag");
        assert_ne!(flag1, flag2);
    }

    #[test]
    fn test_flag_format() {
        let flag = generate_flag(b"test-secret", "evt-1", "rnd-1", "gbx-1", "flag");
        assert!(flag.starts_with("flag{"), "Flag must start with prefix");
        assert!(flag.ends_with("}"), "Flag must end with }}");
    }

    #[test]
    fn test_hash_and_verify() {
        let flag = "flag{test123}";
        let hash = hash_flag(flag);
        assert!(verify_flag(flag, &hash));
        assert!(!verify_flag("flag{wrong}", &hash));
    }

    #[test]
    fn test_flag_hash_is_stable() {
        let h1 = hash_flag("flag{abc}");
        let h2 = hash_flag("flag{abc}");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_custom_prefix() {
        let flag = generate_flag(b"secret", "e", "r", "g", "FLAG");
        assert!(flag.starts_with("FLAG{"));
    }
}
