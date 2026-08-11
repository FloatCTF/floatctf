//! 通用题目实例的持久化操作。

use sea_orm::{ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{challenge_instances, sea_orm_active_enums::InstanceStatus};

pub async fn find_owned_running(
    db: &DatabaseConnection,
    instance_id: Uuid,
    user_id: Uuid,
) -> Result<Option<challenge_instances::Model>, sea_orm::DbErr> {
    challenge_instances::Entity::find_by_id(instance_id)
        .filter(challenge_instances::Column::UserId.eq(user_id))
        .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
        .one(db)
        .await
}

pub async fn list_cleanup_candidates(
    db: &DatabaseConnection,
) -> Result<Vec<challenge_instances::Model>, sea_orm::DbErr> {
    challenge_instances::Entity::find()
        .filter(
            challenge_instances::Column::Status
                .is_in([InstanceStatus::Running, InstanceStatus::Failed]),
        )
        .all(db)
        .await
}

pub async fn transition_status(
    db: &DatabaseConnection,
    instance_id: Uuid,
    expected: InstanceStatus,
    next: InstanceStatus,
) -> Result<(), sea_orm::DbErr> {
    // Use `set(ActiveModel)` (not `col_expr(Expr::value(..))`) so enum columns are
    // written through `Column::save_as` → `CAST(... AS instance_status)`.
    // A raw `Expr::value` binds TEXT and Postgres rejects it with
    // "column \"status\" is of type instance_status but expression is of type text".
    let result = challenge_instances::Entity::update_many()
        .set(challenge_instances::ActiveModel {
            status: Set(next),
            updated_at: Set(chrono::Utc::now().fixed_offset()),
            ..Default::default()
        })
        .filter(challenge_instances::Column::Id.eq(instance_id))
        .filter(challenge_instances::Column::Status.eq(expected))
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
