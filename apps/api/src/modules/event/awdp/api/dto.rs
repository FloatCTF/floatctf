//! AWDP API DTO。

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{
    awdp_event_gameboxes, awdp_events, awdp_runs, gameboxes, sea_orm_active_enums::AwdpPhase,
};
use crate::modules::event::awdp::domain::AwdpConfig;
use crate::modules::event::awdp::service::runtime::InstanceView;

// ────────────────────────────────────────────────────────────────────────────
// Player（competition event 视图）
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
    /// 玩家手动 Reset 次数（比赛 subject×gamebox；练习恒 0）。
    pub reset_count: i64,
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
            reset_count: v.reset_count,
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

/// 选手侧概览：phase / timing / score / gameboxes（competition event 视图）。
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
// Admin（配置 + run 汇总）
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AwdpEventConfigDto {
    pub event_id: Uuid,
    /// 运行态阶段来自 active run（无 run = pending）。
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

impl AwdpEventConfigDto {
    pub fn from_config_and_run(a: &awdp_events::Model, run: Option<&awdp_runs::Model>) -> Self {
        let config = AwdpConfig {
            break_duration_secs: a.break_duration_secs,
            fix_duration_secs: a.fix_duration_secs,
            fix_round_interval_secs: a.fix_round_interval_secs,
            break_score: a.break_score,
            fix_round_score: a.fix_round_score,
        };
        Self {
            event_id: a.event_id,
            phase: run.map(|r| r.phase.clone()).unwrap_or(AwdpPhase::Pending),
            break_duration_secs: a.break_duration_secs,
            fix_duration_secs: a.fix_duration_secs,
            fix_round_interval_secs: a.fix_round_interval_secs,
            break_score: a.break_score,
            fix_round_score: a.fix_round_score,
            total_rounds: config.total_rounds(),
            configuration_generation: a.configuration_generation,
            updated_at: a.updated_at,
            started_at: run.and_then(|r| r.started_at),
            break_ends_at: run.and_then(|r| r.break_ends_at),
            fix_started_at: run.and_then(|r| r.fix_started_at),
            fix_ends_at: run.and_then(|r| r.fix_ends_at),
            finished_at: run.and_then(|r| r.finished_at),
            current_round: run.map(|r| r.current_round).unwrap_or(0),
            next_action_at: run.and_then(|r| r.next_action_at),
        }
    }
}

/// 管理端 run 汇总（事件 run 历史 inspect）。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpAdminRunDto {
    pub run_id: Uuid,
    pub phase: AwdpPhase,
    pub current_round: i32,
    pub total_rounds: i32,
    pub started_at: Option<DateTime<FixedOffset>>,
    pub finished_at: Option<DateTime<FixedOffset>>,
    pub next_action_at: Option<DateTime<FixedOffset>>,
    pub instance_count: usize,
    pub score_sum: i64,
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
    /// 完整 [awdp] capability（source.tar.gz 产物存在）。
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
    /// 玩家手动 Reset 次数（管理端 inspect 用）。
    pub reset_count: i64,
    pub container_name: String,
    pub endpoints: Vec<EndpointDto>,
}

impl From<&InstanceView> for AwdpAdminInstanceDto {
    fn from(v: &InstanceView) -> Self {
        Self {
            instance_id: v.instance_id,
            event_gamebox_id: v.gamebox_id,
            gamebox_name: v.gamebox_name.clone(),
            owner_user_id: None,
            owner_team_id: None,
            runtime_state: v.runtime_state.clone(),
            runtime_generation: v.runtime_generation,
            reset_count: v.reset_count,
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

// ────────────────────────────────────────────────────────────────────────────
// Training Ground（/api/service，run 维度）
// ────────────────────────────────────────────────────────────────────────────

/// 安全目录条目（plan §56：禁止 exploit/source key/source_code_dir/credentials）。
#[derive(Debug, Clone, Serialize)]
pub struct GameBoxCatalogDto {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub category: String,
    pub version: Option<String>,
    /// 作者（gameboxes.username，zip 包制作人）。
    pub author: Option<String>,
    pub updated_at: DateTime<FixedOffset>,
    /// == gameboxes.awdp_source_artifact_key 非空（五列全有，DB CHECK 保证）。
    pub awdp_capable: bool,
    pub recommended_cpu_millis: i64,
    pub recommended_memory_bytes: i64,
    pub recommended_pids_limit: i64,
    /// 当前 user 的 active practice run（若有）。
    pub active_training: Option<ActiveTrainingDto>,
    /// 当前请求用户是否训练过该 GameBox（该用户对该 gamebox 的练习 run 至少启动过一次实例）。
    pub solved: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveTrainingDto {
    pub run_id: Uuid,
    pub phase: AwdpPhase,
    pub score: i64,
}

/// Practice run 统一 view-model（前端 AwdpWorkbench 数据源）。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpRunDto {
    pub run_id: Uuid,
    pub gamebox_id: Uuid,
    pub gamebox_name: String,
    pub gamebox_category: String,
    pub gamebox_description: String,
    pub event_id: Option<Uuid>,
    pub phase: AwdpPhase,
    pub break_duration_secs: i32,
    pub fix_duration_secs: i32,
    pub fix_round_interval_secs: i32,
    pub break_score: i64,
    pub fix_round_score: i64,
    pub total_rounds: i32,
    pub started_at: Option<DateTime<FixedOffset>>,
    pub break_ends_at: Option<DateTime<FixedOffset>>,
    pub fix_started_at: Option<DateTime<FixedOffset>>,
    pub fix_ends_at: Option<DateTime<FixedOffset>>,
    pub finished_at: Option<DateTime<FixedOffset>>,
    pub current_round: i32,
    pub next_action_at: Option<DateTime<FixedOffset>>,
    pub my_score: i64,
    /// 练习模式每轮 check 失败扣分（前端 History 展示用；竞赛 run 无此概念）。
    pub fix_round_penalty: i64,
    /// Fix 阶段才返回的源码目录说明。
    pub source_code_dir: Option<String>,
    pub instances: Vec<RunInstanceDto>,
    /// 练习 data plane Flag Server endpoint（仅 GameBox 内部网络可达；Fix 阶段弱化）。
    pub judge_endpoint: Option<JudgeEndpointDto>,
}

/// JudgeServer data plane 玩家可见端点（plan §50/§51）。
#[derive(Debug, Clone, Serialize)]
pub struct JudgeEndpointDto {
    /// data plane 基址（GameBox 内部网络 DNS alias）。
    pub base_url: String,
    /// Break flag 端点（GET）。
    pub flag_url: String,
    /// 可达范围（固定 "gamebox_internal"）。
    pub scope: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunInstanceDto {
    pub instance_id: Uuid,
    pub gamebox_id: Uuid,
    pub runtime_state: String,
    pub runtime_generation: i64,
    pub reset_count: i64,
    pub broken: bool,
    pub endpoints: Vec<EndpointDto>,
}

impl From<&InstanceView> for RunInstanceDto {
    fn from(v: &InstanceView) -> Self {
        Self {
            instance_id: v.instance_id,
            gamebox_id: v.gamebox_id,
            runtime_state: v.runtime_state.clone(),
            runtime_generation: v.runtime_generation,
            reset_count: v.reset_count,
            broken: v.broken,
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

/// 我的得分视图（总分 + 明细历史）。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpRunScoresDto {
    pub total: i64,
    pub history: Vec<AwdpScoreEventDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AwdpScoreEventDto {
    pub id: Uuid,
    pub gamebox_id: Uuid,
    pub score_type: String,
    pub fix_round_id: Option<Uuid>,
    pub delta: i64,
    pub created_at: DateTime<FixedOffset>,
}

/// Patch 提交结果。
#[derive(Debug, Clone, Serialize)]
pub struct PatchSubmitResponse {
    pub status: String,
    /// 失败原因（status = failed 时给出；applied 时为 None）。
    pub error_message: Option<String>,
}

/// 手动 Test Check 结果。
#[derive(Debug, Clone, Serialize)]
/// 手动 Test Check 结果 DTO（异步：POST 后立即返回 pending，前端轮询 evaluations 终态）。
pub struct ManualCheckDto {
    /// 创建的 manual 评估 id（前端据 id 从 evaluations 列表取终态详情）。
    pub evaluation_id: Uuid,
    /// 初始状态（"pending"）；终态时前端用 evaluations 行的 healthcheck/judge 结果。
    pub status: String,
    pub healthcheck_ok: Option<bool>,
    pub healthcheck_detail: Option<Vec<String>>,
    pub judge_ok: Option<bool>,
    pub judge_detail: Option<String>,
    /// 练习模式才执行 exploit 诊断（不计分）；非练习恒 None。
    pub exploit_ok: Option<bool>,
    pub exploit_detail: Option<String>,
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
    pub gamebox_id: Uuid,
    pub fix_round_id: Option<Uuid>,
    pub round_sequence: Option<i32>,
    pub kind: crate::entity::sea_orm_active_enums::AwdpEvaluationKind,
    pub status: crate::entity::sea_orm_active_enums::AwdpEvaluationStatus,
    pub healthcheck_result: Option<String>,
    pub judge_result: Option<String>,
    /// exploit 结果详情（练习 manual / official 终态才有；其余 None）。
    pub exploit_result: Option<String>,
    pub finished_at: Option<DateTime<FixedOffset>>,
}

/// ALL Check 结果（练习模式；status=patched → 剩余回合全部计分 + run 已结束）。
#[derive(Debug, Clone, Serialize)]
pub struct AllCheckDto {
    /// 终态：patched=修复成功（swept=true）；其余 = 本次未通过（不落账，等官方 check）。
    pub status: crate::entity::sea_orm_active_enums::AwdpEvaluationStatus,
    /// status=patched：剩余回合已全部计分且 run 已结束。
    pub swept: bool,
    pub swept_rounds: i32,
    pub target_round: i32,
    pub healthcheck_detail: Option<String>,
    pub judge_detail: Option<String>,
    pub exploit_detail: Option<String>,
}

/// 我的 Writeup DTO（练习 run 属主可读写，一 run 一份）。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpRunWriteupDto {
    pub run_id: Uuid,
    pub content: String,
    pub updated_at: Option<DateTime<FixedOffset>>,
}
