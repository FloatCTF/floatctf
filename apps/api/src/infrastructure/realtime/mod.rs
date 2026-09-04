//! 实时事件发布（WebSocket / SSE / 多节点 Redis 扇出）。
//!
//! Redis 扇出通过 API TOML 的 `[realtime]` 配置。

pub mod publisher;

pub use publisher::{
    BroadcastEventPublisher, EventPublisher, HybridEventPublisher, NoopEventPublisher,
    RealtimeEvent, RecordingEventPublisher, build_realtime,
};
