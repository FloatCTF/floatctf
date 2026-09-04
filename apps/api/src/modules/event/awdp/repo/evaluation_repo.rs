//! awdp_evaluations 仓储（run 作用域）+ Pull/Lease worker 元数据。
//!
//! Lease 模型（plan §12-§18）：
//! - `claim_jobs`：事务内先回收 stale running（lease 到期 → pending，超 max_attempts → PLATFORM_ERROR），
//!   再 SKIP LOCKED 领取 pending，写入 lease 元数据并返回明文 token（仅 worker 内存）。
//! - `heartbeat`：验证 token/worker/状态后延长 lease。
//! - `finish_with_lease`：验证 lease token + attempt + worker 后写终态；stale 结果拒绝。
//! - token 明文不落库（只存 sha256），不落日志。

use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    DatabaseTransaction, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait, sea_query::LockType,
};
use uuid::Uuid;

use crate::entity::{
    awdp_evaluations, awdp_instances, event_instances,
    sea_orm_active_enums::{AwdpEvaluationKind, AwdpEvaluationStatus},
};
use crate::modules::event::awdp::{AwdpError, AwdpResult};

/// 领取的作业（含明文 lease token，仅在 worker 内存；不落库不落日志）。
#[derive(Debug, Clone)]
pub struct ClaimedJob {
    pub evaluation: awdp_evaluations::Model,
    pub lease_token: String,
    /// 本次领取对应的 attempt 序号（= attempt_count 递增后的值）。
    pub attempt: i32,
}

/// 心跳结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatOutcome {
    /// lease 有效并已延长。
    Ok,
    /// 无有效 lease（token/worker/状态不匹配，或评估已终态）。
    NoLease,
}

/// 写终态结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishOutcome {
    /// 结果已写入。
    Ok,
    /// stale worker 的晚结果（lease 无效 / attempt 不匹配）——拒绝，不覆盖新 attempt。
    StaleRejected,
}

/// 生成 32 字节随机 lease token（hex 编码）。
fn new_lease_token() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

/// lease token 的 sha256 哈希（落库用）。
fn lease_token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

/// 创建 official 评估（每 round × instance 唯一；冲突视为已存在）。
pub async fn create_official(
    db: &DatabaseConnection,
    run_id: Uuid,
    instance_id: Uuid,
    fix_round_id: Uuid,
) -> AwdpResult<awdp_evaluations::Model> {
    let now = Utc::now().into();
    let model = awdp_evaluations::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run_id),
        instance_id: Set(instance_id),
        fix_round_id: Set(Some(fix_round_id)),
        kind: Set(AwdpEvaluationKind::Official),
        status: Set(AwdpEvaluationStatus::Pending),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    match model.insert(db).await {
        Ok(m) => Ok(m),
        // INSERT..RETURNING 的冲突按 DbErr::Query 上报（非 Exec）——变体无关匹配。
        Err(e) if e.to_string().contains("awdp_evaluations_official_uidx") => {
            awdp_evaluations::Entity::find()
                .filter(awdp_evaluations::Column::FixRoundId.eq(fix_round_id))
                .filter(awdp_evaluations::Column::InstanceId.eq(instance_id))
                .one(db)
                .await
                .map_err(|e| AwdpError::Database(e.to_string()))?
                .ok_or_else(|| {
                    AwdpError::Internal("official evaluation missing after conflict".into())
                })
        }
        Err(e) => Err(AwdpError::Database(e.to_string())),
    }
}

/// 创建 manual 评估（healthcheck + judge，不计分）。
pub async fn create_manual(
    db: &DatabaseConnection,
    run_id: Uuid,
    instance_id: Uuid,
) -> AwdpResult<awdp_evaluations::Model> {
    let now = Utc::now().into();
    awdp_evaluations::ActiveModel {
        id: Set(Uuid::new_v4()),
        run_id: Set(run_id),
        instance_id: Set(instance_id),
        fix_round_id: Set(None),
        kind: Set(AwdpEvaluationKind::Manual),
        status: Set(AwdpEvaluationStatus::Pending),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|e| AwdpError::Database(e.to_string()))
}

// ────────────────────────────────────────────────────────────────────────────
// Pull + Lease
// ────────────────────────────────────────────────────────────────────────────

/// 回收 stale running：lease 已到期（或旧数据 lease 为 NULL）的 running 行。
///
/// - `attempt_count < max_attempts` → 重置 pending（清 lease 元数据）等待重领；
/// - `attempt_count >= max_attempts` → 终态 PLATFORM_ERROR（不允许永久 running）。
///
/// 返回 (reclaimed_to_pending, terminal_platform_error)。
async fn recover_stale_running<C>(
    db: &C,
    max_attempts: i32,
    now: DateTime<Utc>,
) -> AwdpResult<(usize, usize)>
where
    C: ConnectionTrait,
{
    use sea_orm::sea_query::Condition;
    let stale = awdp_evaluations::Entity::find()
        .filter(awdp_evaluations::Column::Status.eq(AwdpEvaluationStatus::Running))
        .filter(
            Condition::any()
                .add(awdp_evaluations::Column::LeaseExpiresAt.lt(now))
                // 旧模型置 running 的行 lease 为 NULL（无心跳）→ 一律视为 stale。
                .add(awdp_evaluations::Column::LeaseExpiresAt.is_null()),
        )
        .lock(LockType::Update)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    let mut reclaimed = 0usize;
    let mut terminal = 0usize;
    for row in stale {
        let mut am: awdp_evaluations::ActiveModel = row.clone().into();
        if row.attempt_count >= max_attempts {
            am.status = Set(AwdpEvaluationStatus::PlatformError);
            am.finished_at = Set(Some(now.into()));
            terminal += 1;
        } else {
            am.status = Set(AwdpEvaluationStatus::Pending);
            reclaimed += 1;
        }
        am.claimed_by = Set(None);
        am.claimed_at = Set(None);
        am.heartbeat_at = Set(None);
        am.lease_expires_at = Set(None);
        am.lease_token_hash = Set(None);
        am.updated_at = Set(now.into());
        am.update(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
    }
    Ok((reclaimed, terminal))
}

/// claim pending 评估（Pull + Lease）：
///
/// 单事务内：
///   1. 先回收 stale running（见 `recover_stale_running`）；
///   2. `SELECT ... WHERE status='pending' [AND kind IN kinds] FOR UPDATE SKIP LOCKED LIMIT capacity`；
///   3. 每条：status=running、attempt_count+=1、写入 lease 元数据、返回明文 token。
///
/// `kinds` 为空 = 不限制（JudgeServer 同时消费 manual + official）；
/// 平台侧进程内 worker 只传 official（manual 由 Test Check 同步流程独占）。
/// `event_id` 非 None = 仅领取该赛事（awdp_runs.event_id 过滤）——赛事专属 JudgeServer
/// 只能访问本赛事 data 网络，绝不能领到别的赛事的 job（target_ip 不可达会误判 service_down）；
/// None = 不限制（平台进程内 worker，宿主可达所有赛事网络）。
#[allow(clippy::too_many_arguments)]
pub async fn claim_jobs(
    db: &DatabaseConnection,
    worker_id: &str,
    capacity: u64,
    lease_duration_secs: i64,
    max_attempts: i32,
    kinds: &[AwdpEvaluationKind],
    event_id: Option<Uuid>,
) -> AwdpResult<Vec<ClaimedJob>> {
    let txn: DatabaseTransaction = db
        .begin()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let now = Utc::now();

    recover_stale_running(&txn, max_attempts, now).await?;

    let mut query = awdp_evaluations::Entity::find()
        .filter(awdp_evaluations::Column::Status.eq(AwdpEvaluationStatus::Pending))
        .order_by_asc(awdp_evaluations::Column::CreatedAt)
        .limit(capacity)
        .lock_with_behavior(
            sea_orm::sea_query::LockType::Update,
            sea_orm::sea_query::LockBehavior::SkipLocked,
        );
    if !kinds.is_empty() {
        query = query.filter(awdp_evaluations::Column::Kind.is_in(kinds.iter().cloned()));
    }
    if let Some(event_id) = event_id {
        // 赛事专属 worker：只领本赛事 job（evaluations.run_id → awdp_runs.event_id）。
        query = query
            .inner_join(crate::entity::awdp_runs::Entity)
            .filter(crate::entity::awdp_runs::Column::EventId.eq(event_id));
    }
    let rows = query
        .all(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    let mut jobs = Vec::with_capacity(rows.len());
    for row in rows {
        let token = new_lease_token();
        let attempt = row.attempt_count + 1;
        let mut am: awdp_evaluations::ActiveModel = row.clone().into();
        am.status = Set(AwdpEvaluationStatus::Running);
        am.started_at = Set(Some(now.into()));
        am.attempt_count = Set(attempt);
        am.claimed_by = Set(Some(worker_id.to_string()));
        am.claimed_at = Set(Some(now.into()));
        am.heartbeat_at = Set(Some(now.into()));
        am.lease_expires_at = Set(Some((now + Duration::seconds(lease_duration_secs)).into()));
        am.lease_token_hash = Set(Some(lease_token_hash(&token)));
        am.updated_at = Set(now.into());
        am.update(&txn)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        jobs.push(ClaimedJob {
            evaluation: row,
            lease_token: token,
            attempt,
        });
    }

    txn.commit()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(jobs)
}

/// 心跳：验证 lease 后延长（heartbeat_at = now，lease_expires_at = now + duration）。
pub async fn heartbeat(
    db: &DatabaseConnection,
    evaluation_id: Uuid,
    worker_id: &str,
    lease_token: &str,
    lease_duration_secs: i64,
) -> AwdpResult<HeartbeatOutcome> {
    let txn = db
        .begin()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let row = awdp_evaluations::Entity::find_by_id(evaluation_id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let Some(row) = row else {
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Ok(HeartbeatOutcome::NoLease);
    };

    let now = Utc::now();
    let valid = row.status == AwdpEvaluationStatus::Running
        && row.claimed_by.as_deref() == Some(worker_id)
        && row.lease_token_hash.as_deref() == Some(lease_token_hash(lease_token).as_str())
        && row
            .lease_expires_at
            .map(|t| t.with_timezone(&Utc) > now)
            .unwrap_or(false);
    if !valid {
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Ok(HeartbeatOutcome::NoLease);
    }

    let mut am: awdp_evaluations::ActiveModel = row.into();
    am.heartbeat_at = Set(Some(now.into()));
    am.lease_expires_at = Set(Some((now + Duration::seconds(lease_duration_secs)).into()));
    am.updated_at = Set(now.into());
    am.update(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    txn.commit()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(HeartbeatOutcome::Ok)
}

/// 写终态（lease 校验版）：验证 worker/token/attempt 后原子写入。
///
/// stale worker 的晚结果（attempt 不匹配 / token 无效 / 非 running / lease 已失效）
/// 返回 `FinishOutcome::StaleRejected`——绝不覆盖新 attempt 的评估结果。
#[allow(clippy::too_many_arguments)]
pub async fn finish_with_lease(
    db: &DatabaseConnection,
    evaluation_id: Uuid,
    worker_id: &str,
    lease_token: &str,
    attempt: i32,
    status: AwdpEvaluationStatus,
    healthcheck_result: Option<&str>,
    judge_result: Option<&str>,
    exploit_result: Option<&str>,
    stdout_limited: Option<&str>,
    stderr_limited: Option<&str>,
) -> AwdpResult<FinishOutcome> {
    let txn = db
        .begin()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let row = awdp_evaluations::Entity::find_by_id(evaluation_id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let Some(row) = row else {
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Err(AwdpError::NotFound("evaluation not found".into()));
    };

    let now = Utc::now();
    let valid = row.status == AwdpEvaluationStatus::Running
        && row.claimed_by.as_deref() == Some(worker_id)
        && row.lease_token_hash.as_deref() == Some(lease_token_hash(lease_token).as_str())
        && row.attempt_count == attempt
        && row
            .lease_expires_at
            .map(|t| t.with_timezone(&Utc) >= now)
            .unwrap_or(false);
    if !valid {
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Ok(FinishOutcome::StaleRejected);
    }

    let mut am: awdp_evaluations::ActiveModel = row.into();
    am.status = Set(status);
    if let Some(v) = healthcheck_result {
        am.healthcheck_result = Set(Some(v.to_string()));
    }
    if let Some(v) = judge_result {
        am.judge_result = Set(Some(v.to_string()));
    }
    if let Some(v) = exploit_result {
        am.exploit_result = Set(Some(v.to_string()));
    }
    if let Some(v) = stdout_limited {
        am.stdout_limited = Set(Some(v.to_string()));
    }
    if let Some(v) = stderr_limited {
        am.stderr_limited = Set(Some(v.to_string()));
    }
    am.finished_at = Set(Some(now.into()));
    // 终态释放 lease。
    am.claimed_by = Set(None);
    am.claimed_at = Set(None);
    am.heartbeat_at = Set(None);
    am.lease_expires_at = Set(None);
    am.lease_token_hash = Set(None);
    am.updated_at = Set(now.into());
    am.update(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    txn.commit()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(FinishOutcome::Ok)
}

/// 基础设施失败（脚本 spawn 失败/超时/输出畸形/runner 错误）的处理：
/// 不能判玩家失败。`attempt < max_attempts` → 释放回 pending 允许重领重试
/// （attempt_count 保留，下次 claim +1）；`attempt >= max_attempts` → 终态
/// PLATFORM_ERROR（不允许永久 running / 无限重试）。
///
/// 返回 `FinishOutcome::Ok` 表示已处理（重试或终态）；`StaleRejected` 表示
/// lease 已失效（另一个 worker 已接管），本 worker 的结果被忽略。
pub async fn release_or_fail(
    db: &DatabaseConnection,
    evaluation_id: Uuid,
    worker_id: &str,
    lease_token: &str,
    attempt: i32,
    max_attempts: i32,
    detail: &str,
) -> AwdpResult<FinishOutcome> {
    let txn = db
        .begin()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let row = awdp_evaluations::Entity::find_by_id(evaluation_id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let Some(row) = row else {
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Err(AwdpError::NotFound("evaluation not found".into()));
    };

    let now = Utc::now();
    let valid = row.status == AwdpEvaluationStatus::Running
        && row.claimed_by.as_deref() == Some(worker_id)
        && row.lease_token_hash.as_deref() == Some(lease_token_hash(lease_token).as_str())
        && row.attempt_count == attempt;
    if !valid {
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Ok(FinishOutcome::StaleRejected);
    }

    let mut am: awdp_evaluations::ActiveModel = row.into();
    am.claimed_by = Set(None);
    am.claimed_at = Set(None);
    am.heartbeat_at = Set(None);
    am.lease_expires_at = Set(None);
    am.lease_token_hash = Set(None);
    am.updated_at = Set(now.into());
    if attempt >= max_attempts {
        am.status = Set(AwdpEvaluationStatus::PlatformError);
        am.finished_at = Set(Some(now.into()));
    } else {
        // 释放回 pending 重试（attempt_count 保留；下次 claim 再 +1）。
        am.status = Set(AwdpEvaluationStatus::Pending);
        am.stdout_limited = Set(Some(truncate_str(detail, 64 * 1024)));
    }
    am.update(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    txn.commit()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(FinishOutcome::Ok)
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…({} bytes)", &s[..max], s.len())
    }
}

/// 写入终态（无 lease 校验版）——仅用于非 claim 的同步流程（manual Test Check）。
///
/// 终态必须释放 lease 元数据（`awdp_evaluations_lease_consistency_check` 要求终态行
/// lease_token_hash 为 NULL）。防御性清空：即使该行曾被 worker 误 claim（历史竞态），
/// 同步路径写终态也不会触发约束违例。
pub async fn finish(
    db: &DatabaseConnection,
    evaluation_id: Uuid,
    status: AwdpEvaluationStatus,
    healthcheck_result: Option<&str>,
    judge_result: Option<&str>,
    exploit_result: Option<&str>,
    stdout_limited: Option<&str>,
    stderr_limited: Option<&str>,
) -> AwdpResult<()> {
    let now = Utc::now().into();
    let mut am: awdp_evaluations::ActiveModel = awdp_evaluations::Entity::find_by_id(evaluation_id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("evaluation not found".into()))?
        .into();
    am.status = Set(status);
    if let Some(v) = healthcheck_result {
        am.healthcheck_result = Set(Some(v.to_string()));
    }
    if let Some(v) = judge_result {
        am.judge_result = Set(Some(v.to_string()));
    }
    if let Some(v) = exploit_result {
        am.exploit_result = Set(Some(v.to_string()));
    }
    if let Some(v) = stdout_limited {
        am.stdout_limited = Set(Some(v.to_string()));
    }
    if let Some(v) = stderr_limited {
        am.stderr_limited = Set(Some(v.to_string()));
    }
    am.finished_at = Set(Some(now));
    // 终态释放 lease（约束：终态行不得持有 lease）。
    am.claimed_by = Set(None);
    am.claimed_at = Set(None);
    am.heartbeat_at = Set(None);
    am.lease_expires_at = Set(None);
    am.lease_token_hash = Set(None);
    am.updated_at = Set(now);
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

/// 按 id 取评估（worker / ALL Check 重取终态用）。
pub async fn find_by_id(db: &DatabaseConnection, id: Uuid) -> AwdpResult<awdp_evaluations::Model> {
    awdp_evaluations::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("evaluation not found".into()))
}

/// run 的全部评估（选手视角过滤在 service 层）。
pub async fn list_for_run(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Vec<awdp_evaluations::Model>> {
    awdp_evaluations::Entity::find()
        .filter(awdp_evaluations::Column::RunId.eq(run_id))
        .order_by_desc(awdp_evaluations::Column::CreatedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// run 下全部带 instance 的评估（管理端视图）。
pub async fn list_for_run_with_instances(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<
    Vec<(
        awdp_evaluations::Model,
        awdp_instances::Model,
        event_instances::Model,
    )>,
> {
    let evals = list_for_run(db, run_id).await?;
    let mut out = Vec::with_capacity(evals.len());
    for ev in evals {
        let ext = awdp_instances::Entity::find_by_id(ev.instance_id)
            .one(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            .ok_or_else(|| AwdpError::Internal("awdp instance missing".into()))?;
        let inst = event_instances::Entity::find_by_id(ev.instance_id)
            .one(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            .ok_or_else(|| AwdpError::Internal("instance missing".into()))?;
        out.push((ev, ext, inst));
    }
    Ok(out)
}

/// 该实例在指定 round 是否存在未完成的评估（pending/running）。
pub async fn has_unfinished_for_instance(
    db: &DatabaseConnection,
    instance_id: Uuid,
    fix_round_id: Uuid,
) -> AwdpResult<bool> {
    use sea_orm::sea_query::Condition;
    let count = awdp_evaluations::Entity::find()
        .filter(awdp_evaluations::Column::InstanceId.eq(instance_id))
        .filter(awdp_evaluations::Column::FixRoundId.eq(fix_round_id))
        .filter(
            Condition::any()
                .add(awdp_evaluations::Column::Status.eq(AwdpEvaluationStatus::Pending))
                .add(awdp_evaluations::Column::Status.eq(AwdpEvaluationStatus::Running)),
        )
        .count(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(count > 0)
}
