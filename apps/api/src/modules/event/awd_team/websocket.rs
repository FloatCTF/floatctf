//! WebSocket event publisher for AWD real-time updates.
//!
//! # Event Format
//!
//! ```json
//! {
//!   "event": "attack.success",
//!   "event_id": "uuid",
//!   "sequence": 42,
//!   "data": { ... },
//!   "timestamp": "2025-01-01T00:00:00Z"
//! }
//! ```
//!
//! # Security
//!
//! Events must NOT contain:
//! - Full flags
//! - WireGuard keys
//! - Event secrets
//! - Internal tokens

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::infrastructure::realtime::RealtimeEvent;

/// An AWD real-time event to be broadcast over WebSocket.
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

/// Publish an attack success event.
pub fn attack_success(
    event_id: Uuid,
    attacker_team_id: Uuid,
    victim_team_id: Uuid,
    template_id: Uuid,
    points: i64,
) -> AwdEvent {
    AwdEvent::new(
        "attack.success",
        event_id,
        serde_json::json!({
            "attacker_team_id": attacker_team_id,
            "victim_team_id": victim_team_id,
            "template_id": template_id,
            "points": points,
        }),
    )
}

/// Publish a score change event.
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

/// Publish a judge result event.
pub fn judge_result(
    event_id: Uuid,
    team_id: Uuid,
    template_id: Uuid,
    status: &str,
    duration_ms: Option<i32>,
) -> AwdEvent {
    AwdEvent::new(
        "judge.result",
        event_id,
        serde_json::json!({
            "team_id": team_id,
            "template_id": template_id,
            "status": status,
            "duration_ms": duration_ms,
        }),
    )
}

/// Publish a round lifecycle event.
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

/// Publish a GameBox reset event.
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

/// Publish a team banned event.
pub fn team_banned(event_id: Uuid, team_id: Uuid) -> AwdEvent {
    AwdEvent::new(
        "team.banned",
        event_id,
        serde_json::json!({
            "team_id": team_id,
        }),
    )
}

/// Publish a team unbanned event.
pub fn team_unbanned(event_id: Uuid, team_id: Uuid) -> AwdEvent {
    AwdEvent::new(
        "team.unbanned",
        event_id,
        serde_json::json!({
            "team_id": team_id,
        }),
    )
}

/// Publish an event lifecycle event.
pub fn event_paused(event_id: Uuid) -> AwdEvent {
    AwdEvent::new("event.paused", event_id, serde_json::json!({}))
}

pub fn event_resumed(event_id: Uuid) -> AwdEvent {
    AwdEvent::new("event.resumed", event_id, serde_json::json!({}))
}

/// Publish a network error event.
pub fn network_error(event_id: Uuid, error_msg: &str) -> AwdEvent {
    AwdEvent::new(
        "network.error",
        event_id,
        serde_json::json!({
            "error": error_msg,
        }),
    )
}
