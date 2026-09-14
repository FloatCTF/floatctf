//! Jeopardy 共享领域类型。

pub mod instance;
pub mod policy;
pub mod scoreboard;
pub mod scoring;
pub mod solve;
pub mod trend;

pub use instance::{CleanupFailure, CleanupReport};
pub use policy::JeopardyPolicy;
pub use scoreboard::{ChallengeScoreboard, ScoreboardItem};
pub use scoring::{calculate_next_dynamic_score, dynamic_score};
pub use solve::{JeopardySubmitRequest, SolveSubject};
pub use trend::{TrendItem, TrendPoint};
