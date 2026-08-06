//! Persistence operations for generic challenge instances.

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
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
    let result = instances::Entity::update_many()
        .col_expr(
            instances::Column::Status,
            sea_orm::sea_query::Expr::value(next),
        )
        .col_expr(
            instances::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now().fixed_offset()),
        )
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
