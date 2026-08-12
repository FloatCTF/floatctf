//! 通用题目实例的持久化操作。
//!
//! 归一化实例：`event_challenge_instance` 是关联表（id = instances.id），
//! 运行时身份（容器名/状态/过期）在 `instances`；查询一律 join。

use sea_orm::{
    ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect,
};
use uuid::Uuid;

use crate::entity::{event_challenge_instance, event_instances};

/// 一个挑战实例 = 题目领域行 + 通用运行时行（1:1）。
pub type InstanceRow = (event_challenge_instance::Model, event_instances::Model);

/// 查找某用户拥有的运行中实例（runtime_state = running）。
pub async fn find_owned_running(
    db: &DatabaseConnection,
    instance_id: Uuid,
    user_id: Uuid,
) -> Result<Option<InstanceRow>, sea_orm::DbErr> {
    let row = event_challenge_instance::Entity::find_by_id(instance_id)
        .filter(event_challenge_instance::Column::UserId.eq(user_id))
        .find_also_related(event_instances::Entity)
        .one(db)
        .await?;
    let Some((instance, runtime)) = row else {
        return Ok(None);
    };
    let Some(runtime) = runtime else {
        return Ok(None);
    };
    if runtime.runtime_state != "running" {
        return Ok(None);
    }
    Ok(Some((instance, runtime)))
}

/// 清理候选：runtime_state ∈ {running, failed}。
pub async fn list_cleanup_candidates(
    db: &DatabaseConnection,
) -> Result<Vec<InstanceRow>, sea_orm::DbErr> {
    let rows = event_challenge_instance::Entity::find()
        .filter(
            event_instances::Column::RuntimeState
                .is_in(["running".to_string(), "failed".to_string()]),
        )
        .find_also_related(event_instances::Entity)
        .all(db)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(i, r)| r.map(|r| (i, r)))
        .collect())
}

/// 流转 instances.runtime_state（expected → next），乐观并发保护。
pub async fn transition_runtime_state(
    db: &DatabaseConnection,
    instance_id: Uuid,
    expected: &str,
    next: &str,
) -> Result<(), sea_orm::DbErr> {
    let result = event_instances::Entity::update_many()
        .set(event_instances::ActiveModel {
            runtime_state: Set(next.to_string()),
            stopped_at: Set(if next == "completed" {
                Some(chrono::Utc::now().fixed_offset())
            } else {
                None
            }),
            updated_at: Set(chrono::Utc::now().fixed_offset()),
            ..Default::default()
        })
        .filter(event_instances::Column::Id.eq(instance_id))
        .filter(event_instances::Column::RuntimeState.eq(expected))
        .exec(db)
        .await?;

    if result.rows_affected == 1 {
        Ok(())
    } else {
        Err(sea_orm::DbErr::Custom(format!(
            "instance {instance_id} changed concurrently"
        )))
    }
}
