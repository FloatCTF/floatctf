//! 积分趋势投影类型。

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrendPoint {
    pub name: String,
    pub score: f64,
    pub time: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrendItem {
    pub name: String,
    pub points: Vec<TrendPoint>,
}
