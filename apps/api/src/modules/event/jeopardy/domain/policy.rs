//! Jeopardy mode policy derived from EventMode.

use crate::entity::sea_orm_active_enums::ParticipantMode;
use crate::modules::event::common::domain::capability::EventCapabilities;
use crate::modules::event::common::domain::event_mode::{EventMode, JeopardyVariant};

/// Who receives score credit for a formal Jeopardy solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreOwner {
    User,
    Team,
}

/// Unified Jeopardy policy derived from EventMode (no EventType).
#[derive(Debug, Clone)]
pub struct JeopardyPolicy {
    mode: EventMode,
    variant: JeopardyVariant,
}

impl JeopardyPolicy {
    pub fn from_mode(mode: EventMode) -> Option<Self> {
        let variant = mode.jeopardy_variant()?;
        Some(Self { mode, variant })
    }

    pub fn mode(&self) -> &EventMode {
        &self.mode
    }

    pub fn variant(&self) -> JeopardyVariant {
        self.variant
    }

    pub fn participant_mode(&self) -> ParticipantMode {
        self.mode.participant_mode.clone()
    }

    pub fn score_owner(&self) -> ScoreOwner {
        match self.mode.participant_mode {
            ParticipantMode::Individual => ScoreOwner::User,
            ParticipantMode::Team => ScoreOwner::Team,
        }
    }

    pub fn allow_repeat_solve(&self) -> bool {
        self.variant.allow_repeat_solve()
    }

    pub fn requires_team(&self) -> bool {
        self.variant.requires_team()
    }

    pub fn capabilities(&self) -> EventCapabilities {
        EventCapabilities::for_mode(&self.mode)
    }

    /// Max concurrent running instances for a participant (user or team).
    pub fn max_concurrent_instances(&self, team_member_count: Option<u64>) -> u64 {
        match self.variant {
            JeopardyVariant::Practice => 1,
            JeopardyVariant::IndividualCompetition => 2,
            JeopardyVariant::TeamCompetition => {
                team_member_count.unwrap_or(1).saturating_mul(2).max(2)
            }
        }
    }
}

/// Backward-compatible trait alias surface used by mode service bags.
pub trait JeopardyModePolicy: Send + Sync {
    fn policy(&self) -> JeopardyPolicy;
    fn participant_mode(&self) -> ParticipantMode {
        self.policy().participant_mode()
    }
    fn score_owner(&self) -> ScoreOwner {
        self.policy().score_owner()
    }
    fn allow_repeat_solve(&self) -> bool {
        self.policy().allow_repeat_solve()
    }
    fn requires_team(&self) -> bool {
        self.policy().requires_team()
    }
    fn capabilities(&self) -> EventCapabilities {
        self.policy().capabilities()
    }
    fn max_concurrent_instances(&self, team_member_count: Option<u64>) -> u64 {
        self.policy().max_concurrent_instances(team_member_count)
    }
}
