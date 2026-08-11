//! AWD 赛事领域扩展。

use crate::entity::sea_orm_active_enums::{AwdEventStatus, AwdPhase};

pub trait AwdEventStatusExt {
    fn is_terminal(&self) -> bool;
    fn is_active(&self) -> bool;
    fn is_configurable(&self) -> bool;
    fn valid_transitions(&self) -> &'static [AwdEventStatus];
    fn can_transition_to(&self, target: AwdEventStatus) -> Result<(), String>;
    /// Validate and return the target status (or error).
    fn transition(&self, target: AwdEventStatus) -> Result<AwdEventStatus, String>;
}

impl AwdEventStatusExt for AwdEventStatus {
    fn is_terminal(&self) -> bool {
        matches!(self, Self::Finished | Self::Archived)
    }

    fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }

    fn is_configurable(&self) -> bool {
        matches!(
            self,
            Self::Draft
                | Self::Configuring
                | Self::Deployed
                | Self::Prechecking
                | Self::Verified
                | Self::StartBlocked
                | Self::DeployFailed
                | Self::VerificationFailed
        )
    }

    fn valid_transitions(&self) -> &'static [AwdEventStatus] {
        match self {
            Self::Draft => &[Self::Configuring],
            // Phase 0 补充：配置变更后不重新部署、直接重检（Phase 2 §6 集成路径
            // Configuring→Prechecking→Verified→Running）。
            Self::Configuring => &[Self::Deploying, Self::Draft, Self::Prechecking],
            Self::Deploying => &[Self::Deployed, Self::DeployFailed],
            Self::Deployed => &[Self::Prechecking, Self::Configuring],
            Self::Prechecking => &[Self::Verified, Self::VerificationFailed, Self::Deployed],
            Self::Verified => &[Self::Running, Self::StartBlocked, Self::Configuring],
            Self::Running => &[Self::Paused, Self::Finished, Self::NetworkError],
            // P4-10：暂停中网络策略应用失败同样 Fail Closed（不留"Paused 但网络没生效"）
            Self::Paused => &[Self::Running, Self::Finished, Self::NetworkError],
            Self::NetworkError => &[Self::Paused, Self::Finished],
            Self::StartBlocked => &[Self::Prechecking, Self::Configuring],
            Self::Finished => &[Self::Archived],
            Self::Archived => &[],
            Self::DeployFailed => &[Self::Configuring],
            Self::VerificationFailed => &[Self::Prechecking, Self::Configuring],
        }
    }

    fn can_transition_to(&self, target: AwdEventStatus) -> Result<(), String> {
        if self.valid_transitions().contains(&target) {
            Ok(())
        } else {
            Err(format!("Invalid transition: {:?} -> {:?}", self, target))
        }
    }

    fn transition(&self, target: AwdEventStatus) -> Result<AwdEventStatus, String> {
        self.can_transition_to(target.clone())?;
        Ok(target)
    }
}

pub trait AwdPhaseExt {
    fn allows_flag_issue(&self) -> bool;
    fn allows_flag_submission(&self) -> bool;
    fn allows_judge(&self) -> bool;
    /// phase 转移守卫（Phase 0）：Pause 可从任意阶段进入；resume 只能回到 Hardening/Attack；
    /// Hardening ↔ Attack 可互换；幂等重设允许。
    fn can_transition_to(&self, target: AwdPhase) -> Result<(), String>;
}

impl AwdPhaseExt for AwdPhase {
    fn allows_flag_issue(&self) -> bool {
        matches!(self, Self::Attack)
    }

    fn allows_flag_submission(&self) -> bool {
        matches!(self, Self::Attack)
    }

    fn allows_judge(&self) -> bool {
        matches!(self, Self::Attack | Self::Hardening)
    }

    fn can_transition_to(&self, target: AwdPhase) -> Result<(), String> {
        if self == &target {
            return Ok(());
        }
        match (self, &target) {
            // 任意阶段可进入 Pause
            (_, AwdPhase::Pause) => Ok(()),
            // resume：Pause 回到比赛阶段
            (AwdPhase::Pause, AwdPhase::Hardening) | (AwdPhase::Pause, AwdPhase::Attack) => Ok(()),
            // 轮次切换
            (AwdPhase::Hardening, AwdPhase::Attack) | (AwdPhase::Attack, AwdPhase::Hardening) => {
                Ok(())
            }
            _ => Err(format!(
                "Invalid phase transition: {:?} -> {:?}",
                self, target
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_to_configuring_is_valid() {
        assert!(
            AwdEventStatus::Draft
                .can_transition_to(AwdEventStatus::Configuring)
                .is_ok()
        );
    }

    #[test]
    fn finished_to_draft_is_invalid() {
        assert!(
            AwdEventStatus::Finished
                .can_transition_to(AwdEventStatus::Draft)
                .is_err()
        );
    }

    #[test]
    fn archived_has_no_transitions() {
        assert!(AwdEventStatus::Archived.valid_transitions().is_empty());
    }

    #[test]
    fn verified_can_return_to_configuring() {
        assert!(
            AwdEventStatus::Verified
                .can_transition_to(AwdEventStatus::Configuring)
                .is_ok()
        );
    }

    #[test]
    fn phase_flag_issuing() {
        assert!(AwdPhase::Attack.allows_flag_issue());
        assert!(!AwdPhase::Hardening.allows_flag_issue());
        assert!(!AwdPhase::Pause.allows_flag_issue());
    }

    #[test]
    fn terminal_and_active_states() {
        assert!(AwdEventStatus::Finished.is_terminal());
        assert!(AwdEventStatus::Archived.is_terminal());
        assert!(!AwdEventStatus::Running.is_terminal());
        assert!(AwdEventStatus::Running.is_active());
        assert!(AwdEventStatus::Paused.is_active());
        assert!(!AwdEventStatus::Draft.is_active());
    }

    #[test]
    fn transition_helper_returns_target() {
        let next = AwdEventStatus::Draft
            .transition(AwdEventStatus::Configuring)
            .unwrap();
        assert_eq!(next, AwdEventStatus::Configuring);
    }

    #[test]
    fn running_to_network_error_is_valid() {
        assert!(
            AwdEventStatus::Running
                .can_transition_to(AwdEventStatus::NetworkError)
                .is_ok()
        );
    }

    #[test]
    fn paused_to_network_error_is_valid() {
        // P4-10：暂停中网络策略应用失败必须能 Fail Closed（NetworkError）
        assert!(
            AwdEventStatus::Paused
                .can_transition_to(AwdEventStatus::NetworkError)
                .is_ok()
        );
    }

    #[test]
    fn configuring_can_enter_prechecking() {
        // Phase 0 补充路径：配置变更后无需重新部署即可重检。
        assert!(
            AwdEventStatus::Configuring
                .can_transition_to(AwdEventStatus::Prechecking)
                .is_ok()
        );
    }

    #[test]
    fn draft_to_running_is_invalid() {
        assert!(
            AwdEventStatus::Draft
                .can_transition_to(AwdEventStatus::Running)
                .is_err()
        );
    }

    #[test]
    fn phase_guard_allows_round_switches() {
        assert!(
            AwdPhase::Hardening
                .can_transition_to(AwdPhase::Attack)
                .is_ok()
        );
        assert!(
            AwdPhase::Attack
                .can_transition_to(AwdPhase::Hardening)
                .is_ok()
        );
        assert!(AwdPhase::Pause.can_transition_to(AwdPhase::Attack).is_ok());
        assert!(
            AwdPhase::Pause
                .can_transition_to(AwdPhase::Hardening)
                .is_ok()
        );
    }

    #[test]
    fn phase_guard_allows_pause_from_any_phase() {
        assert!(
            AwdPhase::Hardening
                .can_transition_to(AwdPhase::Pause)
                .is_ok()
        );
        assert!(AwdPhase::Attack.can_transition_to(AwdPhase::Pause).is_ok());
        assert!(AwdPhase::Pause.can_transition_to(AwdPhase::Pause).is_ok());
    }

    #[test]
    fn phase_guard_rejects_nonsense() {
        // Pause 只能回到 Hardening/Attack，不存在其他阶段；同阶段幂等允许。
        assert!(
            AwdPhase::Hardening
                .can_transition_to(AwdPhase::Hardening)
                .is_ok()
        );
        assert!(AwdPhase::Attack.can_transition_to(AwdPhase::Attack).is_ok());
    }
}
