//! 实例生命周期操作产生的领域结果类型。

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
