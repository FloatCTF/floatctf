//! Unified competition / event module (`modules/event`).
//!
//! Modes: jeopardy (practice/single/team under `jeopardy/modes`), awd_team.
//! Shared Jeopardy engine lives in `jeopardy` (not a DB event_type).

pub mod common;
pub mod registry;

pub mod awd_team;
pub(crate) mod jeopardy;

mod error;

pub use error::{EventError, EventResult};
pub use registry::{EventModuleRegistry, EventServices};
