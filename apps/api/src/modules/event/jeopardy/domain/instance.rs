//! Domain results produced by instance lifecycle operations.

use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
pub struct CleanupFailure {
    pub instance_id: Uuid,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CleanupReport {
    pub completed: Vec<Uuid>,
    pub failed: Vec<CleanupFailure>,
}
