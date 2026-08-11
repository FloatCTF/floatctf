//! Policy for Jeopardy Individual Competition.

use crate::modules::event::common::domain::event_mode::EventMode;
use crate::modules::event::jeopardy::domain::policy::{JeopardyModePolicy, JeopardyPolicy};

#[derive(Debug, Default, Clone, Copy)]
pub struct JeopardySinglePolicy;

impl JeopardyModePolicy for JeopardySinglePolicy {
    fn policy(&self) -> JeopardyPolicy {
        JeopardyPolicy::from_mode(EventMode::jeopardy_individual_competition())
            .expect("individual competition mode valid")
    }
}
