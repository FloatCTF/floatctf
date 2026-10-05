//! 通用题目实例的持久化操作。
//!
//! 归一化实例：`event_challenge_instance` 是关联表（id = instances.id），
//! 运行时身份（容器名/状态/过期）在 `instances`；查询一律 join。

use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter,
    QuerySelect,
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

/// 查找某赛事战队拥有的运行中实例。
///
/// 团队实例只在创建时记录实际启动者 `user_id`，但整个战队共享该实例；因此团队成员
/// 的销毁/解题后清理必须按 `(event_id, team_id)` 授权，不能按启动者用户 ID 授权。
pub async fn find_team_running(
    db: &DatabaseConnection,
    instance_id: Uuid,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<Option<InstanceRow>, sea_orm::DbErr> {
    let row = event_challenge_instance::Entity::find_by_id(instance_id)
        .filter(event_challenge_instance::Column::EventId.eq(event_id))
        .filter(event_challenge_instance::Column::TeamId.eq(team_id))
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

/// 清理候选：已过期的 running 实例，以及需要重试收敛的 failed 实例。
/// 活跃且未过期的 running 实例必须在 API 重启/滚动发布时保留。
pub async fn list_cleanup_candidates(
    db: &DatabaseConnection,
) -> Result<Vec<InstanceRow>, sea_orm::DbErr> {
    let now = chrono::Utc::now().fixed_offset();
    // 注意：必须**直接查 event_instances**。此前写成
    // `event_challenge_instance::Entity::find().filter(event_instances::Column::…)`
    // ——在基实体上过滤关联表的列，实测匹配不到任何行，导致清理任务每次
    // 都是 0 候选、僵尸实例无限累积（F54 根因）。
    let runtimes = event_instances::Entity::find()
        .filter(
            Condition::any()
                .add(event_instances::Column::RuntimeState.eq("failed"))
                .add(
                    Condition::all()
                        .add(event_instances::Column::RuntimeState.eq("running"))
                        .add(event_instances::Column::ExpiresAt.lte(now)),
                ),
        )
        .all(db)
        .await?;
    if runtimes.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<Uuid> = runtimes.iter().map(|r| r.id).collect();
    let instances = event_challenge_instance::Entity::find()
        .filter(event_challenge_instance::Column::Id.is_in(ids))
        .all(db)
        .await?;
    let by_id: std::collections::HashMap<Uuid, event_challenge_instance::Model> =
        instances.into_iter().map(|i| (i.id, i)).collect();
    Ok(runtimes
        .into_iter()
        .filter_map(|r| by_id.get(&r.id).cloned().map(|i| (i, r)))
        .collect())
}

/// 存活性对账候选（F54）：标记 `running` 但 `updated_at` 已早于 `now - min_age_secs`
/// 的实例。用于发现「容器已被外部删除 / 宿主重启 / 容器运行时被清理」而 DB 仍记为
/// running 的僵尸记录（此前只有 TTL 到期才会被清理，实测 90 秒后仍不自愈）。
pub async fn list_running_older_than(
    db: &DatabaseConnection,
    min_age_secs: i64,
) -> Result<Vec<event_instances::Model>, sea_orm::DbErr> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(min_age_secs)).fixed_offset();
    event_instances::Entity::find()
        .filter(event_instances::Column::RuntimeState.eq("running"))
        .filter(event_instances::Column::UpdatedAt.lte(cutoff))
        .all(db)
        .await
}

/// 删除同容器名、状态为 `completed` 的旧实例行。
///
/// 练习/竞赛 identifier 对 (event, user/team, challenge) 是确定性的（如练习 `JP-{user}-{challenge}`）：
/// 实例销毁后行保留为 `completed`，容器名仍占着 `event_instances_container_name_uidx`，
/// 再次启动同一题会撞唯一约束报 400（练习复练被阻塞）。容器已移除、行已无价值，
/// 启动前先清掉旧行（id → event_instances 级联删除 event_challenge_instance 关联行）。
pub async fn delete_completed_by_container_name(
    db: &DatabaseConnection,
    container_name: &str,
) -> Result<u64, sea_orm::DbErr> {
    let result = event_instances::Entity::delete_many()
        .filter(event_instances::Column::ContainerName.eq(container_name))
        .filter(event_instances::Column::RuntimeState.eq("completed"))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
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
