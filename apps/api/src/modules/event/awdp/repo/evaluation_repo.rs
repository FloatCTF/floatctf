//! awdp_evaluations 仓储。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::entity::{
    awdp_evaluations, awdp_instances, instances,
    sea_orm_active_enums::{AwdpEvaluationKind, AwdpEvaluationStatus},
};
use crate::modules::event::awdp::{AwdpError, AwdpResult};

/// 创建 official 评估（每 round × instance 唯一；冲突视为已存在）。
pub async fn create_official(
    db: &DatabaseConnection,
    event_id: Uuid,
    instance_id: Uuid,
    fix_round_id: Uuid,
) -> AwdpResult<awdp_evaluations::Model> {
    let now = Utc::now().into();
    let model = awdp_evaluations::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
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
        Err(sea_orm::DbErr::Exec(inner))
            if inner.to_string().contains("awdp_evaluations_official_uidx") =>
        {
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
    event_id: Uuid,
    instance_id: Uuid,
) -> AwdpResult<awdp_evaluations::Model> {
    let now = Utc::now().into();
    awdp_evaluations::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
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

/// worker 领取 pending 评估（SKIP LOCKED）。
pub async fn claim_pending(
    db: &DatabaseConnection,
    limit: u64,
) -> AwdpResult<Vec<awdp_evaluations::Model>> {
    use sea_orm::{QuerySelect, sea_query::LockBehavior};

    let rows = awdp_evaluations::Entity::find()
        .filter(awdp_evaluations::Column::Status.eq(AwdpEvaluationStatus::Pending))
        .order_by_asc(awdp_evaluations::Column::CreatedAt)
        .limit(limit)
        .lock_with_behavior(
            sea_orm::sea_query::LockType::Update,
            LockBehavior::SkipLocked,
        )
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    for row in &rows {
        let mut am: awdp_evaluations::ActiveModel = row.clone().into();
        am.status = Set(AwdpEvaluationStatus::Running);
        am.started_at = Set(Some(Utc::now().into()));
        am.updated_at = Set(Utc::now().into());
        am.update(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?;
    }
    Ok(rows)
}

/// 写入终态（结果摘要 + status）。
#[allow(clippy::too_many_arguments)]
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
    am.updated_at = Set(now);
    am.update(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;
    Ok(())
}

/// 事件的全部评估（选手视角过滤在 service 层）。
pub async fn list_for_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<Vec<awdp_evaluations::Model>> {
    awdp_evaluations::Entity::find()
        .filter(awdp_evaluations::Column::EventId.eq(event_id))
        .order_by_desc(awdp_evaluations::Column::CreatedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))
}

/// 事件下全部带 instance 的评估（管理端视图）。
pub async fn list_for_event_with_instances(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> AwdpResult<
    Vec<(
        awdp_evaluations::Model,
        awdp_instances::Model,
        instances::Model,
    )>,
> {
    let evals = list_for_event(db, event_id).await?;
    let mut out = Vec::with_capacity(evals.len());
    for ev in evals {
        let ext = awdp_instances::Entity::find_by_id(ev.instance_id)
            .one(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
            .ok_or_else(|| AwdpError::Internal("awdp instance missing".into()))?;
        let inst = instances::Entity::find_by_id(ev.instance_id)
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
