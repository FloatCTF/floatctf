//! Judge service — batch creation, dispatch to JudgeServer, and callback handling.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tracing::info;
use uuid::Uuid;

use crate::entity::{
    awd_gamebox_instances, awd_judge_batches, awd_judge_tasks, awd_team_bans,
    sea_orm_active_enums::{BanStatus, GameboxStatus, JudgeTaskStatus},
};
use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::JudgeTaskStatusExt,
    repo::{event_gamebox_repo, event_repo, gamebox_repo, judge_repo},
    service::gamebox_service,
};

#[derive(Debug, Serialize, PartialEq, Eq)]
struct JudgeDispatchTask {
    id: Uuid,
    script_content: String,
    script_args_json: Option<String>,
    target_ip: String,
    timeout_secs: u64,
    callback_id: String,
}

#[derive(Debug, Serialize)]
struct JudgeDispatchBatch {
    tasks: Vec<JudgeDispatchTask>,
}

#[derive(Debug, Deserialize)]
struct JudgeDispatchResponse {
    accepted: bool,
}

#[derive(Clone, Copy)]
enum JudgeBatchStatus {
    Running,
    Completed,
}

impl JudgeBatchStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
        }
    }
}

/// Create a judge batch for a round: one task per (non-banned) team × template.
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

    // Get all EventGameBoxes（§27：Judge 目标是 EventGameBox，配置从 Revision 解析）
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
            // Find the instance for this team × EventGameBox
            let instance = instances
                .iter()
                .find(|i| i.team_id == team.id && i.event_gamebox_id == eg.id);

            let (instance_id, status) = match instance {
                Some(inst) => {
                    // Skip if resetting or not healthy
                    if inst.status == GameboxStatus::Resetting
                        || inst.status == GameboxStatus::Pending
                    {
                        (inst.id, JudgeTaskStatus::SkippedResetting)
                    } else {
                        (inst.id, JudgeTaskStatus::Pending)
                    }
                }
                None => continue, // no instance, skip
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

/// Dispatch a batch to the JudgeServer via HTTP.
pub async fn dispatch_batch(
    db: &DatabaseConnection,
    batch_id: Uuid,
    judgeserver_url: &str,
    internal_token: &str,
) -> AwdResult<()> {
    if internal_token.trim().is_empty() {
        return Err(AwdError::Validation(
            "JudgeServer internal token is empty".into(),
        ));
    }

    let batch = awd_judge_batches::Entity::find_by_id(batch_id)
        .one(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("Judge batch not found".into()))?;

    let awd_event = event_repo::find_by_event_id(db, batch.event_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .ok_or_else(|| AwdError::NotFound("AWD event not found".into()))?;

    let batch_tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::BatchId.eq(batch_id))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    let tasks = batch_tasks
        .iter()
        .filter(|task| task.status == JudgeTaskStatus::Pending)
        .cloned()
        .collect::<Vec<_>>();

    if tasks.is_empty() {
        if batch_tasks.iter().all(|task| task.status.is_terminal()) {
            set_batch_status(db, batch_id, JudgeBatchStatus::Completed).await?;
            info!("[Judge] Batch {} has completed", batch_id);
        } else {
            info!(
                "[Judge] Batch {} has no tasks available to dispatch",
                batch_id
            );
        }
        return Ok(());
    }

    let event_gamebox_ids: HashSet<Uuid> =
        tasks.iter().filter_map(|t| t.event_gamebox_id).collect();
    let instance_ids: HashSet<Uuid> = tasks.iter().map(|task| task.gamebox_instance_id).collect();

    // 每个 EventGameBox → 单一 resolver（pin Revision + Event 覆盖，§27/§49）
    let mut resolved_specs = HashMap::new();
    for eg_id in &event_gamebox_ids {
        resolved_specs.insert(
            *eg_id,
            gamebox_service::resolve_event_gamebox_spec(db, *eg_id).await?,
        );
    }

    let instances = awd_gamebox_instances::Entity::find()
        .filter(awd_gamebox_instances::Column::Id.is_in(instance_ids))
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?
        .into_iter()
        .map(|instance| (instance.id, instance))
        .collect::<HashMap<_, _>>();

    let mut dispatch_tasks = Vec::with_capacity(tasks.len());
    for task in &tasks {
        let eg_id = task
            .event_gamebox_id
            .ok_or_else(|| AwdError::NotFound("Judge task lacks event_gamebox_id".into()))?;
        let resolved = resolved_specs
            .get(&eg_id)
            .ok_or_else(|| AwdError::NotFound(format!("EventGameBox {} not found", eg_id)))?;
        let instance = instances.get(&task.gamebox_instance_id).ok_or_else(|| {
            AwdError::NotFound(format!(
                "GameBox instance {} not found",
                task.gamebox_instance_id
            ))
        })?;

        let script_content = resolved
            .gamebox
            .judge_script_content
            .as_deref()
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| {
                AwdError::Validation(format!(
                    "GameBox {} has no judge script content",
                    resolved.gamebox.name
                ))
            })?;
        let script_args_json = serialize_script_args(resolved.gamebox.judge_args_json.as_ref())?;
        let timeout_secs = resolved
            .effective_judge_timeout_secs
            .unwrap_or(awd_event.judge_default_timeout_secs);
        if timeout_secs <= 0 {
            return Err(AwdError::Validation(format!(
                "EventGameBox {} has an invalid judge timeout",
                eg_id
            )));
        }

        dispatch_tasks.push(JudgeDispatchTask {
            id: task.id,
            script_content: script_content.to_string(),
            script_args_json,
            target_ip: instance.gamebox_ip.ip().to_string(),
            timeout_secs: timeout_secs as u64,
            callback_id: task.callback_idempotency_key.clone().unwrap_or_else(|| {
                format!(
                    "judge:{}:{}:{}:{}",
                    task.event_id, task.round_id, task.team_id, task.gamebox_instance_id
                )
            }),
        });
    }

    let endpoint = judge_batch_endpoint(judgeserver_url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AwdError::Network(format!("Failed to build JudgeServer client: {e}")))?;
    let response = client
        .post(&endpoint)
        .bearer_auth(internal_token)
        .json(&JudgeDispatchBatch {
            tasks: dispatch_tasks,
        })
        .send()
        .await
        .map_err(|e| AwdError::Network(format!("JudgeServer dispatch failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AwdError::Network(format!(
            "JudgeServer rejected batch with HTTP {}: {}",
            status,
            limit_error_body(&body)
        )));
    }

    let accepted = response
        .json::<JudgeDispatchResponse>()
        .await
        .map_err(|e| AwdError::Network(format!("Invalid JudgeServer response: {e}")))?;
    if !accepted.accepted {
        return Err(AwdError::Network(
            "JudgeServer did not accept the batch".into(),
        ));
    }

    let task_ids = tasks.iter().map(|task| task.id).collect::<Vec<_>>();
    // `set(ActiveModel)` casts enum values for the `judge_task_status` column;
    // a raw `col_expr(Expr::value(..))` binds TEXT and Postgres rejects it.
    awd_judge_tasks::Entity::update_many()
        .set(awd_judge_tasks::ActiveModel {
            status: Set(JudgeTaskStatus::Running),
            started_at: Set(Some(chrono::Utc::now().fixed_offset())),
            ..Default::default()
        })
        .filter(awd_judge_tasks::Column::Id.is_in(task_ids))
        .filter(awd_judge_tasks::Column::Status.eq(JudgeTaskStatus::Pending))
        .exec(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    set_batch_status(db, batch_id, JudgeBatchStatus::Running).await?;

    info!("[Judge] Batch {} dispatched successfully", batch_id);
    Ok(())
}

fn serialize_script_args(value: Option<&serde_json::Value>) -> AwdResult<Option<String>> {
    value
        .map(|value| {
            serde_json::from_value::<Vec<String>>(value.clone()).map_err(|e| {
                AwdError::Validation(format!("Judge script args must be a string array: {e}"))
            })?;
            serde_json::to_string(value)
                .map_err(|e| AwdError::Internal(format!("Failed to serialize judge args: {e}")))
        })
        .transpose()
}

fn judge_batch_endpoint(base_url: &str) -> AwdResult<String> {
    let base_url = base_url.trim().trim_end_matches('/');
    if base_url.is_empty() {
        return Err(AwdError::Validation("JudgeServer URL is empty".into()));
    }
    if base_url.ends_with("/batch") {
        Ok(base_url.to_string())
    } else {
        Ok(format!("{base_url}/batch"))
    }
}

fn limit_error_body(body: &str) -> &str {
    let boundary = body
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| *index >= 512)
        .unwrap_or(body.len());
    &body[..boundary]
}

async fn set_batch_status(
    db: &DatabaseConnection,
    batch_id: Uuid,
    status: JudgeBatchStatus,
) -> AwdResult<()> {
    let active = awd_judge_batches::ActiveModel {
        id: Set(batch_id),
        status: Set(status.as_str().to_string()),
        ..Default::default()
    };
    active
        .update(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;
    Ok(())
}

/// Count pending tasks for a batch.
pub async fn pending_task_count(db: &DatabaseConnection, batch_id: Uuid) -> AwdResult<i64> {
    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::BatchId.eq(batch_id))
        .filter(
            awd_judge_tasks::Column::Status
                .is_in([JudgeTaskStatus::Pending, JudgeTaskStatus::Running]),
        )
        .all(db)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

    Ok(tasks.len() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judge_batch_endpoint_accepts_base_or_batch_url() {
        assert_eq!(
            judge_batch_endpoint("http://judge:8082").unwrap(),
            "http://judge:8082/batch"
        );
        assert_eq!(
            judge_batch_endpoint("http://judge:8082/batch/").unwrap(),
            "http://judge:8082/batch"
        );
        assert!(judge_batch_endpoint("  ").is_err());
    }

    #[test]
    fn script_args_must_be_a_string_array() {
        let args = serde_json::json!(["--host", "{target_ip}"]);
        assert_eq!(
            serialize_script_args(Some(&args)).unwrap(),
            Some("[\"--host\",\"{target_ip}\"]".to_string())
        );

        let invalid = serde_json::json!({"host": "{target_ip}"});
        assert!(serialize_script_args(Some(&invalid)).is_err());
    }

    #[test]
    fn error_body_limit_preserves_utf8_boundaries() {
        let body = "测".repeat(300);
        let limited = limit_error_body(&body);
        assert!(limited.len() <= 514);
        assert!(body.starts_with(limited));
    }
}
