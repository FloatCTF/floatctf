//! Policy for jeopardy_team (formal team contest).

use crate::entity::sea_orm_active_enums::EventType;
use crate::modules::event::common::domain::capability::ParticipantMode;
use crate::modules::event::jeopardy::domain::policy::{JeopardyModePolicy, ScoreOwner};

#[derive(Debug, Default, Clone, Copy)]
pub struct JeopardyTeamPolicy;

impl JeopardyModePolicy for JeopardyTeamPolicy {
    fn event_type(&self) -> EventType {
        EventType::JeopardyTeam
    }

    fn participant_mode(&self) -> ParticipantMode {
        ParticipantMode::Team
    }

    fn score_owner(&self) -> ScoreOwner {
        ScoreOwner::Team
    }

    fn allow_repeat_solve(&self) -> bool {
        false
    }

    fn requires_team(&self) -> bool {
        true
    }

    fn instance_ref_label(&self) -> &'static str {
        "JeopardyTeam"
    }

    fn max_concurrent_instances(&self, team_member_count: Option<u64>) -> u64 {
        team_member_count.unwrap_or(1).saturating_mul(2)
    }
}
