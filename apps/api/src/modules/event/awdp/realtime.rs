//! AWDP 实时事件（plan §41）——经共享 EventPublisher 发布 `awdp.*` 事件。

use serde_json::json;
use uuid::Uuid;

use crate::bootstrap::AppState;
use crate::infrastructure::realtime::RealtimeEvent;

/// 发布一条 awdp 事件（state.publisher 为 Noop 时静默）。
pub fn publish(state: &AppState, event_id: Uuid, event_type: &str, payload: serde_json::Value) {
    let event = RealtimeEvent::new(event_id, event_type.to_string(), payload);
    let publisher = state.publisher.clone();
    actix_web::rt::spawn(async move {
        let _ = publisher.publish(event).await;
    });
}

/// score_changed（Break 得分 / Fix 得分）。
pub fn score_changed(state: &AppState, event_id: Uuid, score_type: &str, delta: i64) {
    publish(
        state,
        event_id,
        "awdp.score_changed",
        json!({ "score_type": score_type, "delta": delta }),
    );
}

/// phase_changed。
pub fn phase_changed(state: &AppState, event_id: Uuid, phase: &str) {
    publish(
        state,
        event_id,
        "awdp.phase_changed",
        json!({ "phase": phase }),
    );
}

/// patch_applied。
pub fn patch_applied(state: &AppState, event_id: Uuid, instance_id: Uuid, status: &str) {
    publish(
        state,
        event_id,
        "awdp.patch_applied",
        json!({ "instance_id": instance_id, "status": status }),
    );
}

/// manual_check_completed。
pub fn manual_check_completed(state: &AppState, event_id: Uuid, instance_id: Uuid, ok: bool) {
    publish(
        state,
        event_id,
        "awdp.manual_check_completed",
        json!({ "instance_id": instance_id, "ok": ok }),
    );
}
