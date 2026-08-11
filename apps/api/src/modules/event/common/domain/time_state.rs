//! Event wall-clock time status (common layer; distinct from AWD runtime state).

use chrono::{DateTime, Utc};

use crate::entity::events;
use crate::entity::sea_orm_active_enums::EventPurpose;

/// Time-window status derived from event start/end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTimeStatus {
    NotStarted,
    Ongoing,
    Ended,
}

/// Compute time status for an event.
///
/// Practice (`end_time = NULL`): NotStarted if now < start, else Ongoing (never Ended).
/// Competition: standard start/end window.
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
