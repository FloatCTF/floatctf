//! AWD 实时更新的 WebSocket 事件发布。
//!
//! 事件载荷不得包含密钥、完整规则集等敏感明细。

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::infrastructure::realtime::RealtimeEvent;

/// 经 WebSocket 广播的 AWD 实时事件。
#[derive(Debug, Clone, Serialize)]
pub struct AwdEvent {
    /// Event type (e.g. "attack.success", "score.changed", "judge.result")
    pub event: String,
    /// The AWD event ID
    pub event_id: Uuid,
    /// Sequence number for ordering
    pub sequence: Option<u64>,
    /// Event payload (must not contain secrets)
    pub data: serde_json::Value,
    /// ISO 8601 timestamp
    pub timestamp: String,
}

impl AwdEvent {
    pub fn new(event: &str, event_id: Uuid, data: serde_json::Value) -> Self {
        Self {
            event: event.to_string(),
            event_id,
            sequence: None,
            data,
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Convert to the platform-wide realtime envelope.
    pub fn into_realtime(self) -> RealtimeEvent {
        let mut ev = RealtimeEvent::new(self.event_id, self.event, self.data);
        ev.sequence = self.sequence;
        ev.occurred_at = self.timestamp;
        ev
    }
}

/// 发布攻击成功事件。
pub fn attack_success(
    event_id: Uuid,
    attacker_team_id: Uuid,
    victim_team_id: Uuid,
    event_gamebox_id: Uuid,
    points: i64,
) -> AwdEvent {
    AwdEvent::new(
        "attack.success",
        event_id,
        serde_json::json!({
            "attacker_team_id": attacker_team_id,
            "victim_team_id": victim_team_id,
            "event_gamebox_id": event_gamebox_id,
            "points": points,
        }),
    )
}

/// 发布比分变更事件。
pub fn score_changed(event_id: Uuid, team_id: Uuid, new_total: i64, delta: i64) -> AwdEvent {
    AwdEvent::new(
        "score.changed",
        event_id,
        serde_json::json!({
            "team_id": team_id,
            "new_total": new_total,
            "delta": delta,
        }),
    )
}

/// 发布裁判结果事件。
pub fn judge_result(
    event_id: Uuid,
    team_id: Uuid,
    event_gamebox_id: Uuid,
    status: &str,
    duration_ms: Option<i32>,
) -> AwdEvent {
    AwdEvent::new(
        "judge.result",
        event_id,
        serde_json::json!({
            "team_id": team_id,
            "event_gamebox_id": event_gamebox_id,
            "status": status,
            "duration_ms": duration_ms,
        }),
    )
}

/// 发布轮次生命周期事件。
pub fn round_started(event_id: Uuid, round_number: i32, phase: &str) -> AwdEvent {
    AwdEvent::new(
        "round.started",
        event_id,
        serde_json::json!({
            "round_number": round_number,
            "phase": phase,
        }),
    )
}

pub fn round_ended(event_id: Uuid, round_number: i32) -> AwdEvent {
    AwdEvent::new(
        "round.ended",
        event_id,
        serde_json::json!({
            "round_number": round_number,
        }),
    )
}

/// Publish a network policy applied event（P3-7：payload 只含 revision/phase，不含完整 ruleset）。
pub fn network_policy_applied(
    event_id: Uuid,
    desired_revision: u64,
    observed_revision: u64,
    phase: &str,
) -> AwdEvent {
    AwdEvent::new(
        "network.policy.applied",
        event_id,
        serde_json::json!({
            "desired_revision": desired_revision,
            "observed_revision": observed_revision,
            "phase": phase,
        }),
    )
}

/// 发布网络策略失败事件（P3-7）。
pub fn network_policy_failed(
    event_id: Uuid,
    desired_revision: u64,
    observed_revision: Option<u64>,
    phase: &str,
) -> AwdEvent {
    AwdEvent::new(
        "network.policy.failed",
        event_id,
        serde_json::json!({
            "desired_revision": desired_revision,
            "observed_revision": observed_revision,
            "phase": phase,
        }),
    )
}

/// 发布轮次完成事件（P3-7）。
pub fn round_completed(event_id: Uuid, round_number: i32) -> AwdEvent {
    AwdEvent::new(
        "round.completed",
        event_id,
        serde_json::json!({
            "round_number": round_number,
        }),
    )
}

/// 发布 GameBox 重置事件。
pub fn gamebox_reset(event_id: Uuid, instance_id: Uuid, team_id: Uuid, status: &str) -> AwdEvent {
    AwdEvent::new(
        "gamebox.reset",
        event_id,
        serde_json::json!({
            "instance_id": instance_id,
            "team_id": team_id,
            "status": status,
        }),
    )
}

/// 发布战队封禁事件。
pub fn team_banned(event_id: Uuid, team_id: Uuid) -> AwdEvent {
    AwdEvent::new(
        "team.banned",
        event_id,
        serde_json::json!({
            "team_id": team_id,
        }),
    )
}

/// 发布战队解封事件。
pub fn team_unbanned(event_id: Uuid, team_id: Uuid) -> AwdEvent {
    AwdEvent::new(
        "team.unbanned",
        event_id,
        serde_json::json!({
            "team_id": team_id,
        }),
    )
}

/// 发布赛事生命周期
pub fn event_paused(event_id: Uuid) -> AwdEvent {
    AwdEvent::new("event.paused", event_id, serde_json::json!({}))
}

pub fn event_resumed(event_id: Uuid) -> AwdEvent {
    AwdEvent::new("event.resumed", event_id, serde_json::json!({}))
}

/// 发布网络错误事件。
pub fn network_error(event_id: Uuid, error_msg: &str) -> AwdEvent {
    AwdEvent::new(
        "network.error",
        event_id,
        serde_json::json!({
            "error": error_msg,
        }),
    )
}
