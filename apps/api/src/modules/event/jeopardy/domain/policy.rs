//! Jeopardy mode policy derived from EventMode.

use crate::entity::sea_orm_active_enums::ParticipantMode;
use crate::modules::event::common::domain::capability::EventCapabilities;
use crate::modules::event::common::domain::event_mode::{EventMode, JeopardyVariant};

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
