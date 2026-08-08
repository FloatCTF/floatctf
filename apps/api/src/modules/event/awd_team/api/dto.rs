//! AWD API data transfer objects.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Admin request DTOs ──

#[derive(Debug, Deserialize)]
pub struct CreateAwdEventRequest {
    pub event_id: Uuid,
    pub gamebox_cidr: String,
    pub wireguard_cidr: String,
    pub wireguard_interface_name: String,
    pub wireguard_listen_port: i32,
    pub flagserver_ip: String,
    pub judgeserver_ip: String,
    #[serde(default = "default_round_duration")]
    pub round_duration_secs: i32,
    /// 计划开始时间（可选；设置后创建 awd.event.start 一次性任务，P2-12）。
    #[serde(default)]
    pub planned_start_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

fn default_round_duration() -> i32 {
    300
}

#[derive(Debug, Deserialize)]
pub struct DeployEventRequest {
    pub event_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct PrecheckRequest {
    pub event_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct BanTeamRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ScoreAdjustRequest {
    pub team_id: Uuid,
    pub delta: i64,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetGameBoxRequest {
    pub instance_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct NetworkUpdateRequest {
    pub gamebox_cidr: Option<String>,
    pub wireguard_cidr: Option<String>,
    pub wireguard_interface_name: Option<String>,
    pub wireguard_listen_port: Option<i32>,
    pub flagserver_ip: Option<String>,
    pub judgeserver_ip: Option<String>,
}

// ── Player request DTOs ──

#[derive(Debug, Deserialize)]
pub struct SubmitFlagRequest {
    pub flag: String,
}

// ── Internal request DTOs ──

#[derive(Debug, Deserialize)]
pub struct IssueFlagInternalRequest {
    pub source_ip: String,
}

#[derive(Debug, Deserialize)]
pub struct JudgeCallbackRequest {
    pub task_id: Uuid,
    pub callback_id: String,
    pub status: String,
    pub attempt_count: i32,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

// ── Response DTOs ──

#[derive(Debug, Serialize)]
pub struct AwdEventResponse {
    pub id: Uuid,
    pub event_id: Uuid,
    pub status: String,
    pub phase: String,
    pub gamebox_cidr: String,
    pub wireguard_cidr: String,
    pub flagserver_ip: String,
    pub judgeserver_ip: String,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
pub struct GameBoxResponse {
    pub id: Uuid,
    pub team_id: Uuid,
    pub template_id: Uuid,
    pub status: String,
    pub gamebox_ip: String,
    pub container_name: String,
    pub health_status: String,
}

#[derive(Debug, Serialize)]
pub struct ScoreboardResponse {
    pub scores: Vec<super::super::domain::TeamScore>,
    pub round: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct WireGuardConfigResponse {
    pub config: String,
}

#[derive(Debug, Serialize)]
pub struct FlagIssueResponse {
    pub flag: String,
}

#[derive(Debug, Serialize)]
pub struct SubmissionResponse {
    pub success: bool,
    pub attack_score: i64,
    pub victim_loss: i64,
    pub first_bonus: i64,
    pub was_first_blood: bool,
}

#[derive(Debug, Serialize)]
pub struct PrecheckResponse {
    pub id: Uuid,
    pub status: String,
    pub config_check: Option<serde_json::Value>,
    pub container_check: Option<serde_json::Value>,
    pub network_check: Option<serde_json::Value>,
}

// ── Additional response DTOs ──

#[derive(Debug, Serialize)]
pub struct PrecheckRunDto {
    pub id: Uuid,
    pub event_id: Uuid,
    pub status: String,
    pub trigger: Option<String>,
    pub revision: Option<String>,
    pub error_msg: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JudgeBatchDto {
    pub id: Uuid,
    pub event_id: Uuid,
    pub round_id: Option<Uuid>,
    pub total_tasks: i32,
    pub completed_tasks: i32,
    pub failed_tasks: i32,
    pub status: String,
    pub created_at: Option<String>,
}
