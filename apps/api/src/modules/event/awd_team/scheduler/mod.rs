//! AWD scheduler handlers — integrates with the existing TaskScheduler.
//!
//! Task keys:
//! - `awd.event.auto_precheck` — run the event precheck before start
//! - `awd.event.start` — start a verified event
//! - `awd.round.start` — start a new round
//! - `awd.round.end` — end the current round (start grace period)
//! - `awd.round.grace_end` — complete a round after its grace period
//! - `awd.archive.cleanup` — archive cleanup after retention period

use crate::entity::scheduled_tasks;
use crate::infrastructure::{WebDb, WebDocker};
use crate::scheduler::{TaskHandler, TaskKey};
use async_trait::async_trait;
use chrono::{DateTime, Duration, FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};
use serde::Deserialize;
use tracing::{info, warn};
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::{AwdEventStatus, AwdPhase, RoundStatus};
use crate::modules::event::awd_team::{
    crypto::{AwdCrypto, EncryptedBlob},
    domain::AwdEventStatusExt,
    infrastructure::{firewall::FirewallRuntime, network::AwdNetworkRuntime},
    repo::{event_repo, round_repo},
    service::{event_service, firewall_service, round_service},
};
use fcmc::AwdContainerRuntime;
use std::sync::Arc;

/// Payload for round-related tasks.
#[derive(Debug, Deserialize)]
struct RoundTaskPayload {
    event_id: Uuid,
}

/// Return the T-1h execution time when an event is far enough in the future.
/// Events starting in less than one hour require an explicit manual precheck.
pub fn automatic_precheck_at(
    start_time: DateTime<FixedOffset>,
    now: DateTime<Utc>,
) -> Option<DateTime<FixedOffset>> {
    let execute_at = start_time.with_timezone(&Utc) - Duration::hours(1);
    (execute_at >= now).then(|| execute_at.fixed_offset())
}

/// Create the one-shot automatic precheck task once per AWD Event.
/// Returns `None` for near-term Events and when the task already exists.
pub async fn schedule_auto_precheck<C: ConnectionTrait + Send>(
    db: &C,
    event_id: Uuid,
    start_time: DateTime<FixedOffset>,
    now: DateTime<Utc>,
) -> Result<Option<scheduled_tasks::Model>, sea_orm::DbErr> {
    let Some(execute_at) = automatic_precheck_at(start_time, now) else {
        return Ok(None);
    };

    let task_key = TaskKey::AwdAutoPrecheck.to_string();
    let exists = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq(&task_key))
        .one(db)
        .await?;
    if exists.is_some() {
        return Ok(None);
    }

    let task = scheduled_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        group_id: Set(Some(event_id)),
        task_name: Set(format!("AWD Event {event_id} automatic precheck")),
        description: Set(Some(
            "Automatic precheck one hour before Event start".into(),
        )),
        task_key: Set(task_key),
        trigger_type: Set("once".into()),
        status: Set("pending".into()),
        execute_at: Set(Some(execute_at)),
        expires_at: Set(Some(start_time)),
        payload: Set(Some(serde_json::json!({ "event_id": event_id }))),
        enabled: Set(true),
        protected: Set(true),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(Some(task))
}

/// Create the one-shot event-start task at `planned_start_at`（P2-12）。
/// 幂等：同一 event 已存在 AwdEventStart 任务则跳过。
/// 无 planned start 时间时返回 None（手动开始）。
pub async fn schedule_event_start<C: ConnectionTrait + Send>(
    db: &C,
    event_id: Uuid,
    planned_start_at: Option<DateTime<FixedOffset>>,
) -> Result<Option<scheduled_tasks::Model>, sea_orm::DbErr> {
    let Some(start_at) = planned_start_at else {
        return Ok(None);
    };

    let task_key = TaskKey::AwdEventStart.to_string();
    let exists = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq(&task_key))
        .one(db)
        .await?;
    if exists.is_some() {
        return Ok(None);
    }

    let now = Utc::now();
    let task = scheduled_tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        group_id: Set(Some(event_id)),
        task_name: Set(format!("AWD Event {event_id} planned start")),
        description: Set(Some(
            "Planned start: begin competition at scheduled time".into(),
        )),
        task_key: Set(task_key),
        trigger_type: Set("once".into()),
        status: Set("pending".into()),
        execute_at: Set(Some(start_at)),
        expires_at: Set(Some(start_at + Duration::hours(6))),
        payload: Set(Some(serde_json::json!({ "event_id": event_id }))),
        enabled: Set(true),
        protected: Set(true),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(Some(task))
}

fn event_id_from_task(task: &scheduled_tasks::Model) -> anyhow::Result<Uuid> {
    let payload: RoundTaskPayload =
        serde_json::from_value(task.payload.clone().unwrap_or_default())?;
    Ok(payload.event_id)
}

/// Handler: Run the automatic precheck scheduled before an AWD event.
pub struct AwdAutoPrecheckHandler {
    pub db: WebDb,
    pub network: Arc<dyn AwdNetworkRuntime>,
    pub firewall: Arc<dyn FirewallRuntime>,
    pub containers: Arc<dyn AwdContainerRuntime>,
    pub crypto: Arc<AwdCrypto>,
}

#[async_trait]
impl TaskHandler for AwdAutoPrecheckHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdAutoPrecheck
    }

    fn trigger_type(&self) -> &'static str {
        "once"
    }

    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()> {
        let event_id = event_id_from_task(&task)?;
        info!("[AWD] Running automatic precheck for event {}", event_id);
        crate::modules::event::awd_team::service::precheck_service::run_precheck(
            self.db.get_ref(),
            event_id,
            "scheduled",
            self.network.as_ref(),
            self.firewall.as_ref(),
            self.containers.as_ref(),
            self.crypto.as_ref(),
        )
        .await?;
        Ok(())
    }
}

/// Handler: Start a verified AWD event at its scheduled start time.
pub struct AwdEventStartHandler {
    pub db: WebDb,
    pub network: std::sync::Arc<
        dyn crate::modules::event::awd_team::infrastructure::network::AwdNetworkRuntime,
    >,
    pub firewall: Arc<dyn FirewallRuntime>,
    pub publisher: Arc<dyn crate::infrastructure::realtime::EventPublisher>,
}

#[async_trait]
impl TaskHandler for AwdEventStartHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdEventStart
    }

    fn trigger_type(&self) -> &'static str {
        "once"
    }

    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()> {
        let event_id = event_id_from_task(&task)?;
        info!("[AWD] Starting scheduled event {}", event_id);
        event_service::start_event(
            self.db.get_ref(),
            self.network.as_ref(),
            self.firewall.as_ref(),
            self.publisher.as_ref(),
            event_id,
        )
        .await?;
        Ok(())
    }
}

/// Handler: Start a new AWD round（thin 代理 → round_service::start_round，P3-2）。
pub struct AwdRoundStartHandler {
    pub db: WebDb,
    pub network: Arc<dyn AwdNetworkRuntime>,
    pub firewall: Arc<dyn FirewallRuntime>,
    pub publisher: Arc<dyn crate::infrastructure::realtime::EventPublisher>,
}

#[async_trait]
impl TaskHandler for AwdRoundStartHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdRoundStart
    }

    fn trigger_type(&self) -> &'static str {
        "cron"
    }

    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()> {
        let payload: round_service::RoundTaskPayload =
            serde_json::from_value(task.payload.clone().unwrap_or_default())?;
        let event_id = payload.event_id;

        info!("[AWD] Starting new round for event {}", event_id);

        // 非 Running 赛事：跳过（scheduler 任务不失败）
        if let Some(ev) = event_repo::find_by_event_id(self.db.get_ref(), event_id).await? {
            if !ev.status.is_active() {
                warn!(
                    "[AWD] Event {} is not active ({:?}), skipping round start",
                    event_id, ev.status
                );
                return Ok(());
            }
        }

        round_service::start_round(
            self.db.get_ref(),
            self.network.as_ref(),
            self.firewall.as_ref(),
            self.publisher.as_ref(),
            event_id,
        )
        .await?;

        Ok(())
    }
}

/// Handler: End the current AWD round (grace period)——thin 代理 → round_service::end_round。
pub struct AwdRoundEndHandler {
    pub db: WebDb,
}

#[async_trait]
impl TaskHandler for AwdRoundEndHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdRoundEnd
    }

    fn trigger_type(&self) -> &'static str {
        "cron"
    }

    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()> {
        let payload: round_service::RoundTaskPayload =
            serde_json::from_value(task.payload.clone().unwrap_or_default())?;
        let event_id = payload.event_id;
        // end 任务携带 round_id；缺失时按 active round 兜底
        let round_id = match payload.round_id {
            Some(id) => id,
            None => match round_repo::find_active_round(self.db.get_ref(), event_id).await? {
                Some(r) => r.id,
                None => {
                    warn!("[AWD] No active round for event {}", event_id);
                    return Ok(());
                }
            },
        };

        info!("[AWD] Ending round {round_id} for event {event_id}");
        round_service::end_round(self.db.get_ref(), event_id, round_id).await?;
        Ok(())
    }
}

/// Handler: Grace period ends — complete the round（thin 代理 → round_service::grace_end_round）。
pub struct AwdRoundGraceEndHandler {
    pub db: WebDb,
    pub publisher: Arc<dyn crate::infrastructure::realtime::EventPublisher>,
}

#[async_trait]
impl TaskHandler for AwdRoundGraceEndHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdRoundGraceEnd
    }

    fn trigger_type(&self) -> &'static str {
        "cron"
    }

    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()> {
        let payload: round_service::RoundTaskPayload =
            serde_json::from_value(task.payload.clone().unwrap_or_default())?;
        let event_id = payload.event_id;
        let round_id = match payload.round_id {
            Some(id) => id,
            None => match round_repo::find_active_round(self.db.get_ref(), event_id).await? {
                Some(r) => r.id,
                None => {
                    warn!("[AWD] No active round for event {}", event_id);
                    return Ok(());
                }
            },
        };

        info!("[AWD] Grace period ending for round {round_id} event {event_id}");
        round_service::grace_end_round(
            self.db.get_ref(),
            event_id,
            round_id,
            self.publisher.as_ref(),
        )
        .await?;
        Ok(())
    }
}

/// Handler: 自动解封（P4-7，duration 到期任务）。
pub struct AwdTeamUnbanHandler {
    pub db: WebDb,
    pub network: Arc<dyn AwdNetworkRuntime>,
    pub firewall: Arc<dyn FirewallRuntime>,
    pub publisher: Arc<dyn crate::infrastructure::realtime::EventPublisher>,
}

#[async_trait]
impl TaskHandler for AwdTeamUnbanHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdTeamUnban
    }

    fn trigger_type(&self) -> &'static str {
        "once"
    }

    async fn run(&self, task: scheduled_tasks::Model) -> anyhow::Result<()> {
        let payload: round_service::RoundTaskPayload =
            serde_json::from_value(task.payload.clone().unwrap_or_default())?;
        let event_id = payload.event_id;
        let ban_id = payload
            .round_id
            .ok_or_else(|| anyhow::anyhow!("unban task missing ban_id"))?;

        info!("[Ban] Auto-unban task for event {event_id} ban {ban_id}");
        crate::modules::event::awd_team::service::ban_service::unban_team_by_ban_id(
            self.db.get_ref(),
            self.network.as_ref(),
            self.firewall.as_ref(),
            self.publisher.as_ref(),
            event_id,
            ban_id,
        )
        .await?;
        Ok(())
    }
}

/// Handler: Cleanup archived events after retention period.
pub struct AwdArchiveCleanupHandler {
    pub db: WebDb,
    pub docker: WebDocker,
    pub network: Arc<dyn AwdNetworkRuntime>,
    pub containers: Arc<dyn AwdContainerRuntime>,
}

#[async_trait]
impl TaskHandler for AwdArchiveCleanupHandler {
    fn task_key(&self) -> TaskKey {
        TaskKey::AwdArchiveCleanup
    }

    fn trigger_type(&self) -> &'static str {
        "cron"
    }

    async fn run(&self, _task: scheduled_tasks::Model) -> anyhow::Result<()> {
        info!("[AWD] Running archive cleanup");

        use crate::entity::awd_events;
        use sea_orm::QueryFilter;

        let now = chrono::Utc::now();
        let retention_cutoff = now - chrono::Duration::hours(168); // default 7 days

        let events = awd_events::Entity::find()
            .filter(awd_events::Column::Status.eq(AwdEventStatus::Finished))
            .filter(awd_events::Column::FinishedAt.lte(retention_cutoff))
            .all(self.db.get_ref())
            .await?;

        let event_count = events.len();
        for event in events {
            info!(
                "[AWD] Archiving event {} (retention period expired)",
                event.event_id
            );
            crate::modules::event::awd_team::service::archive_service::archive_event(
                self.db.get_ref(),
                self.containers.as_ref(),
                self.network.as_ref(),
                event.event_id,
            )
            .await?;
        }

        info!(
            "[AWD] Archive cleanup complete. {} events archived.",
            event_count
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn automatic_precheck_is_one_hour_before_start() {
        let now = Utc.with_ymd_and_hms(2026, 7, 27, 10, 0, 0).unwrap();
        let start = FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 7, 27, 20, 30, 0)
            .unwrap();

        let scheduled = automatic_precheck_at(start, now).unwrap();

        assert_eq!(scheduled, start - Duration::hours(1));
    }

    #[test]
    fn near_term_event_requires_manual_precheck() {
        let now = Utc.with_ymd_and_hms(2026, 7, 27, 10, 0, 0).unwrap();
        let start = now.fixed_offset() + Duration::minutes(59);

        assert_eq!(automatic_precheck_at(start, now), None);
    }

    #[test]
    fn exactly_one_hour_out_can_schedule_immediately() {
        let now = Utc.with_ymd_and_hms(2026, 7, 27, 10, 0, 0).unwrap();
        let start = now.fixed_offset() + Duration::hours(1);

        assert_eq!(automatic_precheck_at(start, now), Some(now.fixed_offset()));
    }
}
