//! Jeopardy shared application services.

pub mod common;
pub mod context;
pub mod core;
pub mod instance_service;
pub mod submission_service;

pub use context::{EventContext, EventContextBuilder, ModeInstanceResult, SubmitFlagRequest};
pub use instance_service::InstanceService;
pub use submission_service::{JeopardySubmissionService, submit_practice};
