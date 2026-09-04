//! 跨模块核心原语：配置、密钥、鉴权、系统固定主键等。

pub mod config;
pub mod secret;
pub mod security;
pub mod system_ids;

pub use config::AppConfig;
pub use secret::Secret;
pub use system_ids::{
    EVENT_PRACTICE_JEOPARDY, EVENT_PRACTICE_JEOPARDY_SYSTEM_KEY, SCHED_CHECK_PRACTICE_EVENT,
    SCHED_CLEAN_INSTANCES, SCHED_CLEAN_RUSTFS, startup_scheduled_task_seeds,
};
