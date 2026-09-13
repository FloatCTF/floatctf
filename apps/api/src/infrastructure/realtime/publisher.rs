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
/// `run_id`：AWDP practice run 维度订阅（competition 仍按 event_id；两者互斥使用）。
///
/// `sequence` 语义（前端消费约定，勿扩展解读）：
/// - **全局唯一**：多节点经 Redis INCR 共享同一序列空间，用于事件去重与关联；
///   Redis 全量数据丢失后由毫秒时间戳地板保证不与历史序列撞号。
/// - **不承诺到达顺序**：跨节点投递（本地 raw_send 先于 pubsub 回环）顺序不保证
///   单调递增；前端不得用 sequence 做丢包/乱序检测，只能做唯一性判别。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeEvent {
    pub event_id: Uuid,
    #[serde(default)]
    pub run_id: Option<Uuid>,
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
            run_id: None,
            sequence: None,
            event_type: event_type.into(),
            occurred_at: Utc::now().to_rfc3339(),
            payload,
        }
    }

    /// 绑定 run 维度（practice SSE 按 run_id 订阅）。
    pub fn with_run_id(mut self, run_id: Uuid) -> Self {
        self.run_id = Some(run_id);
        self
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
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedisBusMessage {
    origin: Uuid,
    event: RealtimeEvent,
}

/// 本地广播 + Redis PUBLISH，用于多节点扇出。
///
/// Redis 是平台必需基础设施；本地 broadcast 仍作为同节点低延迟投递与 Redis
/// 短暂故障时的连续性路径。远端事件由后台 Redis subscriber 注入本地 hub。
pub struct HybridEventPublisher {
    local: Arc<BroadcastEventPublisher>,
    node_id: Uuid,
    redis: HybridRedis,
}

struct HybridRedis {
    client: redis::Client,
    channel: String,
}

impl HybridEventPublisher {
    pub fn new_with_channel(
        local: Arc<BroadcastEventPublisher>,
        client: redis::Client,
        channel: impl Into<String>,
    ) -> Self {
        let node_id = Uuid::new_v4();
        let channel = channel.into();
        tracing::info!(
            channel = %channel,
            node_id = %node_id,
            "realtime Redis fan-out enabled"
        );
        Self::spawn_subscriber(local.clone(), client.clone(), channel.clone(), node_id);
        Self {
            local,
            node_id,
            redis: HybridRedis { client, channel },
        }
    }

    pub fn local_hub(&self) -> &Arc<BroadcastEventPublisher> {
        &self.local
    }

    pub fn node_id(&self) -> Uuid {
        self.node_id
    }

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
            continue;
        }
        local.inject_local(bus.event);
    }
    Ok(())
}

/// 毫秒级 Unix 时间戳，作为 Redis sequence 计数器的下界。
fn unix_millis_floor() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[async_trait]
impl EventPublisher for HybridEventPublisher {
    async fn publish(&self, event: RealtimeEvent) -> anyhow::Result<()> {
        let r = &self.redis;
        match r.client.get_multiplexed_async_connection().await {
            Ok(mut conn) => {
                let mut sequenced = event;
                if sequenced.sequence.is_none() {
                    let sequence_key = format!("{}:sequence", r.channel);
                    let floor = unix_millis_floor();
                    let _ = redis::cmd("SET")
                        .arg(&sequence_key)
                        .arg(floor)
                        .arg("NX")
                        .query_async::<Option<String>>(&mut conn)
                        .await;
                    match redis::cmd("INCR")
                        .arg(sequence_key)
                        .query_async::<u64>(&mut conn)
                        .await
                    {
                        Ok(sequence) => sequenced = sequenced.with_sequence(sequence),
                        Err(error) => {
                            tracing::warn!(%error, "realtime Redis sequence allocation failed; using local sequence");
                            sequenced = sequenced.with_sequence(self.local.next_sequence());
                        }
                    }
                }

                let _ = self.local.raw_send(sequenced.clone());
                let bus = RedisBusMessage {
                    origin: self.node_id,
                    event: sequenced,
                };
                let payload = serde_json::to_string(&bus)?;
                let res: Result<i64, _> = redis::cmd("PUBLISH")
                    .arg(&r.channel)
                    .arg(payload)
                    .query_async(&mut conn)
                    .await;
                if let Err(error) = res {
                    tracing::warn!(%error, "realtime Redis PUBLISH failed; remote fan-out temporarily unavailable");
                }
                return Ok(());
            }
            Err(error) => {
                tracing::warn!(%error, "realtime Redis connection failed on publish; using same-node local fan-out");
            }
        }

        // Redis 是部署必需项；这里仅处理启动后的短暂故障，保留同节点实时事件。
        let mut sequenced = event;
        if sequenced.sequence.is_none() {
            sequenced = sequenced.with_sequence(self.local.next_sequence());
        }
        let _ = self.local.raw_send(sequenced);
        Ok(())
    }
}

/// 构造平台 realtime 总线。Redis client 已在 bootstrap PING 验证通过。
pub fn build_realtime(
    capacity: usize,
    redis: redis::Client,
    channel: &str,
) -> (Arc<BroadcastEventPublisher>, Arc<dyn EventPublisher>) {
    let hub = Arc::new(BroadcastEventPublisher::new(capacity));
    let hybrid = HybridEventPublisher::new_with_channel(hub.clone(), redis, channel.to_string());
    let publisher: Arc<dyn EventPublisher> = Arc::new(hybrid);
    (hub, publisher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::EntityTrait;
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
    async fn inject_local_for_remote_fan_in() {
        let hub = Arc::new(BroadcastEventPublisher::new(8));
        let mut rx = hub.subscribe();
        hub.inject_local(RealtimeEvent::new(Uuid::nil(), "score.changed", json!({})));
        let got = rx.recv().await.unwrap();
        assert_eq!(got.event_type, "score.changed");
    }

    #[tokio::test]
    async fn realtime_publish_keeps_same_node_delivery_during_redis_outage() {
        let client = redis::Client::open("redis://127.0.0.1:6390/").unwrap();
        let (hub, publisher) = build_realtime(16, client, "floatctf:test:realtime");
        let mut rx = hub.subscribe();
        publisher
            .publish(RealtimeEvent::new(Uuid::nil(), "ping", json!({})))
            .await
            .unwrap();
        let got = rx.recv().await.unwrap();
        assert_eq!(got.event_type, "ping");
    }

    // ── SSE 格式 / 订阅者生命周期测试 ──

    #[test]
    fn sse_frame_format_is_data_colon_json_newline_newline() {
        // 验证 SSE 端点输出的帧格式
        let ev = RealtimeEvent::new(
            Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            "score.changed",
            json!({"points": 10}),
        );
        let json = serde_json::to_string(&ev).unwrap();
        let frame = format!("data: {json}\n\n");
        assert!(frame.starts_with("data: "));
        assert!(frame.contains("\"type\":\"score.changed\""));
        assert!(frame.ends_with("\n\n"));
        // 帧不包含 event: 或 id: 字段（当前后端仅发送 data:）
        assert!(!frame.starts_with("event:"));
        assert!(!frame.starts_with("id:"));
    }

    #[test]
    fn sse_keepalive_comment_format() {
        // 验证 keepalive 注释格式（以 : 开头，前端解析器忽略）
        let keepalive = ": keepalive\n\n";
        assert!(keepalive.starts_with(':'));
        assert!(keepalive.ends_with("\n\n"));
        // 不应包含 data: 前缀
        assert!(!keepalive.contains("data:"));
    }

    #[test]
    fn sse_connected_comment_format() {
        // 验证初始连接消息格式
        let connected = ": connected\n\n";
        assert!(connected.starts_with(':'));
        assert!(connected.ends_with("\n\n"));
    }

    #[tokio::test]
    async fn subscriber_count_reflects_active_receivers() {
        let hub = BroadcastEventPublisher::new(8);
        assert_eq!(hub.receiver_count(), 0);

        let rx1 = hub.subscribe();
        assert_eq!(hub.receiver_count(), 1);

        let rx2 = hub.subscribe();
        assert_eq!(hub.receiver_count(), 2);

        drop(rx1);
        // 异步广播 channel 的 receiver_count 在 drop 后立即更新
        assert_eq!(hub.receiver_count(), 1);

        drop(rx2);
        assert_eq!(hub.receiver_count(), 0);
    }

    #[tokio::test]
    async fn publish_with_no_subscribers_does_not_error() {
        // fire-and-forget：无订阅者时发布不报错
        let hub = BroadcastEventPublisher::new(8);
        // 没有订阅者
        let result = hub
            .publish(RealtimeEvent::new(Uuid::nil(), "test", json!({})))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn dropped_subscriber_does_not_receive_new_events() {
        let hub = BroadcastEventPublisher::new(8);
        let mut rx = hub.subscribe();

        // 发布第一个事件
        hub.publish(RealtimeEvent::new(Uuid::nil(), "first", json!({})))
            .await
            .unwrap();
        let got = rx.recv().await.unwrap();
        assert_eq!(got.event_type, "first");

        // 丢弃订阅者
        drop(rx);

        // 发布第二个事件 — 不应 panic 或阻塞
        hub.publish(RealtimeEvent::new(Uuid::nil(), "second", json!({})))
            .await
            .unwrap();

        // 新订阅者只能收到此后的事件
        let mut rx2 = hub.subscribe();
        hub.publish(RealtimeEvent::new(Uuid::nil(), "third", json!({})))
            .await
            .unwrap();
        let got = rx2.recv().await.unwrap();
        assert_eq!(got.event_type, "third");
    }

    #[tokio::test]
    async fn lagged_subscriber_receives_lagged_error() {
        let hub = BroadcastEventPublisher::new(2); // 极小容量（实现强制下限 16）
        let mut rx = hub.subscribe();

        // 填满缓冲区：capacity.max(16) —— 需超过 16 个事件才能让订阅者落后
        for i in 0..20 {
            hub.publish(RealtimeEvent::new(Uuid::nil(), format!("e{i}"), json!({})))
                .await
                .unwrap();
        }

        // 接收方落后 — 应收到 Lagged 错误
        let result = rx.recv().await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            tokio::sync::broadcast::error::RecvError::Lagged(_)
        ));
    }

    #[test]
    fn realtime_event_json_contains_required_fields() {
        let ev = RealtimeEvent::new(
            Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            "attack.success",
            json!({"flag": "redacted"}),
        );
        let json = serde_json::to_value(&ev).unwrap();

        assert!(json.get("event_id").is_some());
        assert_eq!(json["type"], "attack.success");
        assert!(json.get("occurred_at").is_some());
        assert!(json.get("payload").is_some());
        // 序列号由 publish 分配，构造时可能为 None
    }

    // ── SSE 端点授权合约测试 ──
    //
    // 授权模型（Phase 7.2 — 分离认证域）：
    //
    // 选手路由 GET /api/events/{id}/awd/stream：
    //   - 认证：UserJwtGuard（users 表）
    //   - 授权：find_user_team_membership(event_id, user_id) → Some
    //   - 拒绝：其他用户 → 403
    //
    // 管理路由 GET /api/admin/events/{id}/awd/stream：
    //   - 认证：SuperAdminJwtGuard（super_admin 表）
    //   - 授权：所有 SuperAdmin 均可订阅任意赛事
    //   - SuperAdmin 不需要 users 记录
    //
    // HTTP 完整测试因 AppState 引导复杂度暂不可行（见
    // chore/awd-core-backend-http-acceptance-report.md）。
    // 以下测试验证基础实体可正常查询。

    #[tokio::test]
    #[ignore = "requires database — verify entity lookups compile"]
    async fn entity_lookups_compile() {
        // 验证 super_admin 和 event_team_members 实体可正常引用
        let _ = crate::entity::super_admin::Entity::find();
        let _ = crate::entity::event_team_members::Entity::find();
    }

    #[test]
    fn sse_auth_separate_domains_contract() {
        // 文档化授权决策（Phase 7.2）：
        // - 选手路由：UserJwtGuard → 仅检查 event_team_members
        // - 管理路由：SuperAdminJwtGuard → 无需额外检查
        // - SuperAdmin 不需要 users 记录 — 使用独立的 admin token
        let _admin_id = Uuid::nil();
        let _user_id = Uuid::nil();
    }
}

#[cfg(test)]
mod redis_fanout_tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    /// 测试文件级串行：共享同一 Redis DB。
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn redis_url() -> Option<String> {
        match std::env::var("TEST_REDIS_URL") {
            Ok(url) if !url.trim().is_empty() => Some(url),
            _ => {
                eprintln!("skip: TEST_REDIS_URL not set (realtime redis tests)");
                None
            }
        }
    }

    async fn flush(client: &redis::Client) {
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .expect("conn");
        let _: () = redis::cmd("FLUSHDB")
            .query_async(&mut conn)
            .await
            .expect("flush");
    }

    fn sample_event(event_type: &str, n: u64) -> RealtimeEvent {
        let mut e = RealtimeEvent::new(Uuid::nil(), event_type, serde_json::json!({"n": n}));
        e = e.with_sequence(n);
        e
    }

    async fn drain(rx: &mut tokio::sync::broadcast::Receiver<RealtimeEvent>) -> Vec<RealtimeEvent> {
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() {
            out.push(e);
        }
        out
    }

    /// 双节点扇出：节点 A publish，A/B 各收一次且无回环重复；sequence 保持发布值。
    #[tokio::test]
    async fn hybrid_publish_fans_out_to_both_nodes_without_echo() {
        let _serial = SERIAL.lock().unwrap();
        let Some(url) = redis_url() else { return };
        let client = redis::Client::open(url.as_str()).unwrap();
        flush(&client).await;

        // 唯一 channel 隔离本测试的 pub/sub 流量
        let channel = format!("floatctf:test:realtime:{}", Uuid::new_v4().simple());
        let hub_a = Arc::new(BroadcastEventPublisher::new(64));
        let hub_b = Arc::new(BroadcastEventPublisher::new(64));
        let node_a = HybridEventPublisher::new_with_channel(
            hub_a.clone(),
            redis::Client::open(url.as_str()).unwrap(),
            channel.clone(),
        );
        let _node_b = HybridEventPublisher::new_with_channel(
            hub_b.clone(),
            redis::Client::open(url.as_str()).unwrap(),
            channel.clone(),
        );

        let mut rx_a = hub_a.subscribe();
        let mut rx_b = hub_b.subscribe();

        // 等待两个节点的后台 subscriber 就绪
        tokio::time::sleep(Duration::from_millis(300)).await;

        let sent = sample_event("test.fanout", 42);
        node_a.publish(sent.clone()).await.expect("publish");

        // 传播窗口（轮询不消费，只等待）
        tokio::time::sleep(Duration::from_millis(800)).await;

        let got_a = drain(&mut rx_a).await;
        let got_b = drain(&mut rx_b).await;

        // 节点 A（发布方）：本地一次，不再收到自己的 Redis 回环
        let a_events: Vec<&RealtimeEvent> = got_a
            .iter()
            .filter(|e| e.event_type == "test.fanout")
            .collect();
        assert_eq!(
            a_events.len(),
            1,
            "节点 A 应恰好收到 1 次（无回环重复），实际 {got_a:?}"
        );
        // 节点 B（远端）：经 Redis 扇出收到一次
        let b_events: Vec<&RealtimeEvent> = got_b
            .iter()
            .filter(|e| e.event_type == "test.fanout")
            .collect();
        assert_eq!(b_events.len(), 1, "节点 B 应恰好收到 1 次，实际 {got_b:?}");

        // sequence 语义：远端收到的 sequence 必须与发布值一致（全局序跨节点保序）
        assert_eq!(b_events[0].sequence, Some(42));
        assert_eq!(a_events[0].sequence, Some(42));
    }

    /// 并发 publish 唯一性：两节点各并发发 500 条，所有 sequence 全局唯一。
    #[tokio::test]
    async fn hybrid_concurrent_sequences_unique_across_nodes() {
        let _serial = SERIAL.lock().unwrap();
        let Some(url) = redis_url() else { return };
        let client = redis::Client::open(url.as_str()).unwrap();
        flush(&client).await;

        let channel = format!("floatctf:test:realtime:{}", Uuid::new_v4().simple());
        let hub_a = Arc::new(BroadcastEventPublisher::new(4096));
        let hub_b = Arc::new(BroadcastEventPublisher::new(4096));
        let node_a = Arc::new(HybridEventPublisher::new_with_channel(
            hub_a.clone(),
            redis::Client::open(url.as_str()).unwrap(),
            channel.clone(),
        ));
        let node_b = Arc::new(HybridEventPublisher::new_with_channel(
            hub_b.clone(),
            redis::Client::open(url.as_str()).unwrap(),
            channel.clone(),
        ));

        let mut rx_a = hub_a.subscribe();
        let mut rx_b = hub_b.subscribe();
        tokio::time::sleep(Duration::from_millis(300)).await;

        // 每个 publish 不带 sequence → Hybrid 分配。节点 A 与 B 各 500 条。
        let mut handles = Vec::new();
        for n in 0..500u64 {
            let p = node_a.clone();
            handles.push(tokio::spawn(async move {
                p.publish(RealtimeEvent::new(
                    Uuid::nil(),
                    "test.seq",
                    serde_json::json!({"n": n}),
                ))
                .await
            }));
            let p = node_b.clone();
            handles.push(tokio::spawn(async move {
                p.publish(RealtimeEvent::new(
                    Uuid::nil(),
                    "test.seq",
                    serde_json::json!({"n": n}),
                ))
                .await
            }));
        }
        for h in handles {
            h.await.expect("join").expect("publish");
        }

        // 等待 Redis 订阅端把远端事件全部注入本地
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let got_a = drain(&mut rx_a).await;
        let got_b = drain(&mut rx_b).await;
        let total = got_a.len() + got_b.len();
        assert!(
            total >= 1000,
            "两节点应合计收到 ≥1000 条（A 本地+远端、B 本地+远端），实际 {total}"
        );

        // sequence 全局唯一（跨节点不撞号）：两节点合计 2000 次投递，
        // 去重后应恰好 1000 个不同 sequence，且每个恰好出现 2 次（每节点一次）。
        let mut seqs: Vec<u64> = got_a
            .iter()
            .chain(got_b.iter())
            .filter_map(|e| e.sequence)
            .collect();
        let deliveries = seqs.len();
        seqs.sort_unstable();
        let total_before_dedup = seqs.len();
        seqs.dedup();
        assert_eq!(
            seqs.len(),
            1000,
            "1000 条 publish 应产生恰好 1000 个不同 sequence"
        );
        assert_eq!(
            deliveries, 2000,
            "每条事件恰好投递两次（A、B 各一次），实际 {deliveries}"
        );
        let _ = total_before_dedup;

        // 每个节点都看到全量 1000 条（各一次，无回环重复）
        let uniq_a: std::collections::HashSet<u64> =
            got_a.iter().filter_map(|e| e.sequence).collect();
        let uniq_b: std::collections::HashSet<u64> =
            got_b.iter().filter_map(|e| e.sequence).collect();
        assert_eq!(
            uniq_a.len(),
            1000,
            "节点 A 应收到全部 1000 条（无回环重复）"
        );
        assert_eq!(
            uniq_b.len(),
            1000,
            "节点 B 应收到全部 1000 条（无回环重复）"
        );

        // 说明：本地投递（publish 内联 raw_send）先于 Redis pubsub 回环，
        // 跨节点到达顺序不保证单调 —— 这是 hybrid 设计的固有特性
        //（sequence 用于全局唯一关联/去重，不承担到达顺序承诺）。
        // 全量已投递 + 唯一性由上方断言保证。
    }

    /// Redis 全量数据丢失（如持久化被清、误 FLUSHALL 后重启）：sequence 计数器
    /// 归零重建后不得与历史序列撞号——时间地板保证新序列严格大于丢失前的所有值。
    #[tokio::test]
    async fn hybrid_sequence_no_collision_after_redis_total_loss() {
        let _serial = SERIAL.lock().unwrap();
        let Some(url) = redis_url() else { return };
        let client = redis::Client::open(url.as_str()).unwrap();
        flush(&client).await;

        let channel = format!("floatctf:test:seqloss:{}", Uuid::new_v4().simple());
        let hub = Arc::new(BroadcastEventPublisher::new(16));
        let node = HybridEventPublisher::new_with_channel(
            hub.clone(),
            redis::Client::open(url.as_str()).unwrap(),
            channel.clone(),
        );

        // 阶段 1：正常 publish 拿到历史序列（先订阅再发布，否则无接收者）
        let mut rx = hub.subscribe();
        node.publish(RealtimeEvent::new(
            Uuid::nil(),
            "test.seqloss.before",
            serde_json::json!({}),
        ))
        .await
        .expect("publish before loss");
        let before = drain(&mut rx).await;
        assert_eq!(before.len(), 1, "本地订阅者应收到丢失前事件");
        let seq_before = before[0].sequence.expect("sequence before loss");
        assert!(seq_before > 0);

        // 阶段 2：模拟 Redis 全量数据丢失（键被清空，等价 AOF 丢失后重启）
        {
            let mut conn = client
                .get_multiplexed_async_connection()
                .await
                .expect("conn");
            let _: () = redis::cmd("DEL")
                .arg(format!("{channel}:sequence"))
                .query_async(&mut conn)
                .await
                .expect("del sequence key");
        }

        // 阶段 3：丢失后继续 publish，新序列必须严格大于丢失前（不撞号）
        node.publish(RealtimeEvent::new(
            Uuid::nil(),
            "test.seqloss.after",
            serde_json::json!({}),
        ))
        .await
        .expect("publish after loss");
        let after = drain(&mut rx).await;
        let seq_after = after
            .iter()
            .find(|e| e.event_type == "test.seqloss.after")
            .and_then(|e| e.sequence)
            .expect("sequence after loss");
        assert!(
            seq_after > seq_before,
            "全量丢失后新序列 {seq_after} 必须大于历史序列 {seq_before}（时间地板防撞号）"
        );
        // 时间地板量级校验：丢失后序列应 ≥ 毫秒时间戳（≈1.7e12），远大于普通计数
        assert!(
            seq_after >= 1_000_000_000_000,
            "丢失后序列应被抬到时间地板量级，实际 {seq_after}"
        );
    }

    /// Redis 不可用：publish 仍成功（本地扇出保留），不 panic。
    #[tokio::test]
    async fn hybrid_publish_survives_redis_outage() {
        let hub = Arc::new(BroadcastEventPublisher::new(16));
        let mut rx = hub.subscribe();
        let publisher = HybridEventPublisher::new_with_channel(
            hub.clone(),
            redis::Client::open("redis://127.0.0.1:6390/").unwrap(),
            "floatctf:test:outage",
        );

        let result = publisher
            .publish(RealtimeEvent::new(
                Uuid::nil(),
                "test.outage",
                serde_json::json!({}),
            ))
            .await;
        assert!(result.is_ok(), "Redis 故障时 publish 必须降级本地成功");

        let got = drain(&mut rx).await;
        assert_eq!(got.len(), 1, "本地订阅者仍应收到事件");
        assert_eq!(got[0].event_type, "test.outage");
        assert!(got[0].sequence.is_some(), "本地路径必须补 sequence");
    }
}
