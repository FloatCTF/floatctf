use std::str::FromStr;

use sea_orm::Condition;

use crate::api::dto::map_dto_vec;

use crate::modules::event::jeopardy::api::InstancesDto;
use crate::{
    api::{FilterMapping, prelude::*, sea_orm_utils::query_query},
    entity::{challenge_instances, sea_orm_active_enums::InstanceStatus},
};

/// GET /api/admin/instances
#[get("")]
pub async fn get_instances(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<InstancesDto>> {
    let mut query_params = query_params.0;
    // const filterKeys = ["id", "status", "ref", "flag", "challenge_id", "user_id"];

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all().add(
                    challenge_instances::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
        FilterMapping {
            key: "status",
            column: Box::new(|v| {
                Condition::all().add(
                    challenge_instances::Column::Status
                        .eq(serde_json::from_str(v).unwrap_or(InstanceStatus::Running)),
                )
            }),
        },
        FilterMapping {
            key: "ref",
            column: Box::new(|v| {
                Condition::all().add(challenge_instances::Column::Identifier.contains(v))
            }),
        },
        FilterMapping {
            key: "flag",
            column: Box::new(|v| {
                Condition::all().add(challenge_instances::Column::Flag.contains(v))
            }),
        },
        FilterMapping {
            key: "challenge_id",
            column: Box::new(|v| {
                Condition::all().add(
                    challenge_instances::Column::ChallengeId
                        .eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
        FilterMapping {
            key: "user_id",
            column: Box::new(|v| {
                Condition::all().add(
                    challenge_instances::Column::UserId
                        .eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())),
                )
            }),
        },
    ];
    let (items, total_items) = query_query::<challenge_instances::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| {
            stmt.order_by_desc(challenge_instances::Column::UpdatedAt)
        })),
    )
    .await?;

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/admin/instances/{instance_id}
#[get("/{instance_id}")]
pub async fn get_instance(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    instance_id: Path<Uuid>,
) -> UniResult<InstancesDto> {
    let instance_id = instance_id.into_inner();
    let model = challenge_instances::Entity::find_by_id(instance_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", instance_id)))?;

    UniResponse::ok(Some(model.into())).into()
}
