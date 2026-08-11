//! Policy for Jeopardy Practice (system-managed individual training).

use crate::modules::event::common::domain::event_mode::EventMode;
use crate::modules::event::jeopardy::domain::policy::{JeopardyModePolicy, JeopardyPolicy};

#[derive(Debug, Default, Clone, Copy)]
pub struct JeopardyPracticePolicy;

impl JeopardyModePolicy for JeopardyPracticePolicy {
    fn policy(&self) -> JeopardyPolicy {
        JeopardyPolicy::from_mode(EventMode::jeopardy_practice()).expect("practice mode valid")
    }
}
