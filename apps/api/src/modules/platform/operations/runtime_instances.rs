use std::{collections::HashMap, str::FromStr};

use sea_orm::Condition;

use crate::modules::event::jeopardy::api::InstancesDto;
use crate::{
    api::{FilterMapping, prelude::*, sea_orm_utils::query_query},
    entity::{
        challenge_instances, challenges, events, sea_orm_active_enums::InstanceStatus, users,
    },
};

/// GET /api/admin/instances
#[get("")]
pub async fn get_instances(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<InstancesDto>> {
    let mut query_params = query_params.0;

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

    // 批量查询展示名称（题目名/赛事标题/用户昵称），避免逐行 N+1 查询。
    let mut challenge_titles: HashMap<Uuid, String> = HashMap::new();
    let mut event_titles: HashMap<Uuid, String> = HashMap::new();
    let mut user_names: HashMap<Uuid, String> = HashMap::new();
    if !items.is_empty() {
        let challenge_ids: Vec<Uuid> = items.iter().map(|i| i.challenge_id).collect();
        challenges::Entity::find()
            .filter(challenges::Column::Id.is_in(challenge_ids))
            .all(ctx.db.get_ref())
            .await?
            .into_iter()
            .for_each(|c| {
                challenge_titles.insert(c.id, c.name);
            });

        let event_ids: Vec<Uuid> = items.iter().map(|i| i.event_id).collect();
        events::Entity::find()
            .filter(events::Column::Id.is_in(event_ids))
            .all(ctx.db.get_ref())
            .await?
            .into_iter()
            .for_each(|e| {
                event_titles.insert(e.id, e.title);
            });

        let user_ids: Vec<Uuid> = items.iter().map(|i| i.user_id).collect();
        users::Entity::find()
            .filter(users::Column::Id.is_in(user_ids))
            .all(ctx.db.get_ref())
            .await?
            .into_iter()
            .for_each(|u| {
                user_names.insert(u.id, u.nickname);
            });
    }

    let dtos: Vec<InstancesDto> = items
        .into_iter()
        .map(|m| {
            let challenge_id = m.challenge_id;
            let event_id = m.event_id;
            let user_id = m.user_id;
            let dto = InstancesDto::from(m);
            dto.with_names(
                challenge_titles.get(&challenge_id).cloned(),
                event_titles.get(&event_id).cloned(),
                user_names.get(&user_id).cloned(),
            )
        })
        .collect();

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(dtos), query_params.into()).into()
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
