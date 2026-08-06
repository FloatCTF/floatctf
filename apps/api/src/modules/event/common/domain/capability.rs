//! Declared capabilities per competition mode (for API/frontend branching).

use serde::Serialize;

use crate::entity::sea_orm_active_enums::EventType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantMode {
    Individual,
    Team,
}

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
    pub fn for_event_type(event_type: &EventType) -> Self {
        match event_type {
            EventType::JeopardyPractice => Self {
                participant_mode: ParticipantMode::Individual,
                supports_instances: true,
                supports_standard_flag_submission: true,
                supports_teams: false,
                supports_wireguard: false,
                supports_gameboxes: false,
                supports_rounds: false,
                supports_judge: false,
                supports_reset: false,
            },
            EventType::JeopardySingle => Self {
                participant_mode: ParticipantMode::Individual,
                supports_instances: true,
                supports_standard_flag_submission: true,
                supports_teams: false,
                supports_wireguard: false,
                supports_gameboxes: false,
                supports_rounds: false,
                supports_judge: false,
                supports_reset: false,
            },
            EventType::JeopardyTeam => Self {
                participant_mode: ParticipantMode::Team,
                supports_instances: true,
                supports_standard_flag_submission: true,
                supports_teams: true,
                supports_wireguard: false,
                supports_gameboxes: false,
                supports_rounds: false,
                supports_judge: false,
                supports_reset: false,
            },
            EventType::AwdTeam => Self {
                participant_mode: ParticipantMode::Team,
                supports_instances: false,
                supports_standard_flag_submission: false,
                supports_teams: true,
                supports_wireguard: true,
                supports_gameboxes: true,
                supports_rounds: true,
                supports_judge: true,
                supports_reset: true,
            },
        }
    }
}
