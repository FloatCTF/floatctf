//! 按 [`EventMode`] 声明的能力位（供 API / 前端分支）。

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
        match mode.family {
            EventFamily::Jeopardy => Self {
                participant_mode: mode.participant_mode.clone(),
                supports_instances: true,
                supports_standard_flag_submission: true,
                supports_teams: mode.is_team(),
                supports_wireguard: false,
                supports_gameboxes: false,
                supports_rounds: false,
                supports_judge: false,
                supports_reset: false,
            },
            EventFamily::Awd => Self {
                participant_mode: mode.participant_mode.clone(),
                supports_instances: false,
                supports_standard_flag_submission: false,
                supports_teams: mode.is_team(),
                supports_wireguard: true,
                supports_gameboxes: true,
                supports_rounds: true,
                supports_judge: true,
                supports_reset: true,
            },
            // AWDP：通用 instances（按需启动）+ GameBox + Fix rounds + 评估 + reset；
            // 无 WireGuard（V1 沿用 Challenge 的随机 high port 暴露模型）。
            EventFamily::Awdp => Self {
                participant_mode: mode.participant_mode.clone(),
                supports_instances: true,
                supports_standard_flag_submission: false,
                supports_teams: mode.is_team(),
                supports_wireguard: false,
                supports_gameboxes: true,
                supports_rounds: true,
                supports_judge: true,
                supports_reset: true,
            },
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
            EventMode::awdp_team_competition(),
            EventMode::awdp_individual_competition(),
            EventMode::awdp_practice(),
        ] {
            let caps = EventCapabilities::for_mode(&mode);
            assert_eq!(caps.participant_mode, mode.participant_mode);
            if mode.is_jeopardy() {
                assert!(caps.supports_instances);
                assert!(caps.supports_standard_flag_submission);
                assert!(!caps.supports_gameboxes);
            } else if mode.is_awd() {
                assert!(!caps.supports_instances);
                assert!(caps.supports_gameboxes);
                assert!(caps.supports_wireguard);
            } else {
                assert!(caps.supports_instances);
                assert!(caps.supports_gameboxes);
                assert!(!caps.supports_wireguard, "AWDP V1 不使用 WireGuard");
                assert!(!caps.supports_standard_flag_submission);
            }
        }
    }
}
