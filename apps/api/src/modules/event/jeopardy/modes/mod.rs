//! Jeopardy competition mode variants (practice / individual / team).
//!
//! Shared engine lives in `event::jeopardy`; modes only hold policy + thin entry points.

mod practice;
mod single;
mod team;

pub use practice::{JeopardyPracticePolicy, JeopardyPracticeServices};
pub use single::{JeopardySinglePolicy, JeopardySingleServices};
pub use team::{JeopardyTeamPolicy, JeopardyTeamServices};
