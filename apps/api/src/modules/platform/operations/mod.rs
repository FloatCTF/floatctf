//! 平台运营类接口（仪表盘等）。

pub mod dashboard;
pub mod database;
pub mod docker;
pub mod logs;
mod logs_dto;
pub mod runtime_instances;
pub mod scheduled_tasks;
mod scheduled_tasks_dto;
pub mod system;
pub mod terminal;

pub use logs_dto::LogsDto;
pub use scheduled_tasks_dto::ScheduledTasksDto;
