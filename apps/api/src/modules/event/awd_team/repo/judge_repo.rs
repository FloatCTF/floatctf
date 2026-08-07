//! Judge batch and task repository.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::entity::{awd_judge_batches, awd_judge_tasks, sea_orm_active_enums::JudgeTaskStatus};

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

pub async fn timeout_pending_tasks(
    db: &DatabaseConnection,
    round_id: Uuid,
) -> Result<u64, sea_orm::DbErr> {
    let result = awd_judge_tasks::Entity::update_many()
        // `set(ActiveModel)` casts enum values for the `judge_task_status` column.
        .set(awd_judge_tasks::ActiveModel {
            status: Set(JudgeTaskStatus::JudgeTimeout),
            ..Default::default()
        })
        .filter(awd_judge_tasks::Column::RoundId.eq(round_id))
        .filter(
            awd_judge_tasks::Column::Status
                .is_in([JudgeTaskStatus::Pending, JudgeTaskStatus::Running]),
        )
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}
