//! AWD API 请求/响应 DTO。

use std::collections::BTreeMap;

use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{awd_events, gameboxes};

/// serde snake_case 序列化（AwdEventStatus/AwdPhase 无 Display）。
pub fn snake_str<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|x| x.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// AWD 赛事初始化状态（GET /api/admin/events/{event_id}/awd）。
/// 前端用它判断赛事是否已初始化（awd_events 行存在）以及当前生命周期状态。
#[derive(Debug, Serialize)]
pub struct AwdEventStatusDto {
    pub event_id: Uuid,
    pub status: String,
    pub phase: String,
    pub round_count: Option<i32>,
    pub round_duration_secs: i32,
    pub initial_score: i64,
    pub free_reset_count: i32,
    pub extra_reset_penalty: i64,
    pub judge_max_concurrency: i32,
    pub judge_default_timeout_secs: i32,
    pub judge_retry_interval_secs: i32,
    pub judge_grace_period_secs: i32,
    pub archive_retention_hours: i32,
    /// `awd.event.start` 一次性任务的执行时间；None 表示手动开始。
    pub planned_start_at: Option<DateTimeWithTimeZone>,
    pub verified_at: Option<DateTimeWithTimeZone>,
    pub started_at: Option<DateTimeWithTimeZone>,
    pub updated_at: DateTimeWithTimeZone,
    /// 派生：最终轮次已完成且无活动轮次，Judge 仍在结算中。
    /// 此时 competition 操作已关闭，但 scoreboard 尚未最终稳定。
    pub final_settlement: bool,
}

impl From<awd_events::Model> for AwdEventStatusDto {
    fn from(m: awd_events::Model) -> Self {
        Self {
            event_id: m.event_id,
            status: snake_str(&m.status),
            phase: snake_str(&m.phase),
            round_count: m.round_count,
            round_duration_secs: m.round_duration_secs,
            initial_score: m.initial_score,
            free_reset_count: m.free_reset_count,
            extra_reset_penalty: m.extra_reset_penalty,
            judge_max_concurrency: m.judge_max_concurrency,
            judge_default_timeout_secs: m.judge_default_timeout_secs,
            judge_retry_interval_secs: m.judge_retry_interval_secs,
            judge_grace_period_secs: m.judge_grace_period_secs,
            archive_retention_hours: m.archive_retention_hours,
            planned_start_at: None,
            verified_at: m.verified_at,
            started_at: m.started_at,
            updated_at: m.updated_at,
            final_settlement: false, // set by handler after computing from latest round
        }
    }
}

/// 选手端 AWD 赛事状态快照（GET /api/events/{event_id}/awd/status）。
#[derive(Debug, Serialize)]
pub struct AwdPlayerStatusDto {
    pub event_id: Uuid,
    pub status: String,
    pub phase: String,
    pub current_round: Option<i32>,
    pub round_count: Option<i32>,
    pub banned: bool,
    pub score: Option<i64>,
    /// 派生：最终轮次已完成且无活动轮次，Judge 仍在结算中。
    pub final_settlement: bool,
}

// ── Admin request DTOs ──

#[derive(Debug, Deserialize)]
pub struct CreateAwdEventRequest {
    pub event_id: Uuid,
    #[serde(flatten)]
    pub config: AwdEventConfigRequest,
}

/// Configure 页 AWD 专属参数。全部字段可选以支持 PATCH；首次保存缺省时使用
/// 与数据库一致的默认值。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AwdEventConfigRequest {
    /// PATCH 乐观锁版本；首次创建可省略。
    pub expected_updated_at: Option<DateTimeWithTimeZone>,
    pub round_count: Option<i32>,
    pub round_duration_secs: Option<i32>,
    pub initial_score: Option<i64>,
    pub free_reset_count: Option<i32>,
    pub extra_reset_penalty: Option<i64>,
    pub judge_max_concurrency: Option<i32>,
    pub judge_default_timeout_secs: Option<i32>,
    pub judge_retry_interval_secs: Option<i32>,
    pub judge_grace_period_secs: Option<i32>,
    pub archive_retention_hours: Option<i32>,
    /// 计划开始时间；留空且 clear_planned_start=true 时删除已有任务。
    pub planned_start_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[serde(default)]
    pub clear_planned_start: bool,
    /// 捕获旧字段/拼写错误，避免 Serde 默认忽略后返回“成功但未生效”。
    #[serde(flatten)]
    unknown_fields: BTreeMap<String, serde_json::Value>,
}

impl AwdEventConfigRequest {
    pub fn has_changes(&self) -> bool {
        self.round_count.is_some()
            || self.round_duration_secs.is_some()
            || self.initial_score.is_some()
            || self.free_reset_count.is_some()
            || self.extra_reset_penalty.is_some()
            || self.judge_max_concurrency.is_some()
            || self.judge_default_timeout_secs.is_some()
            || self.judge_retry_interval_secs.is_some()
            || self.judge_grace_period_secs.is_some()
            || self.archive_retention_hours.is_some()
            || self.planned_start_at.is_some()
            || self.clear_planned_start
    }

    pub fn validate(&self) -> crate::modules::event::awd::AwdResult<()> {
        use crate::modules::event::awd::AwdError;

        if !self.unknown_fields.is_empty() {
            return Err(AwdError::Validation(format!(
                "unknown AWD configuration fields: {}",
                self.unknown_fields
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if self.clear_planned_start && self.planned_start_at.is_some() {
            return Err(AwdError::Validation(
                "planned_start_at and clear_planned_start cannot be used together".into(),
            ));
        }
        if let Some(start_at) = self.planned_start_at
            && start_at <= chrono::Utc::now()
        {
            return Err(AwdError::Validation(
                "planned_start_at must be in the future".into(),
            ));
        }
        Ok(())
    }
}

impl From<AwdEventConfigRequest>
    for crate::modules::event::awd::service::config_service::AwdEventConfigPatch
{
    fn from(value: AwdEventConfigRequest) -> Self {
        Self {
            expected_updated_at: value.expected_updated_at,
            round_count: value.round_count,
            round_duration_secs: value.round_duration_secs,
            initial_score: value.initial_score,
            free_reset_count: value.free_reset_count,
            extra_reset_penalty: value.extra_reset_penalty,
            judge_max_concurrency: value.judge_max_concurrency,
            judge_default_timeout_secs: value.judge_default_timeout_secs,
            judge_retry_interval_secs: value.judge_retry_interval_secs,
            judge_grace_period_secs: value.judge_grace_period_secs,
            archive_retention_hours: value.archive_retention_hours,
            planned_start_at: if value.clear_planned_start {
                Some(None)
            } else {
                value.planned_start_at.map(Some)
            },
        }
    }
}

#[cfg(test)]
mod config_request_tests {
    use super::*;

    #[test]
    fn planned_start_set_and_clear_are_mutually_exclusive() {
        let request = AwdEventConfigRequest {
            planned_start_at: Some(
                (chrono::Utc::now() + chrono::Duration::hours(1)).fixed_offset(),
            ),
            clear_planned_start: true,
            ..Default::default()
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn planned_start_must_be_in_the_future() {
        let request = AwdEventConfigRequest {
            planned_start_at: Some(
                (chrono::Utc::now() - chrono::Duration::seconds(1)).fixed_offset(),
            ),
            ..Default::default()
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn expected_version_alone_is_not_a_change() {
        let request = AwdEventConfigRequest {
            expected_updated_at: Some(chrono::Utc::now().fixed_offset()),
            ..Default::default()
        };
        assert!(!request.has_changes());
    }

    #[test]
    fn rejects_legacy_or_misspelled_fields() {
        let request: AwdEventConfigRequest = serde_json::from_value(serde_json::json!({
            "round_retry_interval_secs": 5
        }))
        .expect("unknown fields are captured for a validation error");
        assert!(request.validate().is_err());
    }

    #[test]
    fn flattened_create_request_accepts_known_config_fields() {
        let request: CreateAwdEventRequest = serde_json::from_value(serde_json::json!({
            "event_id": Uuid::new_v4(),
            "round_duration_secs": 300
        }))
        .expect("known flattened config fields must deserialize");
        assert_eq!(request.config.round_duration_secs, Some(300));
        assert!(request.config.validate().is_ok());
    }
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
    /// 分配模式：automatic（默认）/ manual
    #[serde(default)]
    pub allocation_mode: Option<String>,
    pub gamebox_cidr: Option<String>,
    pub wireguard_cidr: Option<String>,
    pub wireguard_listen_port: Option<i32>,
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
    pub verified: bool,
}

/// Event Network 响应（赛事网络页，§22/§64）。
#[derive(Debug, Serialize)]
pub struct EventNetworkResponse {
    pub event_id: Uuid,
    pub allocation_mode: String,
    pub gamebox_cidr: String,
    pub wireguard_cidr: String,
    pub infrastructure_subnet: String,
    pub flagserver_ip: String,
    pub judgeserver_ip: String,
    pub wireguard_interface_name: String,
    pub wireguard_listen_port: i32,
    pub docker_network_name: String,
    pub locked: bool,
}

#[derive(Debug, Serialize)]
pub struct GameBoxResponse {
    pub id: Uuid,
    pub team_id: Uuid,
    pub event_gamebox_id: Uuid,
    /// GameBox 身份名（从 EventGameBox → GameBox identity 批量解析）。
    pub gamebox_name: String,
    pub status: String,
    pub gamebox_ip: String,
    pub container_name: String,
    pub health_status: String,
}

/// 单个 GameBox 实例的 SSH 连接信息（IP + 用户名）。
#[derive(Debug, Serialize)]
pub struct SshInstanceInfo {
    pub id: Uuid,
    pub gamebox_ip: String,
    pub username: String,
    pub container_name: String,
    pub health_status: String,
}

/// 队伍级 SSH 访问凭据（一队一密码，GameBox 领域模型 §22.1）。
#[derive(Debug, Serialize)]
pub struct SshAccessResponse {
    /// SSH 端口（固定 22）。
    pub port: u16,
    /// 团队共享 SSH 密码（解密后明文）。
    pub password: String,
    /// 本队各实例的连接信息。
    pub instances: Vec<SshInstanceInfo>,
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
// GameBox 领域（gamebox identity / gamebox_revision / event_gamebox / instance）
// ════════════════════════════════════════════════════════════════════════════

/// 字段显式 null → Some(None)（清空），缺失 → None（不更新）。
/// serde 默认把 JSON null 与缺失都映射为外层 None，无法表达“清空”。
fn deserialize_nullable<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<T>::deserialize(d)?))
}

/// PATCH /api/admin/awd/gameboxes/{gamebox_id} —— 身份 + 可编辑运行参数（不含 digest/镜像/build 状态）
#[derive(Debug, Deserialize)]
pub struct UpdateGameBoxIdentityRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hidden: Option<bool>,
    /// 容器内用户名（healthcheck/judge 执行用）。
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub recommended_cpu_millis: Option<i64>,
    #[serde(default)]
    pub recommended_memory_bytes: Option<i64>,
    #[serde(default)]
    pub recommended_pids_limit: Option<i64>,
    /// JSON 文本；null 清空。
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub healthchecks_json: Option<Option<String>>,
    #[serde(default)]
    pub judge_script_name: Option<String>,
    #[serde(default)]
    pub judge_script_content: Option<String>,
    /// JSON 文本；null 清空。
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub judge_args_json: Option<Option<String>>,
    /// null 清空（继承赛事默认）。
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub judge_timeout_secs: Option<Option<i32>>,
    /// null 清空（继承赛事默认）。
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub judge_retry_interval_secs: Option<Option<i32>>,
}

/// POST /api/admin/events/{event_id}/awd/gameboxes（赛事选择 GameBox 当前版本）
#[derive(Debug, Deserialize)]
pub struct AddEventGameBoxRequest {
    #[serde(default)]
    pub gamebox_id: Uuid,
    /// 可选：确定性 IP 偏移（2..254）；缺省自动分配未占用值。
    #[serde(default)]
    pub host_offset: Option<i16>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default = "default_attack_score")]
    pub attack_score: i64,
    #[serde(default = "default_judge_down_penalty")]
    pub judge_down_penalty: i64,
    #[serde(default = "default_first_bonus")]
    pub first_bonus: i64,
}

fn default_attack_score() -> i64 {
    100
}
fn default_judge_down_penalty() -> i64 {
    200
}
fn default_first_bonus() -> i64 {
    20
}

/// PATCH /api/admin/events/{event_id}/awd/gameboxes/{event_gamebox_id}
#[derive(Debug, Deserialize)]
pub struct UpdateEventGameBoxRequest {
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
    pub attack_score: Option<i64>,
    #[serde(default)]
    pub judge_down_penalty: Option<i64>,
    #[serde(default)]
    pub first_bonus: Option<i64>,
}

// ── 响应 DTO ──

/// GameBox 库行 = 身份 + 当前版本 package 摘要（单版本模型）。
#[derive(Debug, Serialize)]
pub struct GameBoxLibraryDto {
    pub id: Uuid,
    pub name: String,
    pub safe_name: String,
    pub category: String,
    pub description: String,
    pub hidden: bool,
    pub version: Option<String>,
    pub build_status: Option<String>,
    pub package_digest: Option<String>,
    pub image_ref: Option<String>,
    pub image_repo_digest: Option<String>,
    pub username: Option<String>,
    pub cpu_millis: Option<i64>,
    pub memory_bytes: Option<i64>,
    pub pids_limit: Option<i64>,
    pub healthchecks_json: Option<serde_json::Value>,
    pub judge_script_name: Option<String>,
    pub judge_script_content: Option<String>,
    pub judge_args_json: Option<serde_json::Value>,
    pub judge_timeout_secs: Option<i32>,
    pub judge_retry_interval_secs: Option<i32>,
}

impl From<&gameboxes::Model> for GameBoxLibraryDto {
    fn from(g: &gameboxes::Model) -> Self {
        Self {
            id: g.id,
            name: g.name.clone(),
            safe_name: g.safe_name.clone(),
            category: g.category.clone(),
            description: g.description.clone(),
            hidden: g.hidden,
            version: g.version.clone(),
            build_status: g.build_status.clone(),
            package_digest: g.package_digest.clone(),
            image_ref: g.image_ref.clone(),
            image_repo_digest: g.image_repo_digest.clone(),
            username: g.username.clone(),
            cpu_millis: Some(g.recommended_cpu_millis),
            memory_bytes: Some(g.recommended_memory_bytes),
            pids_limit: Some(g.recommended_pids_limit),
            healthchecks_json: g.healthchecks_json.clone(),
            judge_script_name: g.judge_script_name.clone(),
            judge_script_content: g.judge_script_content.clone(),
            judge_args_json: g.judge_args_json.clone(),
            judge_timeout_secs: g.judge_timeout_secs,
            judge_retry_interval_secs: g.judge_retry_interval_secs,
        }
    }
}

/// POST /api/admin/awd/gameboxes/import 响应
#[derive(Debug, Serialize)]
pub struct ImportGameBoxResponse {
    pub gamebox: GameBoxLibraryDto,
}

#[derive(Debug, Serialize)]
pub struct EventGameBoxDto {
    pub id: Uuid,
    pub gamebox_id: Uuid,
    pub gamebox_name: String,
    pub gamebox_safe_name: String,
    pub gamebox_version: Option<String>,
    pub host_offset: i16,
    pub enabled: bool,
    pub hidden: bool,
    pub cpu_millis: i64,
    pub memory_bytes: i64,
    pub pids_limit: i64,
    pub judge_timeout_secs: Option<i32>,
    pub judge_retry_interval_secs: Option<i32>,
    pub attack_score: i64,
    pub judge_down_penalty: i64,
    pub first_bonus: i64,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

// ── Wave 3: Judge Pull + Lease DTOs ──

#[derive(Debug, Deserialize)]
pub struct JudgeClaimRequest {
    pub worker_id: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct JudgeClaimResponse {
    pub tasks: Vec<ClaimedTaskDto>,
}

#[derive(Debug, Serialize)]
pub struct ClaimedTaskDto {
    pub task_id: Uuid,
    pub batch_id: Uuid,
    pub event_id: Uuid,
    pub round_id: Uuid,
    pub gamebox_instance_id: Uuid,
    pub event_gamebox_id: Option<Uuid>,
    pub team_id: Uuid,
    pub attempt: i32,
    pub lease_token: String,
    pub lease_expires_at: DateTimeWithTimeZone,
    pub deadline_at: DateTimeWithTimeZone,
    // Execution payload
    pub script_content: String,
    pub script_args_json: Option<String>,
    pub target_ip: String,
    pub timeout_secs: i32,
}

#[derive(Debug, Deserialize)]
pub struct JudgeHeartbeatRequest {
    pub worker_id: String,
    pub attempt: i32,
    pub lease_token: String,
}

#[derive(Debug, Deserialize)]
pub struct JudgeResultRequest {
    pub worker_id: String,
    pub attempt: i32,
    pub lease_token: String,
    pub result_id: String,
    pub outcome: String, // "up", "down", "target_timeout", "worker_error"
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: Option<i32>,
}
