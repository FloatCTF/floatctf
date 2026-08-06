//! Extension methods for GameBox status transitions.

use crate::entity::sea_orm_active_enums::GameboxStatus;

pub trait GameboxStatusExt {
    fn is_healthy(&self) -> bool;
    fn needs_attention(&self) -> bool;
    fn is_transitional(&self) -> bool;
    fn valid_transitions(&self) -> &'static [GameboxStatus];
    fn can_transition_to(&self, target: GameboxStatus) -> Result<(), String>;
}

impl GameboxStatusExt for GameboxStatus {
    fn is_healthy(&self) -> bool {
        matches!(self, Self::Ready | Self::Running)
    }

    fn needs_attention(&self) -> bool {
        matches!(
            self,
            Self::Missing | Self::Orphan | Self::Conflict | Self::StartFailed | Self::ResetFailed
        )
    }

    fn is_transitional(&self) -> bool {
        matches!(self, Self::Pending | Self::Creating | Self::Resetting)
    }

    fn valid_transitions(&self) -> &'static [GameboxStatus] {
        match self {
            Self::Pending => &[Self::Creating],
            Self::Creating => &[Self::Running, Self::StartFailed],
            Self::Running => &[Self::Ready, Self::Missing, Self::Stopped],
            Self::Ready => &[Self::Resetting, Self::Missing, Self::Stopped, Self::Running],
            Self::Resetting => &[Self::Ready, Self::ResetFailed],
            Self::Missing => &[Self::Creating],
            Self::Orphan => &[Self::Ready, Self::Stopped],
            Self::Conflict => &[Self::Ready],
            Self::StartFailed => &[Self::Creating],
            Self::ResetFailed => &[Self::Resetting],
            Self::Stopped => &[Self::Creating],
        }
    }

    fn can_transition_to(&self, target: GameboxStatus) -> Result<(), String> {
        if self.valid_transitions().contains(&target) {
            Ok(())
        } else {
            Err(format!(
                "Invalid GameBox transition: {:?} -> {:?}",
                self, target
            ))
        }
    }
}
