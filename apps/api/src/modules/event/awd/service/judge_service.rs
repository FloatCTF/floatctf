//! AWD 裁判批次创建服务（Wave 3 — Pull Judge）。
//!
//! 批次创建保留；Push 分发已移除（JudgeServer 通过 Pull + Lease 认领任务）。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use tracing::info;
use uuid::Uuid;

use crate::entity::{
    awd_judge_batches, awd_judge_tasks, awd_team_bans, event_gamebox_instances,
    sea_orm_active_enums::{BanStatus, GameboxStatus, JudgeTaskStatus},
};
use crate::modules::event::awd::{
    AwdError, AwdResult,
    domain::JudgeTaskStatusExt,
    repo::{event_gamebox_repo, event_repo, gamebox_repo, judge_repo},
};

/// 为某一轮创建裁判批次：每个（未封禁）战队 × 模板一条任务。
pub async fn create_batch(
    db: &DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
) -> AwdResult<Uuid> {
    // Get event for timeout config
    let awd_event = event_repo::find_by_event_id(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    // Get all EventGameBoxes
    let event_gameboxes = event_gamebox_repo::find_event_gameboxes_by_event(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    if event_gameboxes.is_empty() {
        return Err(AwdError::Validation(
            "No gameboxes configured for event".into(),
        ));
    }

    // Get all teams
    use crate::entity::event_teams;
    let teams = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // Get banned team IDs
    let bans = awd_team_bans::Entity::find()
        .filter(awd_team_bans::Column::EventId.eq(event_id))
        .filter(awd_team_bans::Column::Status.eq(BanStatus::Active))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    let banned_team_ids: std::collections::HashSet<Uuid> =
        bans.into_iter().map(|b| b.team_id).collect();

    // Get all instances
    let instances = gamebox_repo::find_instances_by_event(db, event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // Create batch
    let task_count = event_gameboxes.len() * teams.len();
    let batch = judge_repo::create_batch(db, event_id, round_id, task_count as i32)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    // Create individual tasks
    let timeout = awd_event.judge_default_timeout_secs as u64;
    let deadline = chrono::Utc::now()
        + chrono::Duration::seconds(timeout as i64 + awd_event.judge_grace_period_secs as i64);

    for team in &teams {
        // Skip banned teams
        if banned_team_ids.contains(&team.id) {
            continue;
        }

        for eg in &event_gameboxes {
            let instance = instances
                .iter()
                .find(|(ext, _)| ext.team_id == team.id && ext.event_gamebox_id == eg.id);

            let (instance_id, status) = match instance {
                Some((inst, _root)) => {
                    if inst.status == GameboxStatus::Resetting
                        || inst.status == GameboxStatus::Pending
                    {
                        (inst.id, JudgeTaskStatus::SkippedResetting)
                    } else {
                        (inst.id, JudgeTaskStatus::Pending)
                    }
                }
                None => continue,
            };

            let task = awd_judge_tasks::ActiveModel {
                id: Set(Uuid::new_v4()),
                batch_id: Set(batch.id),
                event_id: Set(event_id),
                round_id: Set(round_id),
                gamebox_instance_id: Set(instance_id),
                event_gamebox_id: Set(Some(eg.id)),
                team_id: Set(team.id),
                status: Set(status),
                max_attempts: Set(2),
                deadline_at: Set(deadline.into()),
                callback_idempotency_key: Set(Some(format!(
                    "judge:{}:{}:{}:{}",
                    event_id, round_id, team.id, instance_id
                ))),
                ..Default::default()
            };

            task.insert(db)
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
        }
    }

    info!(
        "[Judge] Batch {} created for round {} ({} tasks)",
        batch.id, round_id, task_count
    );

    Ok(batch.id)
}
