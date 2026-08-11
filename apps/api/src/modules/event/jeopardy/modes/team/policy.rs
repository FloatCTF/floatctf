//! Policy for Jeopardy Team Competition.

use crate::modules::event::common::domain::event_mode::EventMode;
use crate::modules::event::jeopardy::domain::policy::{JeopardyModePolicy, JeopardyPolicy};

#[derive(Debug, Default, Clone, Copy)]
pub struct JeopardyTeamPolicy;

impl JeopardyModePolicy for JeopardyTeamPolicy {
    fn policy(&self) -> JeopardyPolicy {
        JeopardyPolicy::from_mode(EventMode::jeopardy_team_competition())
            .expect("team competition mode valid")
    }
}
