//! 积分榜投影类型（Jeopardy 对外表面）。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeScoreboard {
    pub name: String,
    pub solved: bool,
    pub solved_no: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScoreboardItem {
    pub id: Uuid,
    pub no: u64,
    pub name: String,
    pub avatar: Option<String>,
    pub score: f64,
    pub solved_count: u64,
    pub challenges: Vec<ChallengeScoreboard>,
}
