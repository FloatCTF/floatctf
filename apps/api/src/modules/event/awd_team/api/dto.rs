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
    /// 封禁时长（秒）；设置后创建自动解封任务（P4-7）。
    #[serde(default)]
    pub duration_secs: Option<i64>,
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
    pub event_gamebox_id: Uuid,
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

// ════════════════════════════════════════════════════════════════════════════
// GameBox 领域（§46 术语统一：gamebox / gamebox_revision / event_gamebox / gamebox_instance）
// ════════════════════════════════════════════════════════════════════════════

/// 管理员编辑 GameBox 时的运行时配置（= Revision 内容）。
#[derive(Debug, Deserialize)]
pub struct GameBoxConfigPayload {
    #[serde(default)]
    pub source_toml: String,
    pub image_ref: String,
    #[serde(default)]
    pub image_digest: Option<String>,
    #[serde(default = "default_gb_username")]
    pub username: String,
    #[serde(default = "default_gb_cpu")]
    pub cpu_millis: i64,
    #[serde(default = "default_gb_memory")]
    pub memory_bytes: i64,
    #[serde(default = "default_gb_pids")]
    pub pids_limit: i64,
    #[serde(default)]
    pub healthcheck: Option<serde_json::Value>,
    #[serde(default)]
    pub judge_script_name: Option<String>,
    #[serde(default)]
    pub judge_script_content: Option<String>,
    #[serde(default)]
    pub judge_args: Option<serde_json::Value>,
    #[serde(default)]
    pub judge_timeout_secs: Option<i32>,
    #[serde(default)]
    pub judge_retry_interval_secs: Option<i32>,
}

fn default_gb_username() -> String {
    "ctf".into()
}
fn default_gb_cpu() -> i64 {
    1000
}
fn default_gb_memory() -> i64 {
    512 * 1024 * 1024
}
fn default_gb_pids() -> i64 {
    100
}

impl GameBoxConfigPayload {
    pub fn into_config(
        self,
    ) -> crate::modules::event::awd_team::service::gamebox_service::GameBoxConfig {
        crate::modules::event::awd_team::service::gamebox_service::GameBoxConfig {
            source_toml: self.source_toml,
            image_ref: self.image_ref,
            image_digest: self.image_digest,
            username: self.username,
            cpu_millis: self.cpu_millis,
            memory_bytes: self.memory_bytes,
            pids_limit: self.pids_limit,
            healthcheck: self.healthcheck,
            judge_script_name: self.judge_script_name,
            judge_script_content: self.judge_script_content,
            judge_args_json: self.judge_args,
            judge_timeout_secs: self.judge_timeout_secs,
            judge_retry_interval_secs: self.judge_retry_interval_secs,
        }
    }
}

/// POST /api/admin/awd/gameboxes
#[derive(Debug, Deserialize)]
pub struct CreateGameBoxRequest {
    pub name: String,
    /// 可选；缺省由 name slug 生成（自动去重）。
    #[serde(default)]
    pub safe_name: Option<String>,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub hidden: bool,
    pub config: GameBoxConfigPayload,
}

fn default_category() -> String {
    "other".into()
}

/// POST /api/admin/awd/gameboxes/{gamebox_id}/revisions（编辑 → Revision N+1）
#[derive(Debug, Deserialize)]
pub struct EditGameBoxRevisionRequest {
    pub config: GameBoxConfigPayload,
}

/// POST /api/admin/events/{event_id}/awd/gameboxes（赛事选择）
#[derive(Debug, Deserialize)]
pub struct AddEventGameBoxRequest {
    pub gamebox_id: Uuid,
    /// 可选：pin 的具体 revision；缺省使用 latest。
    #[serde(default)]
    pub revision_id: Option<Uuid>,
    /// 可选：确定性 IP 偏移（2..254）；缺省自动分配未占用值。
    #[serde(default)]
    pub host_offset: Option<i16>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default = "default_break_points")]
    pub break_points: i64,
    #[serde(default = "default_loss_points")]
    pub loss_points: i64,
    #[serde(default = "default_fix_points")]
    pub fix_points: i64,
    #[serde(default = "default_down_points")]
    pub down_points: i64,
    #[serde(default = "default_first_bonus")]
    pub first_bonus: i64,
}

fn default_break_points() -> i64 {
    100
}
fn default_loss_points() -> i64 {
    100
}
fn default_fix_points() -> i64 {
    100
}
fn default_down_points() -> i64 {
    200
}
fn default_first_bonus() -> i64 {
    20
}

/// PATCH /api/admin/events/{event_id}/awd/gameboxes/{event_gamebox_id}
#[derive(Debug, Deserialize)]
pub struct UpdateEventGameBoxRequest {
    #[serde(default)]
    pub revision_id: Option<Uuid>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub cpu_millis: Option<i64>,
    #[serde(default)]
    pub memory_bytes: Option<i64>,
    #[serde(default)]
    pub pids_limit: Option<i64>,
    #[serde(default)]
    pub judge_timeout_secs: Option<Option<i32>>,
    #[serde(default)]
    pub judge_retry_interval_secs: Option<Option<i32>>,
    #[serde(default)]
    pub break_points: Option<i64>,
    #[serde(default)]
    pub loss_points: Option<i64>,
    #[serde(default)]
    pub fix_points: Option<i64>,
    #[serde(default)]
    pub down_points: Option<i64>,
    #[serde(default)]
    pub first_bonus: Option<i64>,
}

// ── 响应 DTO ──

#[derive(Debug, Serialize)]
pub struct GameBoxRevisionDto {
    pub id: Uuid,
    pub revision_number: i32,
    pub image_ref: String,
    pub image_digest: Option<String>,
    pub username: String,
    pub spec_digest: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<&crate::entity::gamebox_revisions::Model> for GameBoxRevisionDto {
    fn from(r: &crate::entity::gamebox_revisions::Model) -> Self {
        Self {
            id: r.id,
            revision_number: r.revision_number,
            image_ref: r.image_ref.clone(),
            image_digest: r.image_digest.clone(),
            username: r.username.clone(),
            spec_digest: r.spec_digest.clone(),
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GameBoxLibraryDto {
    pub id: Uuid,
    pub name: String,
    pub safe_name: String,
    pub category: String,
    pub description: String,
    pub hidden: bool,
    pub latest_revision: Option<GameBoxRevisionDto>,
}

#[derive(Debug, Serialize)]
pub struct EventGameBoxDto {
    pub id: Uuid,
    pub gamebox_id: Uuid,
    pub gamebox_name: String,
    pub gamebox_safe_name: String,
    pub revision_id: Uuid,
    pub revision_number: i32,
    pub host_offset: i16,
    pub enabled: bool,
    pub hidden: bool,
    pub cpu_millis: i64,
    pub memory_bytes: i64,
    pub pids_limit: i64,
    pub judge_timeout_secs: Option<i32>,
    pub judge_retry_interval_secs: Option<i32>,
    pub break_points: i64,
    pub loss_points: i64,
    pub fix_points: i64,
    pub down_points: i64,
    pub first_bonus: i64,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}
