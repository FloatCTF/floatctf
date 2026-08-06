//! Jeopardy competition mode variants (practice / single / team).
//!
//! Shared engine lives in `event::jeopardy`; modes only hold policy + thin entry points.

mod practice;
mod single;
mod team;

pub use practice::{JeopardyPracticePolicy, JeopardyPracticeServices};
pub use single::{JeopardySinglePolicy, JeopardySingleServices};
pub use team::{JeopardyTeamPolicy, JeopardyTeamServices};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JeopardyMode {
    Practice,
    Single,
    Team,
}

impl JeopardyMode {
    pub fn requires_team(self) -> bool {
        matches!(self, Self::Team)
    }

    pub fn allow_repeat_solve(self) -> bool {
        matches!(self, Self::Practice)
    }

    pub fn contributes_to_official_score(self) -> bool {
        !matches!(self, Self::Practice)
    }
}
