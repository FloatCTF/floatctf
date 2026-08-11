//! 赛事墙钟时间状态（公共层；与 AWD 运行态无关）。

use chrono::{DateTime, Utc};

use crate::entity::events;
use crate::entity::sea_orm_active_enums::EventPurpose;

/// 由赛事起止时间推导的时间窗状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTimeStatus {
    NotStarted,
    Ongoing,
    Ended,
}

/// 计算赛事时间状态。
///
/// 练习（`end_time = NULL`）：未到 `start` 为未开始，之后为进行中（永不因墙钟结束）。
/// 竞赛：按起止时间标准窗口判定。
pub fn event_time_status(
    start_time: DateTime<chrono::FixedOffset>,
    end_time: Option<DateTime<chrono::FixedOffset>>,
    purpose: &EventPurpose,
    now: DateTime<Utc>,
) -> EventTimeStatus {
    let now = now.fixed_offset();
    if now < start_time {
        return EventTimeStatus::NotStarted;
    }
    match purpose {
        EventPurpose::Practice => EventTimeStatus::Ongoing,
        EventPurpose::Competition => match end_time {
            Some(end) if now > end => EventTimeStatus::Ended,
            _ => EventTimeStatus::Ongoing,
        },
    }
}

pub fn event_time_status_of(event: &events::Model, now: DateTime<Utc>) -> EventTimeStatus {
    event_time_status(event.start_time, event.end_time, &event.purpose, now)
}
