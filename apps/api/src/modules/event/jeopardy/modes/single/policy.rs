//! Policy for jeopardy_single (formal individual contest).

use crate::entity::sea_orm_active_enums::EventType;
use crate::modules::event::common::domain::capability::ParticipantMode;
use crate::modules::event::jeopardy::domain::policy::{JeopardyModePolicy, ScoreOwner};

#[derive(Debug, Default, Clone, Copy)]
pub struct JeopardySinglePolicy;

impl JeopardyModePolicy for JeopardySinglePolicy {
    fn event_type(&self) -> EventType {
        EventType::JeopardySingle
    }

    fn participant_mode(&self) -> ParticipantMode {
        ParticipantMode::Individual
    }

    fn score_owner(&self) -> ScoreOwner {
        ScoreOwner::User
    }

    fn allow_repeat_solve(&self) -> bool {
        false
    }

    fn requires_team(&self) -> bool {
        false
    }

    fn instance_ref_label(&self) -> &'static str {
        "JeopardySingle"
    }

    fn max_concurrent_instances(&self, _team_member_count: Option<u64>) -> u64 {
        2
    }
}
