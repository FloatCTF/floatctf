//! Real-time event publishing (WebSocket / SSE / multi-node Redis fan-out).
//!
//! Env (see `publisher::build_realtime_from_env`):
//! - `REALTIME_REDIS_URL` — enable Redis pub/sub multi-node fan-out (requires
//!   cargo feature `realtime-redis`)
//! - `REALTIME_REDIS_CHANNEL` — optional channel name (default `floatctf:realtime`)

pub mod publisher;

pub use publisher::{
    BroadcastEventPublisher, EventPublisher, HybridEventPublisher, NoopEventPublisher,
    RealtimeEvent, RecordingEventPublisher, build_realtime_from_env,
};
