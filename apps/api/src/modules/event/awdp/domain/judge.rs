//! AWDP 练习 Judge 领域常量与密钥派生（纯逻辑，无 DB / 无 IO）。

use hkdf::Hkdf;
use sha2::Sha256;
use uuid::Uuid;

/// 练习专用 docker 子网名称（全部练习 GameBox 实例 + JudgeServer 所在，data plane）。
pub const PRACTICE_NETWORK_NAME: &str = "fctf-awdp-practice";

/// 练习 control plane docker 子网（internal=true；仅 JudgeServer 加入，GameBox 禁止加入）。
/// FloatCTF API 宿主部署时经该子网网关/宿主绑定地址承接 internal API 调用。
pub const CONTROL_NETWORK_NAME: &str = "fctf-awdp-control";

/// 练习子网动态池（/23 后半段 10.42.3.0/24）——GameBox 实例动态 IP 范围（nftables ACL 识别用）。
pub const PRACTICE_DYNAMIC_POOL: &str = "10.42.3.0/24";

/// JudgeServer data plane 监听端口（与 crates/awdp-judgeserver DATA_LISTEN_ADDR 默认一致）。
/// 80：GameBox 内 `curl http://judge-server/flag` 无需带端口（data plane 不发布到宿主）。
pub const PRACTICE_JUDGE_PORT: u16 = 80;

/// JudgeServer data plane DNS alias（练习 data 网络内 GameBox 可解析；玩家 contract）。
pub const JUDGE_DATA_ALIAS: &str = "judge-server";

/// 练习 JudgeServer 容器名（平台部署，固定名称便于幂等重建）。
pub const PRACTICE_JUDGE_CONTAINER_NAME: &str = "fctf-awdp-practice-judge";

/// 练习固定 nftables ACL 表名（兼容既有部署；赛事专属表见 `event_acl_table_name`）。
pub const PRACTICE_ACL_TABLE_NAME: &str = "floatctf_awdp_practice";

/// 赛事网络/容器名前缀（每赛事独立网络模型）。
const EVENT_PREFIX_LEN: usize = 12;

/// 练习虚拟赛事固定网络名（兼容既有部署；练习不落 awdp_event_networks 表）。
pub fn practice_network_name() -> &'static str {
    PRACTICE_NETWORK_NAME
}

/// 赛事专属 Docker 网络名：`fctf-awdp-{event_id 前 12 hex}`。
pub fn event_network_name(event_id: Uuid) -> String {
    format!(
        "fctf-awdp-{}",
        &event_id.to_string().replace('-', "")[..EVENT_PREFIX_LEN]
    )
}

/// 赛事专属 JudgeServer 容器名：`fctf-awdp-judge-{event_id 前 12 hex}`；
/// 练习虚拟赛事保持旧名 `fctf-awdp-practice-judge`（兼容既有部署）。
pub fn event_judge_container_name(event_id: Uuid) -> String {
    if is_practice_event(event_id) {
        return PRACTICE_JUDGE_CONTAINER_NAME.to_string();
    }
    format!(
        "fctf-awdp-judge-{}",
        &event_id.to_string().replace('-', "")[..EVENT_PREFIX_LEN]
    )
}

/// 赛事专属 JudgeServer worker id（与容器 env WORKER_ID 一致，平台按 event 过滤 claim）。
pub fn event_judge_worker_id(event_id: Uuid) -> String {
    format!(
        "practice-judge-{}",
        &event_id.to_string().replace('-', "")[..8]
    )
}

/// 赛事专属 nftables 表名：`floatctf_awdp_{event_id 前 8 hex}`（练习固定表名兼容）。
pub fn event_acl_table_name(event_id: Uuid) -> String {
    format!(
        "floatctf_awdp_{}",
        &event_id.to_string().replace('-', "")[..8]
    )
}

/// 是否练习虚拟赛事（AWDPlusPractice）：练习沿用固定网络，不落赛事网络表。
pub fn is_practice_event(event_id: Uuid) -> bool {
    event_id == crate::core::system_ids::EVENT_PRACTICE_AWDP
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
    fn event_identities_are_deterministic_and_prefixed() {
        let ev = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let net = event_network_name(ev);
        let cont = event_judge_container_name(ev);
        let worker = event_judge_worker_id(ev);
        let acl = event_acl_table_name(ev);
        assert!(net.starts_with("fctf-awdp-"));
        assert!(cont.starts_with("fctf-awdp-judge-"));
        assert!(worker.starts_with("practice-judge-"));
        assert!(acl.starts_with("floatctf_awdp_"));
        assert_eq!(net, event_network_name(ev)); // deterministic
        assert_ne!(event_network_name(ev), event_network_name(Uuid::nil()));
        // nftables 表名不允许 `-`，只允许 `_`。
        assert!(!acl.contains('-'));
    }

    #[test]
    fn practice_event_is_detected() {
        assert!(is_practice_event(
            crate::core::system_ids::EVENT_PRACTICE_AWDP
        ));
        assert!(!is_practice_event(Uuid::nil()));
    }
}
