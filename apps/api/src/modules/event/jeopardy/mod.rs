//! Jeopardy engine (EventFamily::Jeopardy).
//!
//! Application use cases branch on EventPurpose and ParticipantMode.
//! AWD is a separate engine under `modules::event::awd`.

pub mod api;
pub(crate) mod application;
pub(crate) mod domain;
pub(crate) mod infrastructure;

// Convenience re-exports for crate-internal callers.
pub(crate) use application::instance_service::InstanceService;
pub(crate) use application::submission_service::{JeopardySubmissionService, submit_practice};
pub(crate) use domain::policy::JeopardyPolicy;
pub(crate) use domain::scoreboard::{ChallengeScoreboard, ScoreboardItem};
pub(crate) use domain::scoring::{calculate_next_dynamic_score, dynamic_score};
pub(crate) use domain::solve::{JeopardySubmitRequest, SolveSubject};
pub(crate) use domain::trend::{TrendItem, TrendPoint};
