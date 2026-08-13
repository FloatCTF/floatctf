//! AWDP 练习 Judge 领域常量与密钥派生（纯逻辑，无 DB / 无 IO）。

use hkdf::Hkdf;
use sha2::Sha256;
use uuid::Uuid;

/// 练习专用 docker 子网名称（全部练习 GameBox 实例 + JudgeServer 所在，data plane）。
pub const PRACTICE_NETWORK_NAME: &str = "fctf-awdp-practice";

/// 练习 control plane docker 子网（internal=true；仅 JudgeServer 加入，GameBox 禁止加入）。
/// FloatCTF API 宿主部署时经该子网网关/宿主绑定地址承接 internal API 调用。
pub const CONTROL_NETWORK_NAME: &str = "fctf-awdp-control";

/// 练习子网动态池（10.42.2.128/25）——GameBox 实例动态 IP 范围（nftables ACL 识别用）。
pub const PRACTICE_DYNAMIC_POOL: &str = "10.42.2.128/25";

/// JudgeServer data plane 监听端口（与 crates/awdp-judgeserver DATA_LISTEN_ADDR 默认一致）。
pub const PRACTICE_JUDGE_PORT: u16 = 8080;

/// 练习 JudgeServer 容器名（平台部署，固定名称便于幂等重建）。
pub const PRACTICE_JUDGE_CONTAINER_NAME: &str = "fctf-awdp-practice-judge";

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
}
