//! Declared capabilities per EventMode (for API/frontend branching).

use serde::Serialize;

use crate::entity::sea_orm_active_enums::{EventFamily, ParticipantMode};
use crate::modules::event::common::domain::event_mode::EventMode;

#[derive(Debug, Clone, Serialize)]
pub struct EventCapabilities {
    pub participant_mode: ParticipantMode,
    pub supports_instances: bool,
    pub supports_standard_flag_submission: bool,
    pub supports_teams: bool,
    pub supports_wireguard: bool,
    pub supports_gameboxes: bool,
    pub supports_rounds: bool,
    pub supports_judge: bool,
    pub supports_reset: bool,
}

impl EventCapabilities {
    pub fn for_mode(mode: &EventMode) -> Self {
        let (supports_instances, supports_standard_flag_submission, awd_engine) = match mode.family
        {
            EventFamily::Jeopardy => (true, true, false),
            EventFamily::Awd => (false, false, true),
        };

        Self {
            participant_mode: mode.participant_mode.clone(),
            supports_instances,
            supports_standard_flag_submission,
            supports_teams: mode.is_team(),
            supports_wireguard: awd_engine,
            supports_gameboxes: awd_engine,
            supports_rounds: awd_engine,
            supports_judge: awd_engine,
            supports_reset: awd_engine,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::common::domain::event_mode::EventMode;

    #[test]
    fn all_valid_modes_have_capabilities() {
        for mode in [
            EventMode::jeopardy_practice(),
            EventMode::jeopardy_individual_competition(),
            EventMode::jeopardy_team_competition(),
            EventMode::awd_team_competition(),
        ] {
            let caps = EventCapabilities::for_mode(&mode);
            assert_eq!(caps.participant_mode, mode.participant_mode);
            if mode.is_jeopardy() {
                assert!(caps.supports_instances);
                assert!(caps.supports_standard_flag_submission);
                assert!(!caps.supports_gameboxes);
            } else {
                assert!(!caps.supports_instances);
                assert!(caps.supports_gameboxes);
            }
        }
    }
}
