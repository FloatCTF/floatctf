//! Jeopardy 引擎（`EventFamily::Jeopardy`）。
//!
//! 应用用例按 `EventPurpose`（练习/竞赛）与 `ParticipantMode`（个人/战队）分支。
//! AWD 为独立引擎，见 `modules::event::awd`。

pub mod api;
pub mod application;
pub(crate) mod domain;
pub mod infrastructure;

// crate 内部调用方的便捷再导出
pub(crate) use application::instance_service::InstanceService;
pub(crate) use application::submission_service::{JeopardySubmissionService, submit_practice};
pub(crate) use domain::policy::JeopardyPolicy;
pub(crate) use domain::scoreboard::{ChallengeScoreboard, ScoreboardItem};
pub(crate) use domain::scoring::{calculate_next_dynamic_score, dynamic_score};
pub(crate) use domain::solve::{JeopardySubmitRequest, SolveSubject};
pub(crate) use domain::trend::{TrendItem, TrendPoint};
