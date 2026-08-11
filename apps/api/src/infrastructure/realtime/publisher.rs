//! `EventPublisher` trait 以及进程内 / 多节点实现。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// 平台级实时事件信封。
///
/// 不得包含完整 flag、WireGuard 密钥或内部令牌。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeEvent {
    pub event_id: Uuid,
    pub sequence: Option<u64>,
    /// Event type, e.g. `attack.success`, `score.changed`.
    #[serde(rename = "type")]
    pub event_type: String,
    pub occurred_at: String,
    pub payload: Value,
}

impl RealtimeEvent {
    pub fn new(event_id: Uuid, event_type: impl Into<String>, payload: Value) -> Self {
        Self {
            event_id,
            sequence: None,
            event_type: event_type.into(),
            occurred_at: Utc::now().to_rfc3339(),
            payload,
        }
    }

    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = Some(sequence);
        self
    }
}

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: RealtimeEvent) -> anyhow::Result<()>;
}

/// 丢弃全部事件（在接入 WS hub 前的默认实现）。
pub struct NoopEventPublisher;

#[async_trait]
impl EventPublisher for NoopEventPublisher {
    async fn publish(&self, _event: RealtimeEvent) -> anyhow::Result<()> {
        Ok(())
    }
}

/// 记录已发布事件，供测试使用。
#[derive(Default, Clone)]
pub struct RecordingEventPublisher {
    events: Arc<Mutex<Vec<RealtimeEvent>>>,
}

impl RecordingEventPublisher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<RealtimeEvent> {
        self.events.lock().expect("lock").clone()
    }

    pub fn clear(&self) {
        self.events.lock().expect("lock").clear();
    }
}

#[async_trait]
impl EventPublisher for RecordingEventPublisher {
    async fn publish(&self, event: RealtimeEvent) -> anyhow::Result<()> {
        self.events.lock().expect("lock").push(event);
        Ok(())
    }
}

/// 进程内广播中枢，供 WebSocket / SSE 订阅方使用。
///
/// 订阅者收到已发布事件的克隆。落后接收方会丢弃较旧
/// 消息（广播语义）。在需要多节点总线前适用。
pub struct BroadcastEventPublisher {
    tx: tokio::sync::broadcast::Sender<RealtimeEvent>,
    seq: std::sync::atomic::AtomicU64,
}

impl BroadcastEventPublisher {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(capacity.max(16));
        Self {
            tx,
            seq: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<RealtimeEvent> {
        self.tx.subscribe()
    }

    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }

    fn next_sequence(&self) -> u64 {
        self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Send without allocating a new sequence (caller must set sequence if needed).
    fn raw_send(
        &self,
        event: RealtimeEvent,
    ) -> Result<usize, tokio::sync::broadcast::error::SendError<RealtimeEvent>> {
        self.tx.send(event)
    }

    /// Inject an event into the local hub without assigning a new sequence when
    /// one is already present (used by Redis fan-in).
    pub fn inject_local(&self, mut event: RealtimeEvent) {
        if event.sequence.is_none() {
            event = event.with_sequence(self.next_sequence());
        }
        let _ = self.raw_send(event);
    }
}

#[async_trait]
impl EventPublisher for BroadcastEventPublisher {
    async fn publish(&self, mut event: RealtimeEvent) -> anyhow::Result<()> {
        if event.sequence.is_none() {
            let n = self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            event = event.with_sequence(n);
        }
        // Ignore "no receivers" — publish is fire-and-forget for realtime.
        let _ = self.tx.send(event);
        Ok(())
    }
}

/// Redis pub/sub 线路格式（含 origin，便于节点丢弃回环）。
#[cfg(feature = "realtime-redis")]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedisBusMessage {
    origin: Uuid,
    event: RealtimeEvent,
}

/// 本地广播 + 可选 Redis PUBLISH，用于多节点扇出。
///
/// SSE 仍订阅共享的 [`BroadcastEventPublisher`]。远端
/// 事件经后台 Redis 订阅到达并注入本地。
pub struct HybridEventPublisher {
    local: Arc<BroadcastEventPublisher>,
    node_id: Uuid,
    #[cfg(feature = "realtime-redis")]
    redis: Option<HybridRedis>,
}

#[cfg(feature = "realtime-redis")]
struct HybridRedis {
    client: redis::Client,
    channel: String,
}

impl HybridEventPublisher {
    /// Build a hybrid publisher. When `redis_url` is `None`, behaves like local-only
    /// (still usable as `EventPublisher` wrapping the same hub).
    pub fn new(local: Arc<BroadcastEventPublisher>, redis_url: Option<&str>) -> Self {
        Self::new_with_channel(local, redis_url, None)
    }

    pub fn new_with_channel(
        local: Arc<BroadcastEventPublisher>,
        redis_url: Option<&str>,
        channel: Option<&str>,
    ) -> Self {
        let node_id = Uuid::new_v4();
        let channel = channel
            .map(str::to_string)
            .unwrap_or_else(|| "floatctf:realtime".to_string());

        #[cfg(feature = "realtime-redis")]
        {
            let redis = match redis_url {
                Some(url) if !url.is_empty() => match redis::Client::open(url) {
                    Ok(client) => {
                        tracing::info!(
                            channel = %channel,
                            node_id = %node_id,
                            "realtime Redis fan-out enabled"
                        );
                        Some(HybridRedis { client, channel })
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "invalid REALTIME_REDIS_URL; local-only publish");
                        None
                    }
                },
                _ => None,
            };

            if let Some(ref r) = redis {
                Self::spawn_subscriber(local.clone(), r.client.clone(), r.channel.clone(), node_id);
            }

            return Self {
                local,
                node_id,
                redis,
            };
        }

        #[cfg(not(feature = "realtime-redis"))]
        {
            if redis_url.map(|u| !u.is_empty()).unwrap_or(false) {
                tracing::warn!(
                    "REALTIME_REDIS_URL is set but binary built without feature `realtime-redis`; \
                     using in-process BroadcastEventPublisher only"
                );
            }
            let _ = channel;
            Self { local, node_id }
        }
    }

    pub fn local_hub(&self) -> &Arc<BroadcastEventPublisher> {
        &self.local
    }

    pub fn node_id(&self) -> Uuid {
        self.node_id
    }

    #[cfg(feature = "realtime-redis")]
    fn spawn_subscriber(
        local: Arc<BroadcastEventPublisher>,
        client: redis::Client,
        channel: String,
        node_id: Uuid,
    ) {
        tokio::spawn(async move {
            loop {
                match run_subscriber_loop(&local, &client, &channel, node_id).await {
                    Ok(()) => {
                        tracing::warn!("realtime Redis subscriber ended; reconnecting in 2s");
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "realtime Redis subscriber error; reconnecting in 2s"
                        );
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }
}

#[cfg(feature = "realtime-redis")]
async fn run_subscriber_loop(
    local: &BroadcastEventPublisher,
    client: &redis::Client,
    channel: &str,
    node_id: Uuid,
) -> anyhow::Result<()> {
    use futures_util::StreamExt;

    let mut pubsub = client.get_async_pubsub().await?;
    pubsub.subscribe(channel).await?;
    let mut stream = pubsub.on_message();

    while let Some(msg) = stream.next().await {
        let payload: String = match msg.get_payload() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "redis message payload decode failed");
                continue;
            }
        };
        let bus: RedisBusMessage = match serde_json::from_str(&payload) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = %e, "redis realtime envelope parse failed");
                continue;
            }
        };
        if bus.origin == node_id {
            continue; // echo of our own publish
        }
        local.inject_local(bus.event);
    }
    Ok(())
}

#[async_trait]
impl EventPublisher for HybridEventPublisher {
    async fn publish(&self, event: RealtimeEvent) -> anyhow::Result<()> {
        // Assign sequence + fan out locally first (same-node SSE, no Redis RTT).
        // Clone after local publish so the sequenced event is what we ship to Redis.
        let sequenced = {
            let mut e = event;
            // Reuse BroadcastEventPublisher sequencing via inject path after
            // temporarily publishing through local EventPublisher impl.
            // We call local.publish which mutates sequence, but we need the
            // sequenced value for Redis — so sequence here then inject.
            if e.sequence.is_none() {
                let n = self.local.next_sequence();
                e = e.with_sequence(n);
            }
            let _ = self.local.raw_send(e.clone());
            e
        };

        #[cfg(feature = "realtime-redis")]
        if let Some(ref r) = self.redis {
            let bus = RedisBusMessage {
                origin: self.node_id,
                event: sequenced,
            };
            let payload = serde_json::to_string(&bus)?;
            match r.client.get_multiplexed_async_connection().await {
                Ok(mut conn) => {
                    let res: Result<i64, _> = redis::cmd("PUBLISH")
                        .arg(&r.channel)
                        .arg(payload)
                        .query_async(&mut conn)
                        .await;
                    if let Err(e) = res {
                        tracing::warn!(error = %e, "realtime Redis PUBLISH failed");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "realtime Redis connection failed on publish");
                }
            }
        }

        #[cfg(not(feature = "realtime-redis"))]
        {
            let _ = sequenced;
        }

        Ok(())
    }
}

/// 解析publisher wiring from the static TOML configuration。
pub fn build_realtime(
    capacity: usize,
    redis_url: Option<&str>,
    channel: Option<&str>,
) -> (Arc<BroadcastEventPublisher>, Arc<dyn EventPublisher>) {
    let hub = Arc::new(BroadcastEventPublisher::new(capacity));

    let has_redis = redis_url
        .as_deref()
        .map(|u| !u.trim().is_empty())
        .unwrap_or(false);

    if has_redis {
        let hybrid = HybridEventPublisher::new_with_channel(
            hub.clone(),
            redis_url.as_deref(),
            channel.as_deref(),
        );
        let publisher: Arc<dyn EventPublisher> = Arc::new(hybrid);
        (hub, publisher)
    } else {
        let publisher: Arc<dyn EventPublisher> = hub.clone();
        (hub, publisher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn recording_publisher_keeps_events() {
        let pub_ = RecordingEventPublisher::new();
        let id = Uuid::nil();
        pub_.publish(RealtimeEvent::new(id, "score.changed", json!({"delta": 1})))
            .await
            .unwrap();
        let snap = pub_.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].event_type, "score.changed");
        assert!(snap[0].payload.get("delta").is_some());
    }

    #[tokio::test]
    async fn noop_publisher_succeeds() {
        NoopEventPublisher
            .publish(RealtimeEvent::new(Uuid::nil(), "test", json!({})))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn broadcast_publisher_delivers_to_subscriber() {
        let hub = BroadcastEventPublisher::new(8);
        let mut rx = hub.subscribe();
        hub.publish(RealtimeEvent::new(
            Uuid::nil(),
            "round.started",
            json!({"n": 1}),
        ))
        .await
        .unwrap();
        let got = rx.recv().await.unwrap();
        assert_eq!(got.event_type, "round.started");
        assert!(got.sequence.is_some());
    }

    #[tokio::test]
    async fn hybrid_local_path_without_redis() {
        let hub = Arc::new(BroadcastEventPublisher::new(8));
        let mut rx = hub.subscribe();
        let hybrid = HybridEventPublisher::new(hub.clone(), None);
        hybrid
            .publish(RealtimeEvent::new(
                Uuid::nil(),
                "attack.success",
                json!({"points": 10}),
            ))
            .await
            .unwrap();
        let got = rx.recv().await.unwrap();
        assert_eq!(got.event_type, "attack.success");
        assert!(got.sequence.is_some());
    }

    #[tokio::test]
    async fn hybrid_inject_local_for_remote_fan_in() {
        let hub = Arc::new(BroadcastEventPublisher::new(8));
        let mut rx = hub.subscribe();
        let hybrid = HybridEventPublisher::new(hub.clone(), None);
        // Simulate remote node injecting via Redis path.
        hybrid
            .local
            .inject_local(RealtimeEvent::new(Uuid::nil(), "score.changed", json!({})));
        let got = rx.recv().await.unwrap();
        assert_eq!(got.event_type, "score.changed");
    }

    #[tokio::test]
    async fn build_realtime_local_default() {
        let (hub, publisher) = build_realtime(16, None, None);
        let mut rx = hub.subscribe();
        publisher
            .publish(RealtimeEvent::new(Uuid::nil(), "ping", json!({})))
            .await
            .unwrap();
        let got = rx.recv().await.unwrap();
        assert_eq!(got.event_type, "ping");
    }
}
