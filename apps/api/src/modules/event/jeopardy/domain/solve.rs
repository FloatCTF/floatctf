//! Submission request/value types shared by application services.

use uuid::Uuid;

/// Who receives credit for a Jeopardy solve and how instances are scoped.
///
/// Driven by [`crate::entity::sea_orm_active_enums::ParticipantMode`]:
/// - Individual → [`SolveSubject::User`]
/// - Team → [`SolveSubject::Team`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveSubject {
    /// Per-user ownership / scoring (Individual participant mode).
    User,
    /// Per-team ownership / scoring (Team participant mode; acting user still recorded).
    Team,
}

impl SolveSubject {
    pub fn is_team(self) -> bool {
        matches!(self, Self::Team)
    }
}

/// Input for a formal Jeopardy flag submission (competition scoring path).
#[derive(Debug, Clone)]
pub struct JeopardySubmitRequest {
    pub event_id: Uuid,
    pub user_id: Uuid,
    pub instance_id: Uuid,
    pub flag: String,
    pub subject: SolveSubject,
}
