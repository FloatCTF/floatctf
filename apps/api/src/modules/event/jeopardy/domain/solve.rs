//! Submission request/value types shared by application services.

use uuid::Uuid;

/// Who receives credit for a Jeopardy solve and how Instance is scoped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveSubject {
    /// Jeopardy single: per User.
    User,
    /// Jeopardy team: per Team (submitting user still recorded).
    Team,
}

/// Input for a formal Jeopardy flag submission (not practice).
#[derive(Debug, Clone)]
pub struct JeopardySubmitRequest {
    pub event_id: Uuid,
    pub user_id: Uuid,
    pub instance_id: Uuid,
    pub flag: String,
    pub subject: SolveSubject,
}
