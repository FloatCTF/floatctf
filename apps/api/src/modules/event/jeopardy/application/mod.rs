//! Jeopardy application use cases (Purpose × ParticipantMode driven).

pub mod common;
pub mod context;
pub mod instance;
pub mod instance_service;
pub mod participant;
pub mod scoreboard;
pub mod submission_service;
pub mod submit;
pub mod trend;
pub mod writeup;

pub use context::{EventContext, EventContextBuilder, ModeInstanceResult, SubmitFlagRequest};
pub use instance_service::InstanceService;
pub use submission_service::{JeopardySubmissionService, submit_practice};
