//! 平台调度引擎与任务注册。

pub mod engine;
mod handlers;
mod task_key;

pub use engine::{TaskHandler, TaskRegistry, TaskScheduler, recover_recurring_task};
pub use handlers::{
    CheckPracticeEventHandler, CleanRunningInstancesHandler, CleanUnusedRustFSFilesHandler,
};
pub use task_key::TaskKey;
