//! AWD 实体与领域对象映射规则。

use crate::entity::sea_orm_active_enums::{
    AwdEventStatus, AwdPhase, BanStatus, GameboxStatus, JudgeTaskStatus, PrecheckStatus,
    RoundStatus, ScoreEventType, WgPeerStatus,
};

/// 标记：这些 ActiveEnum 类型是 AWD 领域状态的持久化表示。
pub trait AwdPersistedEnum: Sized + Clone + PartialEq + Eq + Send + Sync + 'static {}

impl AwdPersistedEnum for AwdEventStatus {}
impl AwdPersistedEnum for AwdPhase {}
impl AwdPersistedEnum for GameboxStatus {}
impl AwdPersistedEnum for RoundStatus {}
impl AwdPersistedEnum for JudgeTaskStatus {}
impl AwdPersistedEnum for PrecheckStatus {}
impl AwdPersistedEnum for BanStatus {}
impl AwdPersistedEnum for ScoreEventType {}
impl AwdPersistedEnum for WgPeerStatus {}

/// 在已持有 ActiveEnum 的边界使用的恒等映射。
/// 便于调用点表达意图：`status.persist()` 对比随意强转。
pub trait Persist: AwdPersistedEnum {
    fn persist(self) -> Self {
        self
    }
}

impl<T: AwdPersistedEnum> Persist for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::awd::domain::AwdEventStatusExt;

    #[test]
    fn active_enum_carries_domain_transitions() {
        // Domain rules are attached to the persisted enum via Ext traits.
        assert!(
            AwdEventStatus::Draft
                .can_transition_to(AwdEventStatus::Configuring)
                .is_ok()
        );
        assert_eq!(AwdEventStatus::Draft.persist(), AwdEventStatus::Draft);
    }
}
