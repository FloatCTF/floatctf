//! AWD 调度任务注册与处理器。

use crate::entity::scheduled_tasks;
use crate::infrastructure::{WebDb, WebDocker};
use crate::scheduler::{TaskHandler, TaskKey};
use async_trait::async_trait;
use chrono::{DateTime, Duration, FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use serde::Deserialize;
use tracing::{info, warn};
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::{AwdEventStatus, AwdPhase, RoundStatus};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    crypto::{AwdCrypto, EncryptedBlob},
    domain::AwdEventStatusExt,
    infrastructure::{firewall::FirewallRuntime, network::AwdNetworkRuntime},
    repo::{event_repo, round_repo},
    service::{event_service, firewall_service, round_service},
};
use fcmc::AwdContainerRuntime;
use std::sync::Arc;

/// 轮次相关任务的载荷。
#[derive(Debug, Deserialize)]
struct RoundTaskPayload {
    event_id: Uuid,
}

/// 当赛事开始仍足够远时，返回 T-1h 的执行时间。
/// 一小时内开赛的赛事须显式人工预检。
pub fn automatic_precheck_at(
    start_time: DateTime<FixedOffset>,
    now: DateTime<Utc>,
) -> Option<DateTime<FixedOffset>> {
    let execute_at = start_time.with_timezone(&Utc) - Duration::hours(1);
    (execute_at >= now).then(|| execute_at.fixed_offset())
}

/// 每场 AWD 赛事创建一次一次性自动预检任务。
/// 返回 `None` for near-term Events and when the task already exists。
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
        .filter(scheduled_tasks::Column::Status.is_in(["pending", "running"]))
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

/// 按有效开赛时间重排自动预检。近期开赛（不足一小时）会删除 pending 预检，
/// running 任务拒绝修改，避免已经被 worker 领取的旧任务继续执行。
pub async fn replace_auto_precheck_schedule<C: ConnectionTrait + Send>(
    db: &C,
    event_id: Uuid,
    start_time: DateTime<FixedOffset>,
    now: DateTime<Utc>,
) -> AwdResult<()> {
    use sea_orm::sea_query::LockType;

    let task_key = TaskKey::AwdAutoPrecheck.to_string();
    let active_tasks = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq(&task_key))
        .filter(scheduled_tasks::Column::Status.is_in(["pending", "running"]))
        .lock(LockType::Update)
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    if active_tasks.len() > 1 {
        return Err(AwdError::Conflict(format!(
            "multiple active automatic-precheck tasks exist for event {event_id}"
        )));
    }
    let existing = active_tasks.into_iter().next();
    if existing
        .as_ref()
        .is_some_and(|task| task.status == "running")
    {
        return Err(AwdError::Conflict(
            "automatic precheck is already running and cannot be rescheduled".into(),
        ));
    }

    match (existing, automatic_precheck_at(start_time, now)) {
        (Some(task), Some(execute_at)) => {
            scheduled_tasks::ActiveModel {
                id: Set(task.id),
                enabled: Set(true),
                execute_at: Set(Some(execute_at)),
                expires_at: Set(Some(start_time)),
                error_msg: Set(None),
                attempt_count: Set(0),
                last_error: Set(None),
                updated_at: Set(Utc::now().into()),
                ..Default::default()
            }
            .update(db)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        }
        (Some(task), None) => {
            scheduled_tasks::Entity::delete_by_id(task.id)
                .exec(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
        }
        (None, Some(_)) => {
            schedule_auto_precheck(db, event_id, start_time, now)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
        }
        (None, None) => {}
    }
    Ok(())
}

/// 在 `planned_start_at` 创建一次性开赛任务（P2-12）。
/// 幂等：同一 event 已存在 active AwdEventStart 任务则跳过。
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
        .filter(scheduled_tasks::Column::Status.is_in(["pending", "running"]))
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

/// 查询当前有效定时开赛时间；completed/failed 历史任务不会回显。
pub async fn find_event_start_schedule<C: ConnectionTrait + Send>(
    db: &C,
    event_id: Uuid,
) -> Result<Option<DateTime<FixedOffset>>, sea_orm::DbErr> {
    let tasks = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq(TaskKey::AwdEventStart.to_string()))
        .filter(scheduled_tasks::Column::Status.is_in(["pending", "running"]))
        .filter(scheduled_tasks::Column::Enabled.eq(true))
        .order_by_desc(scheduled_tasks::Column::UpdatedAt)
        .all(db)
        .await?;
    if tasks.len() > 1 {
        return Err(sea_orm::DbErr::Custom(format!(
            "multiple active planned-start tasks exist for event {event_id}"
        )));
    }
    Ok(tasks.into_iter().next().and_then(|task| task.execute_at))
}

/// 手动 Start 成功后取消尚未被 worker 领取的定时开赛/自动预检任务。
pub async fn cancel_pending_event_lifecycle_schedules<C: ConnectionTrait + Send>(
    db: &C,
    event_id: Uuid,
) -> Result<u64, sea_orm::DbErr> {
    let result = scheduled_tasks::Entity::delete_many()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.is_in([
            TaskKey::AwdEventStart.to_string(),
            TaskKey::AwdAutoPrecheck.to_string(),
        ]))
        .filter(scheduled_tasks::Column::Status.eq("pending"))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

/// Configure 页更新定时开赛任务。
///
/// 只能修改/删除 pending 任务；任务一旦被 scheduler claim 为 running，返回 Conflict，
/// 避免 DB 显示已重排但旧 worker 仍按旧时间启动赛事。
pub async fn replace_event_start_schedule<C: ConnectionTrait + Send>(
    db: &C,
    event_id: Uuid,
    planned_start_at: Option<DateTime<FixedOffset>>,
) -> AwdResult<()> {
    use sea_orm::sea_query::LockType;

    let task_key = TaskKey::AwdEventStart.to_string();
    let active_tasks = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::GroupId.eq(event_id))
        .filter(scheduled_tasks::Column::TaskKey.eq(&task_key))
        .filter(scheduled_tasks::Column::Status.is_in(["pending", "running"]))
        .lock(LockType::Update)
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    if active_tasks.len() > 1 {
        return Err(AwdError::Conflict(format!(
            "multiple active planned-start tasks exist for event {event_id}"
        )));
    }
    let existing = active_tasks.into_iter().next();
    if existing
        .as_ref()
        .is_some_and(|task| task.status == "running")
    {
        return Err(AwdError::Conflict(
            "planned start is already being executed and cannot be changed".into(),
        ));
    }

    match (existing, planned_start_at) {
        (Some(task), Some(start_at)) => {
            scheduled_tasks::ActiveModel {
                id: Set(task.id),
                enabled: Set(true),
                execute_at: Set(Some(start_at)),
                expires_at: Set(Some(start_at + Duration::hours(6))),
                error_msg: Set(None),
                attempt_count: Set(0),
                last_error: Set(None),
                updated_at: Set(Utc::now().into()),
                ..Default::default()
            }
            .update(db)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
        }
        (Some(task), None) => {
            scheduled_tasks::Entity::delete_by_id(task.id)
                .exec(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
        }
        (None, Some(start_at)) => {
            schedule_event_start(db, event_id, Some(start_at))
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
        }
        (None, None) => {}
    }
    Ok(())
}

fn event_id_from_task(task: &scheduled_tasks::Model) -> anyhow::Result<Uuid> {
    let payload: RoundTaskPayload =
        serde_json::from_value(task.payload.clone().unwrap_or_default())?;
    Ok(payload.event_id)
}

/// 处理器：执行 AWD 赛前自动预检。
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
        if let Some(event) = event_repo::find_by_event_id(self.db.get_ref(), event_id).await?
            && matches!(
                event.status,
                AwdEventStatus::Running
                    | AwdEventStatus::Paused
                    | AwdEventStatus::NetworkError
                    | AwdEventStatus::Finished
                    | AwdEventStatus::Archived
            )
        {
            info!(
                "[AWD] Event {} already started ({:?}); skipping stale automatic precheck",
                event_id, event.status
            );
            return Ok(());
        }
        info!("[AWD] Running automatic precheck for event {}", event_id);
        crate::modules::event::awd::service::precheck_service::run_precheck(
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

/// 处理器：在计划开赛时间启动已通过预检的 AWD 赛事。
pub struct AwdEventStartHandler {
    pub db: WebDb,
    pub network:
        std::sync::Arc<dyn crate::modules::event::awd::infrastructure::network::AwdNetworkRuntime>,
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
        if let Some(event) = event_repo::find_by_event_id(self.db.get_ref(), event_id).await?
            && matches!(
                event.status,
                AwdEventStatus::Running
                    | AwdEventStatus::Paused
                    | AwdEventStatus::NetworkError
                    | AwdEventStatus::Finished
                    | AwdEventStatus::Archived
            )
        {
            info!(
                "[AWD] Event {} already started ({:?}); skipping stale scheduled start",
                event_id, event.status
            );
            return Ok(());
        }
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
            payload.round_number,
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
        crate::modules::event::awd::service::ban_service::unban_team_by_ban_id(
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

/// 处理器：保留期结束后清理已归档赛事。
pub struct AwdArchiveCleanupHandler {
    pub db: WebDb,
    pub docker: WebDocker,
    pub network: Arc<dyn AwdNetworkRuntime>,
    pub firewall: Arc<dyn FirewallRuntime>,
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

        // P4-12：按每个赛事的 archive_retention_hours 计算 cutoff（不再硬编码 168h）
        let now = chrono::Utc::now();
        let finished = awd_events::Entity::find()
            .filter(awd_events::Column::Status.eq(AwdEventStatus::Finished))
            .all(self.db.get_ref())
            .await?;

        let mut event_count = 0usize;
        for event in finished {
            let retention = event.archive_retention_hours.max(0) as i64;
            let cutoff = now - chrono::Duration::hours(retention);
            let expired = event
                .finished_at
                .map(|f| f.with_timezone(&chrono::Utc) <= cutoff)
                .unwrap_or(false);
            if !expired {
                continue;
            }
            info!(
                "[AWD] Archiving event {} (retention {}h expired)",
                event.event_id, retention
            );
            crate::modules::event::awd::service::archive_service::archive_event(
                self.db.get_ref(),
                self.containers.as_ref(),
                self.network.as_ref(),
                self.firewall.as_ref(),
                event.event_id,
            )
            .await?;
            event_count += 1;
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
