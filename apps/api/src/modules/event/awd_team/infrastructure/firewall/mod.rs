//! Firewall runtime abstraction (Phase 1 P1-3).
//!
//! 唯一生产实现为 native nftables（`NftablesFirewallRuntime`）。
//! 不提供 application-facing 的 add/remove rule API —— application 只描述
//! Desired State，runtime 决定如何收敛（global reconcile）。

pub mod env;
pub mod nftables;
pub mod render;

pub use env::HostFirewallEnvironment;
pub use nftables::NftablesFirewallRuntime;
pub use render::{NftObjectName, ObservedFirewallState, TABLE_NAME};

use async_trait::async_trait;

use crate::modules::event::awd_team::{AwdResult, domain::firewall_state::DesiredFirewallState};

/// 一次 firewall reconcile 的结果。
#[derive(Debug, Clone)]
pub struct FirewallApplyResult {
    pub revision: u64,
    pub applied: bool,
}

/// 防火墙验证结论。
#[derive(Debug, Clone)]
pub struct FirewallVerification {
    pub verified: bool,
    pub observed: ObservedFirewallState,
    pub notes: Vec<String>,
}

/// Firewall runtime 抽象（Phase 1 P1-3）。
///
/// 三个方法均为 desired-state 语义：
/// - `inspect`：观测 `table inet floatctf_awd` 当前状态；
/// - `reconcile`：把全局 DesiredFirewallState 原子收敛到 nftables（`nft -c` 校验 + `nft -f` 应用 + verify）；
/// - `verify`：校验当前观测状态与期望一致（revision 匹配等）。
#[async_trait]
pub trait FirewallRuntime: Send + Sync {
    async fn inspect(&self) -> AwdResult<ObservedFirewallState>;
    async fn reconcile(&self, desired: &DesiredFirewallState) -> AwdResult<FirewallApplyResult>;
    async fn verify(&self, desired: &DesiredFirewallState) -> AwdResult<FirewallVerification>;
}

/// 无操作 runtime：仅用于 unit test / dev mock。
/// **Noop 永远不允许 Verified**（Phase 2 双门禁 + P1-12 host capability）。
pub struct NoopFirewallRuntime;

#[async_trait]
impl FirewallRuntime for NoopFirewallRuntime {
    async fn inspect(&self) -> AwdResult<ObservedFirewallState> {
        Ok(ObservedFirewallState::default())
    }

    async fn reconcile(&self, _desired: &DesiredFirewallState) -> AwdResult<FirewallApplyResult> {
        Ok(FirewallApplyResult {
            revision: 0,
            applied: false,
        })
    }

    async fn verify(&self, _desired: &DesiredFirewallState) -> AwdResult<FirewallVerification> {
        Ok(FirewallVerification {
            verified: false,
            observed: ObservedFirewallState::default(),
            notes: vec!["NoopFirewallRuntime never verifies".into()],
        })
    }
}
