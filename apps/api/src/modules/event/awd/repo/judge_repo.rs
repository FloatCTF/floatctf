//! 裁判任务/批次仓储 — Pull + Lease 实现（Wave 3）。

use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::{
    sea_query::LockType,
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::entity::{awd_judge_batches, awd_judge_tasks, sea_orm_active_enums::JudgeTaskStatus};
use crate::modules::event::awd::domain::JudgeTaskStatusExt;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 租约 TTL（秒）。
pub const LEASE_TTL_SECS: i64 = 120;
/// 心跳间隔（秒）。
pub const HEARTBEAT_INTERVAL_SECS: i64 = 30;

// ---------------------------------------------------------------------------
// 类型
// ---------------------------------------------------------------------------

/// 已认领的任务（返回给 worker）。
pub struct ClaimedTask {
    pub task_id: Uuid,
    pub batch_id: Uuid,
    pub event_id: Uuid,
    pub round_id: Uuid,
    pub gamebox_instance_id: Uuid,
    pub event_gamebox_id: Option<Uuid>,
    pub team_id: Uuid,
    pub attempt: i32,
    pub lease_token: String,
    pub lease_expires_at: DateTime<FixedOffset>,
    pub deadline_at: DateTime<FixedOffset>,
}

pub enum HeartbeatResult {
    Ok,
    Stale,
    NotFound,
}

pub enum SubmitResult {
    Ok,
    Idempotent,
    Stale,
    NotFound,
}

// ---------------------------------------------------------------------------
// 工具函数
// ---------------------------------------------------------------------------

pub fn generate_lease_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.r#gen();
    hex::encode(bytes)
}

pub fn hash_lease_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn now_fixed() -> DateTime<FixedOffset> {
    Utc::now().into()
}

// ---------------------------------------------------------------------------
// 租约回收
// ---------------------------------------------------------------------------

/// 回收过期租约。
/// Running 且 lease_expires_at < now：
/// - 若 attempt_count < max_attempts 且 deadline_at > now → Pending（清除 lease）
/// - 否则 → JudgeError
pub async fn reclaim_expired_leases(
    db: &(impl ConnectionTrait + Send),
    now: DateTime<FixedOffset>,
) -> Result<u64, sea_orm::DbErr> {
    let expired = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::Status.eq(JudgeTaskStatus::Running))
        .filter(awd_judge_tasks::Column::LeaseExpiresAt.is_not_null())
        .filter(awd_judge_tasks::Column::LeaseExpiresAt.lt(now))
        .all(db)
        .await?;

    let mut count = 0u64;
    for task in expired {
        let can_retry = task.attempt_count < task.max_attempts
            && task.deadline_at.with_timezone(&Utc) > now.with_timezone(&Utc);

        if can_retry {
            awd_judge_tasks::ActiveModel {
                id: Set(task.id),
                status: Set(JudgeTaskStatus::Pending),
                worker_id: Set(None),
                lease_token_hash: Set(None),
                lease_expires_at: Set(None),
                heartbeat_at: Set(None),
                claimed_at: Set(None),
                ..Default::default()
            }
            .update(db)
            .await?;
        } else {
            awd_judge_tasks::ActiveModel {
                id: Set(task.id),
                status: Set(JudgeTaskStatus::JudgeError),
                finished_at: Set(Some(now)),
                worker_id: Set(None),
                lease_token_hash: Set(None),
                lease_expires_at: Set(None),
                heartbeat_at: Set(None),
                claimed_at: Set(None),
                ..Default::default()
            }
            .update(db)
            .await?;
        }
        count += 1;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// 认领
// ---------------------------------------------------------------------------

/// 认领 Pending 任务。
/// 1. 在事务内回收过期租约
/// 2. SELECT ... FOR UPDATE SKIP LOCKED 认领 Pending 任务
/// 3. 写入 lease 元数据
pub async fn claim_tasks(
    db: &DatabaseConnection,
    event_id: Uuid,
    worker_id: &str,
    limit: usize,
    lease_ttl_secs: i64,
) -> Result<Vec<ClaimedTask>, sea_orm::DbErr> {
    let txn = db.begin().await?;
    let now: DateTime<FixedOffset> = Utc::now().into();

    // 步骤 1：回收过期租约
    reclaim_expired_leases(&txn, now).await?;

    // 步骤 2：SELECT ... FOR UPDATE SKIP LOCKED
    let rows = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::EventId.eq(event_id))
        .filter(awd_judge_tasks::Column::Status.eq(JudgeTaskStatus::Pending))
        .order_by_asc(awd_judge_tasks::Column::CreatedAt)
        .limit(limit as u64)
        .lock(LockType::Update)
        .all(&txn)
        .await?;

    let lease_expires: DateTime<FixedOffset> =
        (Utc::now() + chrono::Duration::seconds(lease_ttl_secs)).into();

    let mut tasks = Vec::with_capacity(rows.len());
    for row in rows {
        let token = generate_lease_token();
        let token_hash = hash_lease_token(&token);
        let attempt = row.attempt_count + 1;
        let started_at = if row.attempt_count == 0 {
            Some(now)
        } else {
            row.started_at
        };

        awd_judge_tasks::ActiveModel {
            id: Set(row.id),
            status: Set(JudgeTaskStatus::Running),
            worker_id: Set(Some(worker_id.to_string())),
            attempt_count: Set(attempt),
            started_at: Set(started_at),
            claimed_at: Set(Some(now)),
            heartbeat_at: Set(Some(now)),
            lease_expires_at: Set(Some(lease_expires)),
            lease_token_hash: Set(Some(token_hash)),
            ..Default::default()
        }
        .update(&txn)
        .await?;

        tasks.push(ClaimedTask {
            task_id: row.id,
            batch_id: row.batch_id,
            event_id: row.event_id,
            round_id: row.round_id,
            gamebox_instance_id: row.gamebox_instance_id,
            event_gamebox_id: row.event_gamebox_id,
            team_id: row.team_id,
            attempt,
            lease_token: token,
            lease_expires_at: lease_expires,
            deadline_at: row.deadline_at,
        });
    }

    txn.commit().await?;
    Ok(tasks)
}

// ---------------------------------------------------------------------------
// 心跳
// ---------------------------------------------------------------------------

/// 心跳续租。
/// 验证 worker_id、attempt、lease_token 后延长 lease_expires_at。
pub async fn heartbeat_task(
    db: &DatabaseConnection,
    task_id: Uuid,
    worker_id: &str,
    attempt: i32,
    lease_token: &str,
    lease_ttl_secs: i64,
    now: chrono::DateTime<Utc>,
) -> Result<HeartbeatResult, sea_orm::DbErr> {
    let Some(task) = awd_judge_tasks::Entity::find_by_id(task_id)
        .one(db)
        .await?
    else {
        return Ok(HeartbeatResult::NotFound);
    };

    if task.status != JudgeTaskStatus::Running {
        return Ok(HeartbeatResult::Stale);
    }
    if task.worker_id.as_deref() != Some(worker_id) {
        return Ok(HeartbeatResult::Stale);
    }
    if task.attempt_count != attempt {
        return Ok(HeartbeatResult::Stale);
    }
    let expected_hash = hash_lease_token(lease_token);
    if task.lease_token_hash.as_deref() != Some(&expected_hash) {
        return Ok(HeartbeatResult::Stale);
    }
    // 绝对 deadline 已过
    if task.deadline_at.with_timezone(&Utc) <= now {
        return Ok(HeartbeatResult::Stale);
    }

    let new_expiry: DateTime<FixedOffset> =
        (now + chrono::Duration::seconds(lease_ttl_secs)).into();
    // 不超过绝对 deadline
    let effective_expiry = if new_expiry > task.deadline_at {
        task.deadline_at
    } else {
        new_expiry
    };

    awd_judge_tasks::ActiveModel {
        id: Set(task_id),
        heartbeat_at: Set(Some(now.into())),
        lease_expires_at: Set(Some(effective_expiry)),
        ..Default::default()
    }
    .update(db)
    .await?;

    Ok(HeartbeatResult::Ok)
}

// ---------------------------------------------------------------------------
// 提交结果
// ---------------------------------------------------------------------------

/// 提交执行结果。
/// 验证 worker_id、attempt、lease_token 后写入结果。
pub async fn submit_result(
    db: &DatabaseConnection,
    task_id: Uuid,
    worker_id: &str,
    attempt: i32,
    lease_token: &str,
    result_id: &str,
    status: JudgeTaskStatus,
    exit_code: Option<i32>,
    stdout: Option<&str>,
    stderr: Option<&str>,
    duration_ms: Option<i32>,
    now: chrono::DateTime<Utc>,
) -> Result<SubmitResult, sea_orm::DbErr> {
    let Some(task) = awd_judge_tasks::Entity::find_by_id(task_id)
        .one(db)
        .await?
    else {
        return Ok(SubmitResult::NotFound);
    };

    // 幂等：相同 result_id 已提交
    if task.callback_idempotency_key.as_deref() == Some(result_id)
        && task.status.is_terminal()
    {
        return Ok(SubmitResult::Idempotent);
    }

    if task.status != JudgeTaskStatus::Running {
        return Ok(SubmitResult::Stale);
    }
    if task.worker_id.as_deref() != Some(worker_id) {
        return Ok(SubmitResult::Stale);
    }
    if task.attempt_count != attempt {
        return Ok(SubmitResult::Stale);
    }
    let expected_hash = hash_lease_token(lease_token);
    if task.lease_token_hash.as_deref() != Some(&expected_hash) {
        return Ok(SubmitResult::Stale);
    }
    if task.deadline_at.with_timezone(&Utc) <= now {
        return Ok(SubmitResult::Stale);
    }

    let now_ts: DateTime<FixedOffset> = now.into();
    awd_judge_tasks::ActiveModel {
        id: Set(task_id),
        status: Set(status),
        finished_at: Set(Some(now_ts)),
        exit_code: Set(exit_code),
        stdout_limited: Set(stdout.map(|s| s.to_string())),
        stderr_limited: Set(stderr.map(|s| s.to_string())),
        duration_ms: Set(duration_ms),
        callback_idempotency_key: Set(Some(result_id.to_string())),
        worker_id: Set(None),
        lease_token_hash: Set(None),
        lease_expires_at: Set(None),
        heartbeat_at: Set(None),
        claimed_at: Set(None),
        ..Default::default()
    }
    .update(db)
    .await?;

    Ok(SubmitResult::Ok)
}

// ---------------------------------------------------------------------------
// 批次 deadline
// ---------------------------------------------------------------------------

/// 批次绝对 deadline 终端化：将未完成的任务标记为 JudgeError。
pub async fn terminalize_batch_deadline(
    db: &DatabaseConnection,
    batch_id: Uuid,
    now: chrono::DateTime<Utc>,
) -> Result<u64, sea_orm::DbErr> {
    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::BatchId.eq(batch_id))
        .filter(
            awd_judge_tasks::Column::Status
                .is_in([JudgeTaskStatus::Pending, JudgeTaskStatus::Running]),
        )
        .all(db)
        .await?;

    let now_ts: DateTime<FixedOffset> = now.into();
    let mut count = 0u64;
    for task in tasks {
        if task.deadline_at.with_timezone(&Utc) <= now {
            awd_judge_tasks::ActiveModel {
                id: Set(task.id),
                status: Set(JudgeTaskStatus::JudgeError),
                finished_at: Set(Some(now_ts)),
                worker_id: Set(None),
                lease_token_hash: Set(None),
                lease_expires_at: Set(None),
                heartbeat_at: Set(None),
                claimed_at: Set(None),
                ..Default::default()
            }
            .update(db)
            .await?;
            count += 1;
        }
    }
    Ok(count)
}

/// 检查批次是否完成。若所有任务均为终态，更新 batch status。
pub async fn maybe_complete_batch(
    db: &DatabaseConnection,
    batch_id: Uuid,
) -> Result<bool, sea_orm::DbErr> {
    let tasks = awd_judge_tasks::Entity::find()
        .filter(awd_judge_tasks::Column::BatchId.eq(batch_id))
        .all(db)
        .await?;

    let all_terminal = tasks.iter().all(|t| t.status.is_terminal());
    if all_terminal {
        let completed = tasks
            .iter()
            .filter(|t| matches!(t.status, JudgeTaskStatus::Up | JudgeTaskStatus::Down))
            .count() as i32;
        let failed = tasks.len() as i32 - completed;

        awd_judge_batches::ActiveModel {
            id: Set(batch_id),
            status: Set("completed".to_string()),
            completed_tasks: Set(completed),
            failed_tasks: Set(failed),
            ..Default::default()
        }
        .update(db)
        .await?;
    }
    Ok(all_terminal)
}

/// 全局 deadline 清理：将所有过期的非终态任务标记为 JudgeError。
pub async fn terminalize_past_deadline(
    db: &DatabaseConnection,
    now: chrono::DateTime<Utc>,
) -> Result<u64, sea_orm::DbErr> {
    let now_ts: DateTime<FixedOffset> = now.into();
    let tasks = awd_judge_tasks::Entity::find()
        .filter(
            awd_judge_tasks::Column::Status
                .is_in([JudgeTaskStatus::Pending, JudgeTaskStatus::Running]),
        )
        .filter(awd_judge_tasks::Column::DeadlineAt.lte(now_ts))
        .all(db)
        .await?;

    let mut count = 0u64;
    for task in tasks {
        awd_judge_tasks::ActiveModel {
            id: Set(task.id),
            status: Set(JudgeTaskStatus::JudgeError),
            finished_at: Set(Some(now_ts)),
            worker_id: Set(None),
            lease_token_hash: Set(None),
            lease_expires_at: Set(None),
            heartbeat_at: Set(None),
            claimed_at: Set(None),
            ..Default::default()
        }
        .update(db)
        .await?;
        count += 1;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// 保留的原有函数
// ---------------------------------------------------------------------------

/// 创建裁判批次。
pub async fn create_batch(
    db: &DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
    total_tasks: i32,
) -> Result<awd_judge_batches::Model, sea_orm::DbErr> {
    let model = awd_judge_batches::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round_id),
        total_tasks: Set(total_tasks),
        ..Default::default()
    };
    model.insert(db).await
}

pub async fn find_task_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<awd_judge_tasks::Model>, sea_orm::DbErr> {
    awd_judge_tasks::Entity::find_by_id(id).one(db).await
}

pub async fn update_task_status(
    db: &DatabaseConnection,
    id: Uuid,
    status: JudgeTaskStatus,
    exit_code: Option<i32>,
    stdout_limited: Option<&str>,
    stderr_limited: Option<&str>,
    duration_ms: Option<i32>,
) -> Result<(), sea_orm::DbErr> {
    let mut active: awd_judge_tasks::ActiveModel = awd_judge_tasks::ActiveModel {
        id: Set(id),
        status: Set(status),
        exit_code: Set(exit_code),
        stdout_limited: Set(stdout_limited.map(|s| s.to_string())),
        stderr_limited: Set(stderr_limited.map(|s| s.to_string())),
        duration_ms: Set(duration_ms),
        finished_at: Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    };
    active.update(db).await?;
    Ok(())
}