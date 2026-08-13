//! awdp_fix_rounds 仓储（确定性回合时间线；run 作用域）。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
    sea_query::LockType,
};
use uuid::Uuid;

use crate::entity::awdp_fix_rounds;
use crate::modules::event::awdp::{AwdpError, AwdpResult, domain::round_windows, repo::run_repo};

/// Fix 开始时预生成全部回合（幂等：已存在的 sequence 跳过；时间线来自 run snapshot）。
pub async fn materialize_rounds(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Vec<awdp_fix_rounds::Model>> {
    let run = run_repo::require_by_id(db, run_id).await?;
    let fix_start = run
        .fix_started_at
        .ok_or_else(|| AwdpError::InvalidState("fix_started_at is not set".into()))?
        .with_timezone(&Utc);
    let count = run.fix_duration_secs / run.fix_round_interval_secs;
    let windows = round_windows(
        fix_start,
        chrono::Duration::seconds(run.fix_round_interval_secs as i64),
        count,
    );

    let mut created = Vec::new();
    for w in windows {
        let exists = awdp_fix_rounds::Entity::find()
            .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
            .filter(awdp_fix_rounds::Column::Sequence.eq(w.sequence))
            .count(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            > 0;
        if exists {
            continue;
        }
        let now = Utc::now().into();
        let model = awdp_fix_rounds::ActiveModel {
            id: Set(Uuid::new_v4()),
            run_id: Set(run_id),
            sequence: Set(w.sequence),
            starts_at: Set(w.starts_at.into()),
            cutoff_at: Set(w.cutoff_at.into()),
            status: Set("pending".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
        created.push(model);
    }
    Ok(created)
}

/// 当前进行中（starts_at <= now < cutoff_at）或最近到期的回合。
pub async fn current_open_round(
    db: &DatabaseConnection,
    run_id: Uuid,
    now: DateTime<Utc>,
) -> AwdpResult<Option<awdp_fix_rounds::Model>> {
    let rows = awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .filter(awdp_fix_rounds::Column::StartsAt.lte(now))
        .filter(awdp_fix_rounds::Column::CutoffAt.gt(now))
        .order_by_asc(awdp_fix_rounds::Column::Sequence)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(rows.into_iter().next())
}

/// 下一个尚未 cut 的回合（含已过 starts_at 的，用于 tick 到期判定）。
pub async fn next_due_round(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Option<awdp_fix_rounds::Model>> {
    let row = awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .filter(awdp_fix_rounds::Column::Status.ne("completed"))
        .order_by_asc(awdp_fix_rounds::Column::Sequence)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(row)
}

pub async fn find_by_sequence(
    db: &DatabaseConnection,
    run_id: Uuid,
    sequence: i32,
) -> AwdpResult<Option<awdp_fix_rounds::Model>> {
    awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .filter(awdp_fix_rounds::Column::Sequence.eq(sequence))
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

pub async fn find_by_id(db: &DatabaseConnection, id: Uuid) -> AwdpResult<awdp_fix_rounds::Model> {
    awdp_fix_rounds::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .ok_or_else(|| AwdpError::NotFound("awdp fix round not found".into()))
}

pub async fn list_for_run(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Vec<awdp_fix_rounds::Model>> {
    awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .order_by_asc(awdp_fix_rounds::Column::Sequence)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 撤销 fix 会话：删除 run 全部 fix_rounds（级联删除 evaluations + fix 计分账本；
/// patch_submissions.fix_round_id 置 NULL 保留审计）。返回删除行数。
pub async fn clear_fix_session<C: sea_orm::ConnectionTrait>(
    db: &C,
    run_id: Uuid,
) -> AwdpResult<usize> {
    let res = awdp_fix_rounds::Entity::delete_many()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .exec(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(res.rows_affected as usize)
}

/// 标记回合状态（evaluating / completed）。
pub async fn set_status(db: &DatabaseConnection, round_id: Uuid, status: &str) -> AwdpResult<()> {
    let now = Utc::now().into();
    let mut am: awdp_fix_rounds::ActiveModel = find_by_id(db, round_id).await?.into();
    am.status = Set(status.to_string());
    if status == "evaluating" && am.started_at.is_not_set() {
        am.started_at = Set(Some(now));
    }
    if status == "completed" {
        am.finished_at = Set(Some(now));
    }
    am.updated_at = Set(now);
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

/// 原子物化回合评估（crash-safe，plan §39/§40）：
///
/// 单事务内：
///   1. lock round 行（FOR UPDATE）；
///   2. CAS：status 必须仍为 `pending`（并发/重复 tick 直接跳过）；
///   3. 为 run 下全部已启动实例（runtime_state != 'pending'）物化 official 评估
///      （round × instance 唯一；先查后插，唯一冲突时整体重试一次）；
///   4. 记录 expected_eval_count（本轮评估数快照）+ status='evaluating'。
///
/// 杜绝「round 已 evaluating 但评估未物化 → 0 条评估即 completed」的 crash 窗口。
/// 返回物化（预期）评估数。
pub async fn materialize_round_atomic(
    db: &DatabaseConnection,
    run_id: Uuid,
    round_id: Uuid,
    now: DateTime<Utc>,
) -> AwdpResult<usize> {
    // 唯一冲突时整体重试一次（见下方 Err 分支）；第二次必看到既有行。
    for retried in [false, true] {
        match materialize_round_atomic_inner(db, run_id, round_id, now, retried).await {
            // 需要重试：继续循环。
            Err(AwdpError::Retry) => continue,
            other => return other,
        }
    }
    Err(AwdpError::Internal(
        "round materialization retry exhausted".into(),
    ))
}

async fn materialize_round_atomic_inner(
    db: &DatabaseConnection,
    run_id: Uuid,
    round_id: Uuid,
    now: DateTime<Utc>,
    retried: bool,
) -> AwdpResult<usize> {
    use crate::entity::sea_orm_active_enums::{AwdpEvaluationKind, AwdpEvaluationStatus};
    use crate::entity::{awdp_evaluations, awdp_instances, event_instances};

    let txn = db
        .begin()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    let current = awdp_fix_rounds::Entity::find_by_id(round_id)
        .lock(LockType::Update)
        .one(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let Some(current) = current else {
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Err(AwdpError::NotFound("awdp fix round not found".into()));
    };
    if current.status != "pending" {
        // CAS：已被并发 tick / 已完成 → 幂等跳过。
        txn.rollback()
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
        return Ok(0);
    }

    // 物化：run 下全部已启动实例（未启动 = 未参赛，不产生评估）。
    let exts = awdp_instances::Entity::find()
        .filter(awdp_instances::Column::RunId.eq(run_id))
        .all(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let mut expected = 0usize;
    for ext in exts {
        let Some(instance) = event_instances::Entity::find_by_id(ext.instance_id)
            .one(&txn)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
        else {
            continue;
        };
        if instance.runtime_state == "pending" {
            continue;
        }
        // 先查后插：round 行锁串行化同一 round 的并发物化；与其它写者
        // 并发窗口通过「唯一冲突 → 整体重试一次」收敛（第二次必看到既有行）。
        let exists = awdp_evaluations::Entity::find()
            .filter(awdp_evaluations::Column::FixRoundId.eq(round_id))
            .filter(awdp_evaluations::Column::InstanceId.eq(instance.id))
            .count(&txn)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            > 0;
        if exists {
            continue;
        }
        let insert = awdp_evaluations::ActiveModel {
            id: Set(Uuid::new_v4()),
            run_id: Set(run_id),
            instance_id: Set(instance.id),
            fix_round_id: Set(Some(round_id)),
            kind: Set(AwdpEvaluationKind::Official),
            status: Set(AwdpEvaluationStatus::Pending),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
            ..Default::default()
        }
        .insert(&txn)
        .await;
        match insert {
            Ok(_) => expected += 1,
            Err(e) if e.to_string().contains("awdp_evaluations_official_uidx") && !retried => {
                // 并发窗口：另一写者已插入同一 (round, instance)。回滚后整体重试一次，
                // 重试时先查后插必跳过既有行（round 锁 + 已存在行保证无第二次冲突）。
                txn.rollback()
                    .await
                    .map_err(|x| AwdpError::Database(x.to_string()))?;
                return Err(AwdpError::Retry);
            }
            Err(e) => {
                txn.rollback()
                    .await
                    .map_err(|x| AwdpError::Database(x.to_string()))?;
                return Err(AwdpError::Database(e.to_string()));
            }
        }
    }

    let mut am: awdp_fix_rounds::ActiveModel = current.into();
    am.status = Set("evaluating".to_string());
    am.started_at = Set(Some(now.into()));
    am.expected_eval_count = Set(Some(expected as i32));
    am.updated_at = Set(now.into());
    am.update(&txn)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    txn.commit()
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(expected)
}

/// 下一个 status=pending 且 cutoff <= now 的回合（tick 物化用）。
pub async fn next_pending_due_round(
    db: &DatabaseConnection,
    run_id: Uuid,
    now: DateTime<Utc>,
) -> AwdpResult<Option<awdp_fix_rounds::Model>> {
    let row = awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .filter(awdp_fix_rounds::Column::Status.eq("pending"))
        .filter(awdp_fix_rounds::Column::CutoffAt.lte(now))
        .order_by_asc(awdp_fix_rounds::Column::Sequence)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(row)
}

/// 下一个 status=pending 的回合（推进 next_action_at 用）。
pub async fn next_pending_round(
    db: &DatabaseConnection,
    run_id: Uuid,
) -> AwdpResult<Option<awdp_fix_rounds::Model>> {
    let row = awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::RunId.eq(run_id))
        .filter(awdp_fix_rounds::Column::Status.eq("pending"))
        .order_by_asc(awdp_fix_rounds::Column::Sequence)
        .one(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(row)
}

/// 全部 evaluating 回合 → 满足完成条件的标记 completed（幂等兜底）。
///
/// 完成条件（plan §40，杜绝 0 条评估假完成）：
///   - expected_eval_count 快照存在且 > 0：actual_count == expected_eval_count 且全部终态；
///   - 旧数据（快照 NULL）回退：actual_count > 0 且全部终态。
pub async fn complete_finished_rounds(db: &DatabaseConnection) -> AwdpResult<usize> {
    use crate::entity::awdp_evaluations;
    use crate::entity::sea_orm_active_enums::AwdpEvaluationStatus;
    use sea_orm::sea_query::Condition;
    let evaluating = awdp_fix_rounds::Entity::find()
        .filter(awdp_fix_rounds::Column::Status.eq("evaluating"))
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    let mut n = 0usize;
    for round in evaluating {
        let (actual, unfinished) = {
            let all = awdp_evaluations::Entity::find()
                .filter(awdp_evaluations::Column::FixRoundId.eq(round.id))
                .all(db)
                .await
                .map_err(|e| AwdpError::Database(e.to_string()))?;
            let actual = all.len();
            let unfinished = all
                .iter()
                .filter(|e| {
                    matches!(
                        e.status,
                        AwdpEvaluationStatus::Pending | AwdpEvaluationStatus::Running
                    )
                })
                .count();
            (actual, unfinished)
        };
        let ok = match round.expected_eval_count {
            Some(expected) if expected > 0 => actual == expected as usize && unfinished == 0,
            _ => actual > 0 && unfinished == 0,
        };
        if ok {
            set_status(db, round.id, "completed").await?;
            n += 1;
        }
    }
    Ok(n)
}
