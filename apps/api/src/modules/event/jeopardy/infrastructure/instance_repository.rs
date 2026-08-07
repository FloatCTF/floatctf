//! Persistence operations for generic challenge instances.

use sea_orm::{ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{instances, sea_orm_active_enums::InstanceStatus};

pub async fn find_owned_running(
    db: &DatabaseConnection,
    instance_id: Uuid,
    user_id: Uuid,
) -> Result<Option<instances::Model>, sea_orm::DbErr> {
    instances::Entity::find_by_id(instance_id)
        .filter(instances::Column::UserId.eq(user_id))
        .filter(instances::Column::Status.eq(InstanceStatus::Running))
        .one(db)
        .await
}

pub async fn list_cleanup_candidates(
    db: &DatabaseConnection,
) -> Result<Vec<instances::Model>, sea_orm::DbErr> {
    instances::Entity::find()
        .filter(instances::Column::Status.is_in([InstanceStatus::Running, InstanceStatus::Failed]))
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
    let result = instances::Entity::update_many()
        .set(instances::ActiveModel {
            status: Set(next),
            updated_at: Set(chrono::Utc::now().fixed_offset()),
            ..Default::default()
        })
        .filter(instances::Column::Id.eq(instance_id))
        .filter(instances::Column::Status.eq(expected))
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
