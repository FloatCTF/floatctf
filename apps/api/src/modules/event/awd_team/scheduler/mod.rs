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
    infrastructure::network::AwdNetworkRuntime,
    repo::{event_repo, judge_repo, round_repo},
    service::{event_service, judge_service, network_policy_service},
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

fn event_id_from_task(task: &scheduled_tasks::Model) -> anyhow::Result<Uuid> {
    let payload: RoundTaskPayload =
        serde_json::from_value(task.payload.clone().unwrap_or_default())?;
    Ok(payload.event_id)
}

/// Handler: Run the automatic precheck scheduled before an AWD event.
pub struct AwdAutoPrecheckHandler {
    pub db: WebDb,
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
        event_service::start_event(self.db.get_ref(), self.network.as_ref(), event_id).await?;
        Ok(())
    }
}

/// Handler: Start a new AWD round.
pub struct AwdRoundStartHandler {
    pub db: WebDb,
    pub docker: WebDocker,
    pub network: Arc<dyn AwdNetworkRuntime>,
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
        let event_id = event_id_from_task(&task)?;

        info!("[AWD] Starting new round for event {}", event_id);

        // Verify event is running
        let awd_event = event_repo::find_by_event_id(self.db.get_ref(), event_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("AWD event not found"))?;

        if !awd_event.status.is_active() {
            warn!(
                "[AWD] Event {} is not active, skipping round start",
                event_id
            );
            return Ok(());
        }

        // Complete the previous round if any
        if let Some(prev_round) = round_repo::find_active_round(self.db.get_ref(), event_id).await?
        {
            // Timeout pending judge tasks from previous round
            let timed_out =
                judge_repo::timeout_pending_tasks(self.db.get_ref(), prev_round.id).await?;
            info!(
                "[AWD] Round {} ended. {} tasks timed out.",
                prev_round.round_number, timed_out
            );

            round_repo::update_round_status(
                self.db.get_ref(),
                prev_round.id,
                RoundStatus::Completed,
            )
            .await?;
        }

        // Get the latest round number
        let latest = round_repo::find_latest_round(self.db.get_ref(), event_id).await?;
        let next_round_number = latest.map(|r| r.round_number + 1).unwrap_or(1);

        // Determine phase (round 1 starts with hardening, subsequent rounds are attack)
        let phase = if next_round_number == 1 {
            AwdPhase::Hardening
        } else {
            AwdPhase::Attack
        };
        let phase_debug = format!("{:?}", phase);

        // Update event phase
        crate::modules::event::awd_team::repo::event_repo::update_phase(
            self.db.get_ref(),
            awd_event.id,
            phase.clone(),
        )
        .await?;

        let now = chrono::Utc::now();
        let round_end = now + chrono::Duration::seconds(awd_event.round_duration_secs as i64);

        let new_round = round_repo::create_round(
            self.db.get_ref(),
            event_id,
            next_round_number,
            phase.clone(),
            round_end,
        )
        .await?;

        info!(
            "[AWD] Round {} started for event {} (phase: {})",
            new_round.round_number, event_id, phase_debug
        );

        // Same path as start/pause/resume: phase firewall + conntrack flush.
        network_policy_service::apply_phase_policy(
            self.db.get_ref(),
            self.network.as_ref(),
            event_id,
            phase,
        )
        .await?;

        let token_ciphertext = awd_event
            .judgeserver_token_ciphertext
            .clone()
            .ok_or_else(|| anyhow::anyhow!("JudgeServer token is not configured"))?;
        let token_nonce = awd_event
            .judgeserver_token_nonce
            .clone()
            .ok_or_else(|| anyhow::anyhow!("JudgeServer token nonce is not configured"))?;
        let crypto = AwdCrypto::from_config_secret()?;
        let token = crypto.decrypt(
            &EncryptedBlob {
                ciphertext: token_ciphertext,
                nonce: token_nonce,
                key_version: awd_event.key_version,
            },
            &AwdCrypto::build_aad(event_id, "internal_token"),
        )?;
        let token = String::from_utf8(token)
            .map_err(|_| anyhow::anyhow!("JudgeServer token is not valid UTF-8"))?;
        let batch_id =
            judge_service::create_batch(self.db.get_ref(), event_id, new_round.id).await?;
        let judgeserver_url = format!("http://{}:8082", awd_event.judgeserver_ip);
        judge_service::dispatch_batch(self.db.get_ref(), batch_id, &judgeserver_url, &token)
            .await?;

        Ok(())
    }
}

/// Handler: End the current AWD round (grace period).
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
        let event_id = event_id_from_task(&task)?;

        info!("[AWD] Ending round for event {}", event_id);

        let active_round = match round_repo::find_active_round(self.db.get_ref(), event_id).await? {
            Some(r) => r,
            None => {
                warn!("[AWD] No active round for event {}", event_id);
                return Ok(());
            }
        };

        // Start grace period
        round_repo::update_round_status(self.db.get_ref(), active_round.id, RoundStatus::Grace)
            .await?;

        info!(
            "[AWD] Round {} in grace period for event {}",
            active_round.round_number, event_id
        );

        Ok(())
    }
}

/// Handler: Grace period ends — complete the round.
pub struct AwdRoundGraceEndHandler {
    pub db: WebDb,
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
        let event_id = event_id_from_task(&task)?;

        info!("[AWD] Grace period ending for event {}", event_id);

        // Timeout any remaining pending tasks
        let active_round = match round_repo::find_active_round(self.db.get_ref(), event_id).await? {
            Some(r) => r,
            None => return Ok(()),
        };

        let timed_out =
            judge_repo::timeout_pending_tasks(self.db.get_ref(), active_round.id).await?;
        info!(
            "[AWD] Grace ended. {} tasks timed out for round {}.",
            timed_out, active_round.round_number
        );

        // Complete the round
        round_repo::update_round_status(self.db.get_ref(), active_round.id, RoundStatus::Completed)
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
