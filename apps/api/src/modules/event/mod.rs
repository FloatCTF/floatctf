//! Unified competition / event module (`modules/event`).
//!
//! Identity: EventFamily × EventPurpose × ParticipantMode (`EventMode`).
//! Engines: `jeopardy` and `awd`.

pub mod common;

pub mod awd;
pub(crate) mod jeopardy;

mod error;

pub use error::{EventError, EventResult};
