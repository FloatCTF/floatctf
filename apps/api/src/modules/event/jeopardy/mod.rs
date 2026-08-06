//! Shared Jeopardy engine (not a DB event_type).
//!
//! Instance lifecycle, flag submission, scoring, scoreboard types, and launch
//! helpers used by `modes::{practice,single,team}`.

pub mod api;
pub(crate) mod application;
pub(crate) mod domain;
pub(crate) mod infrastructure;
pub mod modes;

// Convenience re-exports for crate-internal callers (scheduler, modes).
pub(crate) use application::instance_service::InstanceService;
pub(crate) use application::submission_service::{JeopardySubmissionService, submit_practice};
pub(crate) use domain::scoreboard::{ChallengeScoreboard, ScoreboardItem};
pub(crate) use domain::scoring::{calculate_next_dynamic_score, dynamic_score};
pub(crate) use domain::solve::{JeopardySubmitRequest, SolveSubject};
pub(crate) use domain::trend::{TrendItem, TrendPoint};

pub use modes::JeopardyMode;
