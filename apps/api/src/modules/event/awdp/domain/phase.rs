//! AWDP 阶段状态机（4 态，不复制 AWD 14 态）。

use crate::entity::sea_orm_active_enums::AwdpPhase;

pub trait AwdpPhaseExt {
    fn is_active(&self) -> bool;
    fn is_terminal(&self) -> bool;
    fn can_transition_to(&self, target: AwdpPhase) -> Result<(), String>;
}

impl AwdpPhaseExt for AwdpPhase {
    fn is_active(&self) -> bool {
        matches!(self, Self::Break | Self::Fix)
    }

    fn is_terminal(&self) -> bool {
        matches!(self, Self::Ended)
    }

    fn can_transition_to(&self, target: AwdpPhase) -> Result<(), String> {
        let ok = match (self, &target) {
            (AwdpPhase::Pending, AwdpPhase::Break) => true, // start
            (AwdpPhase::Break, AwdpPhase::Fix) => true,     // break 到期
            (AwdpPhase::Fix, AwdpPhase::Ended) => true,     // 最后一轮 cutoff
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(format!(
                "Invalid AWDP phase transition: {self:?} -> {target:?}"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_state_machine() {
        assert!(
            AwdpPhase::Pending
                .can_transition_to(AwdpPhase::Break)
                .is_ok()
        );
        assert!(AwdpPhase::Break.can_transition_to(AwdpPhase::Fix).is_ok());
        assert!(AwdpPhase::Fix.can_transition_to(AwdpPhase::Ended).is_ok());
    }

    #[test]
    fn no_retrograde_or_skip() {
        assert!(
            AwdpPhase::Pending
                .can_transition_to(AwdpPhase::Fix)
                .is_err()
        );
        assert!(
            AwdpPhase::Pending
                .can_transition_to(AwdpPhase::Ended)
                .is_err()
        );
        assert!(
            AwdpPhase::Break
                .can_transition_to(AwdpPhase::Pending)
                .is_err()
        );
        assert!(
            AwdpPhase::Ended
                .can_transition_to(AwdpPhase::Pending)
                .is_err()
        );
        assert!(AwdpPhase::Ended.can_transition_to(AwdpPhase::Fix).is_err());
    }

    #[test]
    fn active_and_terminal() {
        assert!(AwdpPhase::Break.is_active());
        assert!(AwdpPhase::Fix.is_active());
        assert!(!AwdpPhase::Pending.is_active());
        assert!(AwdpPhase::Ended.is_terminal());
    }
}
