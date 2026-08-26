//! AWD 赛事参数配置。
//!
//! `Configure` 页面是 AWD 专属配置的唯一管理入口：首次保存创建
//! `awd_events`，后续保存仅允许在未运行的可配置状态修改，并使已有
//! Verified 结果失效。

use chrono::{DateTime, FixedOffset};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QuerySelect, TransactionTrait,
};
use uuid::Uuid;

use crate::entity::{awd_events, sea_orm_active_enums::AwdEventStatus};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    repo::event_repo::{self, TransitionPatch},
    scheduler,
};

pub const DEFAULT_ROUND_DURATION_SECS: i32 = 300;
pub const DEFAULT_FREE_RESET_COUNT: i32 = 3;
pub const DEFAULT_EXTRA_RESET_PENALTY: i64 = 100;
pub const DEFAULT_RESET_PROTECTION_SECS: i32 = 120;
pub const DEFAULT_JUDGE_MAX_CONCURRENCY: i32 = 10;
pub const DEFAULT_JUDGE_TIMEOUT_SECS: i32 = 30;
pub const DEFAULT_JUDGE_RETRY_INTERVAL_SECS: i32 = 5;
pub const DEFAULT_JUDGE_GRACE_PERIOD_SECS: i32 = 30;
pub const DEFAULT_ARCHIVE_RETENTION_HOURS: i32 = 168;

/// PATCH /events/{id}/awd 的应用层输入。
#[derive(Debug, Clone, Default)]
pub struct AwdEventConfigPatch {
    /// 客户端读取配置时拿到的版本；不一致则拒绝覆盖其他管理员的修改。
    pub expected_updated_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub round_count: Option<i32>,
    pub round_duration_secs: Option<i32>,
    pub initial_score: Option<i64>,
    pub free_reset_count: Option<i32>,
    pub extra_reset_penalty: Option<i64>,
    pub reset_protection_secs: Option<i32>,
    pub judge_max_concurrency: Option<i32>,
    pub judge_default_timeout_secs: Option<i32>,
    pub judge_retry_interval_secs: Option<i32>,
    pub judge_grace_period_secs: Option<i32>,
    pub archive_retention_hours: Option<i32>,
    /// `None`=请求未携带；`Some(None)`=清除定时开赛；`Some(Some(_))`=设置。
    pub planned_start_at: Option<Option<DateTime<FixedOffset>>>,
}

impl AwdEventConfigPatch {
    pub fn validate(&self) -> AwdResult<()> {
        validate_range("round_count", self.round_count, 1, 10_000)?;
        validate_range("round_duration_secs", self.round_duration_secs, 30, 86_400)?;
        if let Some(value) = self.initial_score
            && !(0..=1_000_000_000).contains(&value)
        {
            return Err(AwdError::Validation(
                "initial_score must be between 0 and 1000000000".into(),
            ));
        }
        validate_range("free_reset_count", self.free_reset_count, 0, 100)?;
        if let Some(value) = self.extra_reset_penalty
            && !(0..=1_000_000_000).contains(&value)
        {
            return Err(AwdError::Validation(
                "extra_reset_penalty must be between 0 and 1000000000".into(),
            ));
        }
        validate_range(
            "reset_protection_secs",
            self.reset_protection_secs,
            0,
            86_400,
        )?;
        validate_range(
            "judge_max_concurrency",
            self.judge_max_concurrency,
            1,
            1_000,
        )?;
        validate_range(
            "judge_default_timeout_secs",
            self.judge_default_timeout_secs,
            1,
            3_600,
        )?;
        validate_range(
            "judge_retry_interval_secs",
            self.judge_retry_interval_secs,
            1,
            3_600,
        )?;
        validate_range(
            "judge_grace_period_secs",
            self.judge_grace_period_secs,
            0,
            3_600,
        )?;
        validate_range(
            "archive_retention_hours",
            self.archive_retention_hours,
            1,
            87_600,
        )?;
        if let Some(Some(start_at)) = self.planned_start_at
            && start_at <= chrono::Utc::now()
        {
            return Err(AwdError::Validation(
                "planned_start_at must be in the future".into(),
            ));
        }
        Ok(())
    }

    fn runtime_changed(&self, current: &awd_events::Model) -> bool {
        self.round_count
            .is_some_and(|v| Some(v) != current.round_count)
            || self
                .round_duration_secs
                .is_some_and(|v| v != current.round_duration_secs)
            || self
                .initial_score
                .is_some_and(|v| v != current.initial_score)
            || self
                .free_reset_count
                .is_some_and(|v| v != current.free_reset_count)
            || self
                .extra_reset_penalty
                .is_some_and(|v| v != current.extra_reset_penalty)
            || self
                .reset_protection_secs
                .is_some_and(|v| v != current.reset_protection_secs)
            || self
                .judge_max_concurrency
                .is_some_and(|v| v != current.judge_max_concurrency)
            || self
                .judge_default_timeout_secs
                .is_some_and(|v| v != current.judge_default_timeout_secs)
            || self
                .judge_retry_interval_secs
                .is_some_and(|v| v != current.judge_retry_interval_secs)
            || self
                .judge_grace_period_secs
                .is_some_and(|v| v != current.judge_grace_period_secs)
            || self
                .archive_retention_hours
                .is_some_and(|v| v != current.archive_retention_hours)
    }
}

fn validate_range(name: &str, value: Option<i32>, min: i32, max: i32) -> AwdResult<()> {
    if let Some(value) = value
        && !(min..=max).contains(&value)
    {
        return Err(AwdError::Validation(format!(
            "{name} must be between {min} and {max}"
        )));
    }
    Ok(())
}

fn assert_config_editable(status: &AwdEventStatus) -> AwdResult<()> {
    match status {
        AwdEventStatus::Draft
        | AwdEventStatus::Configuring
        | AwdEventStatus::Deployed
        | AwdEventStatus::Verified
        | AwdEventStatus::StartBlocked
        | AwdEventStatus::DeployFailed
        | AwdEventStatus::VerificationFailed => Ok(()),
        other => Err(AwdError::InvalidState(format!(
            "AWD configuration is locked in status {other:?}"
        ))),
    }
}

/// 更新 AWD 参数。runtime 参数变化时同事务执行：
/// 1. 状态回到 Configuring；2. configuration_generation +1；3. 清除验证结果。
/// 仅修改计划开赛时间不使 Precheck 失效，但仍更新 `updated_at` 参与乐观锁。
pub async fn update_event_config<C>(
    db: &C,
    event_id: Uuid,
    patch: AwdEventConfigPatch,
) -> AwdResult<awd_events::Model>
where
    C: ConnectionTrait + TransactionTrait + Send,
{
    use sea_orm::sea_query::LockType;

    patch.validate()?;
    let txn = db
        .begin()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    // 全局锁序：events → awd_events → scheduled_tasks。与首次 Configure 保持一致，
    // 避免父赛事时间和 AWD 调度配置出现并发快照漂移。
    let parent = crate::entity::events::Entity::find_by_id(event_id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("parent event not found".into()))?;
    let current = awd_events::Entity::find()
        .filter(awd_events::Column::EventId.eq(event_id))
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not configured".into()))?;

    assert_config_editable(&current.status)?;
    if let Some(expected) = patch.expected_updated_at
        && expected != current.updated_at
    {
        return Err(AwdError::Conflict(
            "AWD configuration was modified by another administrator; reload and retry".into(),
        ));
    }
    if let Some(Some(start_at)) = patch.planned_start_at
        && parent.end_time.is_some_and(|end| start_at >= end)
    {
        return Err(AwdError::Validation(
            "planned_start_at must be before the event end_time".into(),
        ));
    }
    let runtime_changed = patch.runtime_changed(&current);
    let schedule_changed = patch.planned_start_at.is_some();
    // 兼容旧数据：历史初始化接口可能留下 Draft；Configure 的一次 Save 必须
    // 正式推进到 Configuring，否则 Deploy 会触发非法 Draft→Deploying。
    let enter_configuring = current.status == AwdEventStatus::Draft || runtime_changed;

    if enter_configuring && current.status != AwdEventStatus::Configuring {
        event_repo::transition_event(
            &txn,
            current.id,
            current.status.clone(),
            AwdEventStatus::Configuring,
            TransitionPatch::config_changed(),
        )
        .await?;
    }

    if runtime_changed || schedule_changed {
        let mut active = awd_events::ActiveModel {
            id: Set(current.id),
            updated_at: Set(chrono::Utc::now().into()),
            ..Default::default()
        };
        if runtime_changed {
            active.configuration_generation = Set(current.configuration_generation + 1);
            active.verified_at = Set(None);
            active.verified_revision = Set(None);
            active.verified_generation = Set(None);
        }
        if let Some(value) = patch.round_count {
            active.round_count = Set(Some(value));
        }
        if let Some(value) = patch.round_duration_secs {
            active.round_duration_secs = Set(value);
        }
        if let Some(value) = patch.initial_score {
            active.initial_score = Set(value);
        }
        if let Some(value) = patch.free_reset_count {
            active.free_reset_count = Set(value);
        }
        if let Some(value) = patch.extra_reset_penalty {
            active.extra_reset_penalty = Set(value);
        }
        if let Some(value) = patch.reset_protection_secs {
            active.reset_protection_secs = Set(value);
        }
        if let Some(value) = patch.judge_max_concurrency {
            active.judge_max_concurrency = Set(value);
        }
        if let Some(value) = patch.judge_default_timeout_secs {
            active.judge_default_timeout_secs = Set(value);
        }
        if let Some(value) = patch.judge_retry_interval_secs {
            active.judge_retry_interval_secs = Set(value);
        }
        if let Some(value) = patch.judge_grace_period_secs {
            active.judge_grace_period_secs = Set(value);
        }
        if let Some(value) = patch.archive_retention_hours {
            active.archive_retention_hours = Set(value);
        }
        active
            .update(&txn)
            .await
            .map_err(|e| AwdError::Database(e.to_string()))?;
    }

    if let Some(planned_start_at) = patch.planned_start_at {
        let effective_start = planned_start_at.unwrap_or(parent.start_time);
        scheduler::replace_auto_precheck_schedule(
            &txn,
            event_id,
            effective_start,
            chrono::Utc::now(),
        )
        .await?;
        scheduler::replace_event_start_schedule(&txn, event_id, planned_start_at).await?;
    }

    txn.commit()
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not configured".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_boundaries() {
        let valid = AwdEventConfigPatch {
            round_duration_secs: Some(30),
            free_reset_count: Some(0),
            judge_max_concurrency: Some(1),
            archive_retention_hours: Some(1),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(
            AwdEventConfigPatch {
                round_duration_secs: Some(0),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            AwdEventConfigPatch {
                judge_max_concurrency: Some(0),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            AwdEventConfigPatch {
                extra_reset_penalty: Some(-1),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn locks_active_and_terminal_states() {
        for status in [
            AwdEventStatus::Deploying,
            AwdEventStatus::Prechecking,
            AwdEventStatus::Running,
            AwdEventStatus::Paused,
            AwdEventStatus::NetworkError,
            AwdEventStatus::Finished,
            AwdEventStatus::Archived,
        ] {
            assert!(assert_config_editable(&status).is_err(), "{status:?}");
        }
        assert!(assert_config_editable(&AwdEventStatus::Configuring).is_ok());
        assert!(assert_config_editable(&AwdEventStatus::Verified).is_ok());
    }
}
