//! Jeopardy mode policy — narrow trait for practice / single / team differences.

use crate::entity::sea_orm_active_enums::EventType;
use crate::modules::event::common::domain::capability::{EventCapabilities, ParticipantMode};

/// Who receives score credit for a formal Jeopardy solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreOwner {
    User,
    Team,
}

/// Policy surface for Jeopardy competition modes (not AWD).
pub trait JeopardyModePolicy: Send + Sync {
    fn event_type(&self) -> EventType;
    fn participant_mode(&self) -> ParticipantMode;
    fn score_owner(&self) -> ScoreOwner;
    fn allow_repeat_solve(&self) -> bool;
    fn requires_team(&self) -> bool;
    fn capabilities(&self) -> EventCapabilities {
        EventCapabilities::for_event_type(&self.event_type())
    }
    /// `instances.ref` label and launch identifier prefix.
    fn instance_ref_label(&self) -> &'static str;
    /// Max concurrent running instances for a participant (user or team).
    fn max_concurrent_instances(&self, team_member_count: Option<u64>) -> u64;
}
