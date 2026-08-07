//! Real-time event publishing (WebSocket / SSE / multi-node Redis fan-out).
//!
//! Redis fan-out is configured through `[realtime]` in the API TOML file.

pub mod publisher;

pub use publisher::{
    BroadcastEventPublisher, EventPublisher, HybridEventPublisher, NoopEventPublisher,
    RealtimeEvent, RecordingEventPublisher, build_realtime,
};
