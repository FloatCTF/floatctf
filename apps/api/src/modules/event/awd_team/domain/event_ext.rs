//! Domain state-machine rules for AWD event status and phase.
//!
//! Implemented on SeaORM ActiveEnum types (persistence representation).
//! Do not hand-edit `entity::sea_orm_active_enums`; put rules here only.

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
            Self::Configuring => &[Self::Deploying, Self::Draft],
            Self::Deploying => &[Self::Deployed, Self::DeployFailed],
            Self::Deployed => &[Self::Prechecking, Self::Configuring],
            Self::Prechecking => &[Self::Verified, Self::VerificationFailed, Self::Deployed],
            Self::Verified => &[Self::Running, Self::StartBlocked, Self::Configuring],
            Self::Running => &[Self::Paused, Self::Finished, Self::NetworkError],
            Self::Paused => &[Self::Running, Self::Finished],
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
}
