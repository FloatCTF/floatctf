//! Unified competition / event module (`modules/event`).
//!
//! Identity: EventFamily × EventPurpose × ParticipantMode (`EventMode`).
//! Engines: `jeopardy` (practice / individual / team) and `awd` (competition team).

pub mod common;
pub mod registry;

pub mod awd;
pub(crate) mod jeopardy;

mod error;

pub use error::{EventError, EventResult};
pub use registry::{EventModuleRegistry, EventServices};
