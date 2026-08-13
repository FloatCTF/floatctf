//! AWDP 练习 Judge 领域常量与密钥派生（纯逻辑，无 DB / 无 IO）。

use hkdf::Hkdf;
use sha2::Sha256;
use uuid::Uuid;

/// 练习专用 docker 子网名称（全部练习 GameBox 实例 + JudgeServer 所在）。
pub const PRACTICE_NETWORK_NAME: &str = "fctf-awdp-practice";

/// JudgeServer 监听端口（与 crates/awdp-judgeserver LISTEN_ADDR 默认一致）。
pub const PRACTICE_JUDGE_PORT: u16 = 8082;

/// 练习 JudgeServer 容器名（平台部署，固定名称便于幂等重建）。
pub const PRACTICE_JUDGE_CONTAINER_NAME: &str = "fctf-awdp-practice-judge";

/// 练习 Judge 回调的幂等 callback_id：`awdp-practice-judge:{sweep}:{run}:{instance}:{kind}`。
///
/// - `sweep` = 每次例行检查派发的随机 id：**同一实例不同轮次检查产生新结果行**；
/// - JudgeServer 对同一次派发的重试（同 callback_id）幂等去重。
pub fn judge_callback_id(sweep_id: Uuid, run_id: Uuid, instance_id: Uuid, kind: &str) -> String {
    format!("awdp-practice-judge:{sweep_id}:{run_id}:{instance_id}:{kind}")
}

/// 用平台 Secret 派生练习 Judge 内部令牌（HKDF-SHA256，hex 编码 32 字节）。
///
/// 与 flag 派生同思路（铁律 1/5）：不入库、不落日志；
/// 平台部署 JudgeServer 时注入 env，回调鉴权两侧各自比较。
pub fn practice_judge_token(jwt_secret: &[u8]) -> String {
    let hk = Hkdf::<Sha256>::new(Some(b"floatctf-awdp-practice-judge"), jwt_secret);
    let mut key = [0u8; 32];
    hk.expand(b"internal-token", &mut key)
        .expect("HKDF expand with 32-byte output cannot fail");
    hex::encode(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judge_token_is_deterministic_and_hex() {
        let t1 = practice_judge_token(b"platform-secret");
        let t2 = practice_judge_token(b"platform-secret");
        assert_eq!(t1, t2);
        assert_eq!(t1.len(), 64); // 32 bytes hex
        assert!(t1.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn judge_token_differs_between_secrets() {
        assert_ne!(
            practice_judge_token(b"secret-a"),
            practice_judge_token(b"secret-b")
        );
    }

    #[test]
    fn callback_id_is_stable_per_instance_kind() {
        let s = Uuid::from_u128(9);
        let r = Uuid::from_u128(1);
        let i = Uuid::from_u128(2);
        // 同一次派发（同 sweep）：稳定（JudgeServer 重试幂等）。
        assert_eq!(
            judge_callback_id(s, r, i, "exploit"),
            judge_callback_id(s, r, i, "exploit")
        );
        // 不同派发（新 sweep）：产生新结果行。
        assert_ne!(
            judge_callback_id(s, r, i, "exploit"),
            judge_callback_id(Uuid::from_u128(10), r, i, "exploit")
        );
        assert_ne!(
            judge_callback_id(s, r, i, "exploit"),
            judge_callback_id(s, r, i, "flag")
        );
    }
}
