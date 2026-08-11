//! AWDP API DTO。

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{
    awdp_event_gameboxes, awdp_events, gameboxes, sea_orm_active_enums::AwdpPhase,
};
use crate::modules::event::awdp::domain::AwdpConfig;
use crate::modules::event::awdp::service::runtime::InstanceView;

// ────────────────────────────────────────────────────────────────────────────
// Player
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AwdpGameBoxDto {
    pub id: Uuid,
    pub gamebox_id: Uuid,
    pub name: String,
    pub category: String,
    pub enabled: bool,
    pub hidden: bool,
    /// 声明暴露的端口（healthcheck 推导）：(protocol, port)
    pub exposed: Vec<(String, u16)>,
    /// 是否已 Break（一次性）。
    pub broken: bool,
    pub instance: Option<InstanceViewDto>,
    /// Fix 阶段才返回的源码目录说明。
    pub source_code_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstanceViewDto {
    pub instance_id: Uuid,
    pub runtime_state: String,
    pub runtime_generation: i64,
    pub endpoints: Vec<EndpointDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndpointDto {
    pub protocol: String,
    pub container_port: u16,
    pub public_host: String,
    pub public_port: u16,
}

impl From<&InstanceView> for InstanceViewDto {
    fn from(v: &InstanceView) -> Self {
        Self {
            instance_id: v.instance_id,
            runtime_state: v.runtime_state.clone(),
            runtime_generation: v.runtime_generation,
            endpoints: v
                .endpoints
                .iter()
                .map(|e| EndpointDto {
                    protocol: e.protocol.clone(),
                    container_port: e.container_port,
                    public_host: e.public_host.clone(),
                    public_port: e.public_port,
                })
                .collect(),
        }
    }
}

/// 选手侧概览：phase / timing / score / gameboxes。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpOverviewDto {
    pub event_id: Uuid,
    pub phase: AwdpPhase,
    pub break_duration_secs: i32,
    pub fix_duration_secs: i32,
    pub fix_round_interval_secs: i32,
    pub total_rounds: i32,
    pub break_score: i64,
    pub fix_round_score: i64,
    pub started_at: Option<DateTime<FixedOffset>>,
    pub break_ends_at: Option<DateTime<FixedOffset>>,
    pub fix_started_at: Option<DateTime<FixedOffset>>,
    pub fix_ends_at: Option<DateTime<FixedOffset>>,
    pub finished_at: Option<DateTime<FixedOffset>>,
    pub current_round: i32,
    pub next_action_at: Option<DateTime<FixedOffset>>,
    pub my_score: i64,
    pub gameboxes: Vec<AwdpGameBoxDto>,
}

#[derive(Debug, Deserialize)]
pub struct BreakSubmitRequest {
    pub flag: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BreakSubmitResponse {
    pub accepted: bool,
    pub scored: bool,
    pub already_broken: bool,
}

// ────────────────────────────────────────────────────────────────────────────
// Admin
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AwdpEventConfigDto {
    pub event_id: Uuid,
    pub phase: AwdpPhase,
    pub break_duration_secs: i32,
    pub fix_duration_secs: i32,
    pub fix_round_interval_secs: i32,
    pub break_score: i64,
    pub fix_round_score: i64,
    pub total_rounds: i32,
    pub configuration_generation: i64,
    pub updated_at: DateTime<FixedOffset>,
    pub started_at: Option<DateTime<FixedOffset>>,
    pub break_ends_at: Option<DateTime<FixedOffset>>,
    pub fix_started_at: Option<DateTime<FixedOffset>>,
    pub fix_ends_at: Option<DateTime<FixedOffset>>,
    pub finished_at: Option<DateTime<FixedOffset>>,
    pub current_round: i32,
    pub next_action_at: Option<DateTime<FixedOffset>>,
}

impl From<&awdp_events::Model> for AwdpEventConfigDto {
    fn from(a: &awdp_events::Model) -> Self {
        let config = AwdpConfig {
            break_duration_secs: a.break_duration_secs,
            fix_duration_secs: a.fix_duration_secs,
            fix_round_interval_secs: a.fix_round_interval_secs,
            break_score: a.break_score,
            fix_round_score: a.fix_round_score,
        };
        Self {
            event_id: a.event_id,
            phase: a.phase.clone(),
            break_duration_secs: a.break_duration_secs,
            fix_duration_secs: a.fix_duration_secs,
            fix_round_interval_secs: a.fix_round_interval_secs,
            break_score: a.break_score,
            fix_round_score: a.fix_round_score,
            total_rounds: config.total_rounds(),
            configuration_generation: a.configuration_generation,
            updated_at: a.updated_at,
            started_at: a.started_at,
            break_ends_at: a.break_ends_at,
            fix_started_at: a.fix_started_at,
            fix_ends_at: a.fix_ends_at,
            finished_at: a.finished_at,
            current_round: a.current_round,
            next_action_at: a.next_action_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AwdpConfigPatchRequest {
    pub expected_updated_at: DateTime<FixedOffset>,
    pub break_duration_secs: Option<i32>,
    pub fix_duration_secs: Option<i32>,
    pub fix_round_interval_secs: Option<i32>,
    pub break_score: Option<i64>,
    pub fix_round_score: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AttachGameBoxRequest {
    pub gamebox_id: Uuid,
    pub hidden: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AwdpAdminEventGameBoxDto {
    pub id: Uuid,
    pub event_id: Uuid,
    pub gamebox_id: Uuid,
    pub name: String,
    pub safe_name: String,
    pub category: String,
    pub enabled: bool,
    pub hidden: bool,
    pub cpu_millis: i64,
    pub memory_bytes: i64,
    pub pids_limit: i64,
    /// 完整 [awdp] capability（source.zip 产物存在）。
    pub awdp_capable: bool,
    pub awdp_source_code_dir: Option<String>,
    pub build_status: Option<String>,
}

impl AwdpAdminEventGameBoxDto {
    pub fn from_join(eg: &awdp_event_gameboxes::Model, gb: &gameboxes::Model) -> Self {
        Self {
            id: eg.id,
            event_id: eg.event_id,
            gamebox_id: eg.gamebox_id,
            name: gb.name.clone(),
            safe_name: gb.safe_name.clone(),
            category: gb.category.clone(),
            enabled: eg.enabled,
            hidden: eg.hidden,
            cpu_millis: eg.cpu_millis,
            memory_bytes: eg.memory_bytes,
            pids_limit: eg.pids_limit,
            awdp_capable: gb.awdp_source_artifact_key.is_some(),
            awdp_source_code_dir: gb.awdp_source_code_dir.clone(),
            build_status: gb.build_status.clone(),
        }
    }
}

/// 管理端实例视图。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpAdminInstanceDto {
    pub instance_id: Uuid,
    pub event_gamebox_id: Uuid,
    pub gamebox_name: String,
    pub owner_user_id: Option<Uuid>,
    pub owner_team_id: Option<Uuid>,
    pub runtime_state: String,
    pub runtime_generation: i64,
    pub container_name: String,
    pub endpoints: Vec<EndpointDto>,
}

impl From<&InstanceView> for AwdpAdminInstanceDto {
    fn from(v: &InstanceView) -> Self {
        Self {
            instance_id: v.instance_id,
            event_gamebox_id: v.event_gamebox_id,
            gamebox_name: v.gamebox_name.clone(),
            owner_user_id: None,
            owner_team_id: None,
            runtime_state: v.runtime_state.clone(),
            runtime_generation: v.runtime_generation,
            container_name: String::new(),
            endpoints: v
                .endpoints
                .iter()
                .map(|e| EndpointDto {
                    protocol: e.protocol.clone(),
                    container_port: e.container_port,
                    public_host: e.public_host.clone(),
                    public_port: e.public_port,
                })
                .collect(),
        }
    }
}

/// Patch 提交结果。
#[derive(Debug, Clone, Serialize)]
pub struct PatchSubmitResponse {
    pub status: String,
}

/// 手动 Test Check 结果。
#[derive(Debug, Clone, Serialize)]
pub struct ManualCheckDto {
    pub healthcheck_ok: bool,
    pub healthcheck_detail: Vec<String>,
    pub judge_ok: bool,
    pub judge_detail: String,
}

/// 回合时间线 DTO。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpRoundDto {
    pub id: Uuid,
    pub sequence: i32,
    pub starts_at: DateTime<FixedOffset>,
    pub cutoff_at: DateTime<FixedOffset>,
    pub status: String,
}

/// 我的评估历史 DTO。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpEvaluationDto {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub event_gamebox_id: Uuid,
    pub fix_round_id: Option<Uuid>,
    pub round_sequence: Option<i32>,
    pub kind: crate::entity::sea_orm_active_enums::AwdpEvaluationKind,
    pub status: crate::entity::sea_orm_active_enums::AwdpEvaluationStatus,
    pub healthcheck_result: Option<String>,
    pub judge_result: Option<String>,
    pub finished_at: Option<DateTime<FixedOffset>>,
}
