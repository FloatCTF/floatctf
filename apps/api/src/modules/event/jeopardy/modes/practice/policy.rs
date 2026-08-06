//! Policy for jeopardy_practice (individual training).

use crate::entity::sea_orm_active_enums::EventType;
use crate::modules::event::common::domain::capability::ParticipantMode;
use crate::modules::event::jeopardy::domain::policy::{JeopardyModePolicy, ScoreOwner};

#[derive(Debug, Default, Clone, Copy)]
pub struct JeopardyPracticePolicy;

impl JeopardyModePolicy for JeopardyPracticePolicy {
    fn event_type(&self) -> EventType {
        EventType::JeopardyPractice
    }

    fn participant_mode(&self) -> ParticipantMode {
        ParticipantMode::Individual
    }

    fn score_owner(&self) -> ScoreOwner {
        ScoreOwner::User
    }

    fn allow_repeat_solve(&self) -> bool {
        // Practice records challenge_solves without event ranking; re-practice allowed.
        true
    }

    fn requires_team(&self) -> bool {
        false
    }

    fn instance_ref_label(&self) -> &'static str {
        "JeopardyPractice"
    }

    fn max_concurrent_instances(&self, _team_member_count: Option<u64>) -> u64 {
        1
    }
}
