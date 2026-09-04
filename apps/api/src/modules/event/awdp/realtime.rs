//! AWDP 实时事件（plan §41）——经共享 EventPublisher 发布 `awdp.*` 事件。
//!
//! 维度：competition 按 event_id 订阅；practice run 按 run_id 订阅
//! （run-scoped 事件同时携带 event_id=Uuid::nil() 占位 + run_id 字段）。

use serde_json::json;
use uuid::Uuid;

use crate::bootstrap::AppState;
use crate::infrastructure::realtime::RealtimeEvent;

/// 发布一条 awdp 事件（state.publisher 为 Noop 时静默）。
/// event-scoped：competition SSE 按 event_id 过滤。
pub fn publish(state: &AppState, event_id: Uuid, event_type: &str, payload: serde_json::Value) {
    let event = RealtimeEvent::new(event_id, event_type.to_string(), payload);
    let publisher = state.publisher.clone();
    actix_web::rt::spawn(async move {
        let _ = publisher.publish(event).await;
    });
}

/// 发布一条 run-scoped awdp 事件（practice SSE 按 run_id 过滤）。
pub fn publish_run(state: &AppState, run_id: Uuid, event_type: &str, payload: serde_json::Value) {
    let event =
        RealtimeEvent::new(Uuid::nil(), event_type.to_string(), payload).with_run_id(run_id);
    let publisher = state.publisher.clone();
    actix_web::rt::spawn(async move {
        let _ = publisher.publish(event).await;
    });
}

/// score_changed（Break 得分 / Fix 得分；event 维度）。
pub fn score_changed(state: &AppState, event_id: Uuid, score_type: &str, delta: i64) {
    publish(
        state,
        event_id,
        "awdp.score_changed",
        json!({ "score_type": score_type, "delta": delta }),
    );
}

/// run-scoped score_changed（practice）。
pub fn run_score_changed(state: &AppState, run_id: Uuid, score_type: &str, delta: i64) {
    publish_run(
        state,
        run_id,
        "awdp.score_changed",
        json!({ "score_type": score_type, "delta": delta }),
    );
}

/// phase_changed（event 维度）。
pub fn phase_changed(state: &AppState, event_id: Uuid, phase: &str) {
    publish(
        state,
        event_id,
        "awdp.phase_changed",
        json!({ "phase": phase }),
    );
}

/// run-scoped phase_changed（practice）。
pub fn run_phase_changed(state: &AppState, run_id: Uuid, phase: &str) {
    publish_run(
        state,
        run_id,
        "awdp.phase_changed",
        json!({ "phase": phase }),
    );
}

/// patch_applied（event 维度）。
pub fn patch_applied(state: &AppState, event_id: Uuid, instance_id: Uuid, status: &str) {
    publish(
        state,
        event_id,
        "awdp.patch_applied",
        json!({ "instance_id": instance_id, "status": status }),
    );
}

/// run-scoped patch_applied（practice）。
pub fn run_patch_applied(state: &AppState, run_id: Uuid, instance_id: Uuid, status: &str) {
    publish_run(
        state,
        run_id,
        "awdp.patch_applied",
        json!({ "instance_id": instance_id, "status": status }),
    );
}

/// manual_check_completed（event 维度）。
pub fn manual_check_completed(state: &AppState, event_id: Uuid, instance_id: Uuid, ok: bool) {
    publish(
        state,
        event_id,
        "awdp.manual_check_completed",
        json!({ "instance_id": instance_id, "ok": ok }),
    );
}

/// run-scoped manual_check_completed（practice）。
pub fn run_manual_check_completed(state: &AppState, run_id: Uuid, instance_id: Uuid, ok: bool) {
    publish_run(
        state,
        run_id,
        "awdp.manual_check_completed",
        json!({ "instance_id": instance_id, "ok": ok }),
    );
}
