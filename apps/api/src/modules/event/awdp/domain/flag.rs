//! AWDP Break flag：确定性 HMAC（plan §18）。
//!
//! Flag 稳定绑定：run × gamebox × participant（user/team）。
//! - 每 participant 的容器拿到自己的 flag（不能提交他人 flag）；
//! - 跨 reset 稳定（同 subject 同 gamebox 永远同 flag）；
//! - Break 不需要 round rotating flag。
//!
//! 密钥派生：平台级 Secret（`auth.jwt_secret`，仅内存）经 HKDF 派生 per-run 密钥，
//! 不新增环境变量 / 不入库（铁律 1 / 5）。flag 本体即暴露给选手，
//! 安全性只需防伪造他人 flag；jwt_secret 泄露已等价于全平台失守。

use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

use crate::modules::event::awdp::domain::score::subject_key;

/// 用平台 Secret 派生 per-run flag HMAC 密钥（HKDF-SHA256，32 字节）。
fn derive_run_flag_key(jwt_secret: &[u8], run_id: &str) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::new(Some(b"floatctf-awdp-flag"), jwt_secret);
    let mut key = [0u8; 32];
    hk.expand(run_id.as_bytes(), &mut key)
        .expect("HKDF expand with 32-byte output cannot fail");
    key.to_vec()
}

/// 生成确定性 Break flag：`{prefix}{awdp:{run}:{gamebox}:{subject} 的 HMAC 前 16 字节}`
pub fn awdp_flag(
    jwt_secret: &[u8],
    run_id: Uuid,
    gamebox_id: Uuid,
    user_id: Option<Uuid>,
    team_id: Option<Uuid>,
    prefix: &str,
) -> String {
    let key = derive_run_flag_key(jwt_secret, &run_id.to_string());
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("HMAC accepts any key size");
    let input = format!(
        "awdp:{}:{}:{}",
        run_id,
        gamebox_id,
        subject_key(user_id, team_id)
    );
    mac.update(input.as_bytes());
    let code = mac.finalize().into_bytes();
    let body = hex::encode(&code[..16]);
    format!("{prefix}{{{body}}}")
}

/// 存储哈希（awdp_breaks.flag_sha256）。
pub fn hash_flag(flag: &str) -> String {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(flag.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn ids() -> (Uuid, Uuid, Uuid) {
        (
            Uuid::from_str("00000000-0000-0000-0000-000000000001").unwrap(),
            Uuid::from_str("00000000-0000-0000-0000-000000000002").unwrap(),
            Uuid::from_str("00000000-0000-0000-0000-000000000003").unwrap(),
        )
    }

    #[test]
    fn flag_is_deterministic_per_subject() {
        let (r, g, u) = ids();
        let secret = b"platform-secret";
        let f1 = awdp_flag(secret, r, g, Some(u), None, "flag");
        let f2 = awdp_flag(secret, r, g, Some(u), None, "flag");
        assert_eq!(f1, f2);
        assert!(f1.starts_with("flag{") && f1.ends_with('}'));
    }

    #[test]
    fn different_subjects_get_different_flags() {
        let (r, g, _u) = ids();
        let u1 = Uuid::from_str("00000000-0000-0000-0000-00000000000a").unwrap();
        let u2 = Uuid::from_str("00000000-0000-0000-0000-00000000000b").unwrap();
        let secret = b"platform-secret";
        let f1 = awdp_flag(secret, r, g, Some(u1), None, "flag");
        let f2 = awdp_flag(secret, r, g, Some(u2), None, "flag");
        assert_ne!(f1, f2);
        // user vs team subject 也不同
        let t = Uuid::from_str("00000000-0000-0000-0000-00000000000c").unwrap();
        let f3 = awdp_flag(secret, r, g, None, Some(t), "flag");
        assert_ne!(f1, f3);
    }

    #[test]
    fn different_runs_get_different_flags() {
        let (r, g, u) = ids();
        let r2 = Uuid::from_str("00000000-0000-0000-0000-0000000000ff").unwrap();
        let secret = b"platform-secret";
        assert_ne!(
            awdp_flag(secret, r, g, Some(u), None, "flag"),
            awdp_flag(secret, r2, g, Some(u), None, "flag")
        );
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(hash_flag("flag{x}"), hash_flag("flag{x}"));
        assert_ne!(hash_flag("flag{x}"), hash_flag("flag{y}"));
    }
}
