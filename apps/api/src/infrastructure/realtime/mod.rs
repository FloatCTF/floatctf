//! 实时事件发布（WebSocket / SSE / 多节点 Redis 扇出）。
//!
//! Redis URL 来自必需配置 `[redis].url`；`[realtime].channel` 只配置事件频道。

pub mod publisher;

pub use publisher::{
    BroadcastEventPublisher, EventPublisher, HybridEventPublisher, NoopEventPublisher,
    RealtimeEvent, RecordingEventPublisher, build_realtime,
};
