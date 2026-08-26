//! AWD 轮次领域扩展。

use crate::entity::sea_orm_active_enums::{JudgeTaskStatus, PrecheckStatus, RoundStatus};

pub trait RoundStatusExt {
    fn is_terminal(&self) -> bool;
    fn valid_transitions(&self) -> &'static [RoundStatus];
}

impl RoundStatusExt for RoundStatus {
    fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed)
    }

    fn valid_transitions(&self) -> &'static [RoundStatus] {
        match self {
            Self::Active => &[Self::Paused, Self::Completed],
            Self::Paused => &[Self::Active],
            Self::Completed => &[],
        }
    }
}

pub trait JudgeTaskStatusExt {
    fn is_terminal(&self) -> bool;
    fn is_up(&self) -> bool;
    fn is_down(&self) -> bool;
}

impl JudgeTaskStatusExt for JudgeTaskStatus {
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Up
                | Self::Down
                | Self::JudgeError
                | Self::SkippedResetting
                | Self::SkippedBanned
        )
    }

    fn is_up(&self) -> bool {
        matches!(self, Self::Up)
    }

    fn is_down(&self) -> bool {
        matches!(self, Self::Down)
    }
}

pub trait PrecheckStatusExt {
    fn is_terminal(&self) -> bool;
}

impl PrecheckStatusExt for PrecheckStatus {
    fn is_terminal(&self) -> bool {
        matches!(self, Self::Passed | Self::Failed | Self::Error)
    }
}
