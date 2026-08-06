use crate::entity::scheduled_tasks;
use sea_orm::entity::prelude::{DateTimeWithTimeZone, Uuid};
use serde::Serialize;
use serde_json::Value as Json;

#[derive(Debug, Serialize)]
pub struct ScheduledTasksDto {
    pub id: Uuid,
    pub group_id: Option<Uuid>,
    pub task_key: String,
    pub trigger_type: String,
    pub status: String,
    pub cron_expr: Option<String>,
    pub execute_at: Option<DateTimeWithTimeZone>,
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub payload: Option<Json>,
    pub error_msg: Option<String>,
    pub last_run_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub task_name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub protected: bool,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub timeout_secs: Option<i32>,
    pub last_error: Option<String>,
    pub locked_at: Option<DateTimeWithTimeZone>,
    pub heartbeat_at: Option<DateTimeWithTimeZone>,
}

impl From<scheduled_tasks::Model> for ScheduledTasksDto {
    fn from(m: scheduled_tasks::Model) -> Self {
        Self {
            id: m.id,
            group_id: m.group_id,
            task_key: m.task_key,
            trigger_type: m.trigger_type,
            status: m.status,
            cron_expr: m.cron_expr,
            execute_at: m.execute_at,
            expires_at: m.expires_at,
            payload: m.payload,
            error_msg: m.error_msg,
            last_run_at: m.last_run_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
            task_name: m.task_name,
            description: m.description,
            enabled: m.enabled,
            protected: m.protected,
            attempt_count: m.attempt_count,
            max_attempts: m.max_attempts,
            timeout_secs: m.timeout_secs,
            last_error: m.last_error,
            locked_at: m.locked_at,
            heartbeat_at: m.heartbeat_at,
        }
    }
}
