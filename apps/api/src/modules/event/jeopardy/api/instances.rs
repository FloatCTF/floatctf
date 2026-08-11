use std::str::FromStr;

use actix_web::web;

use sea_orm::Condition;

use crate::modules::event::jeopardy::api::InstancesDto;
use crate::{
    api::{FilterMapping, apply_filters, prelude::*, sea_orm_utils::paginate_query},
    entity::{challenge_instances, challenges, events, sea_orm_active_enums::InstanceStatus},
    modules::event::{
        common::domain::practice_event::require_practice_jeopardy_event,
        jeopardy::application::context::EventContextBuilder,
        jeopardy::application::instance as jeopardy_instance,
    },
};

/// GET /api/instances — 当前用户的系统练习实例列表
#[get("")]
pub async fn get_instances(
    user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<InstancesDto>> {
    let user = user.into_inner();
    let mut query_params = query_params.0;

    let practice = require_practice_jeopardy_event(ctx.db.get_ref())
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

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
            key: "identifier",
            column: Box::new(|v| {
                Condition::all().add(challenge_instances::Column::Identifier.contains(v))
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
    ];

    let stmt = challenge_instances::Entity::find()
        .filter(challenge_instances::Column::Status.eq(InstanceStatus::Running))
        .filter(challenge_instances::Column::EventId.eq(practice.id))
        .filter(challenge_instances::Column::UserId.eq(user.id));
    let stmt = apply_filters(stmt, query_params.filter.clone(), &mappings);
    let stmt = stmt.order_by_desc(challenge_instances::Column::UpdatedAt);

    let (mut items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            paginate_query(stmt, ctx.db.get_ref(), limit, page).await?
        } else {
            let items = stmt.all(ctx.db.get_ref()).await?;
            (items.clone(), items.len())
        };

    for item in &mut items {
        item.flag.clear();
    }

    // 批量查询题目名称，避免逐行查询。
    let challenge_ids: Vec<Uuid> = items.iter().map(|i| i.challenge_id).collect();
    let challenge_titles = if challenge_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        challenges::Entity::find()
            .filter(challenges::Column::Id.is_in(challenge_ids))
            .all(ctx.db.get_ref())
            .await?
            .into_iter()
            .map(|c| (c.id, c.name))
            .collect::<std::collections::HashMap<Uuid, String>>()
    };

    let event_title = practice.title.clone();
    let user_name = user.nickname.clone();

    let dtos: Vec<InstancesDto> = items
        .into_iter()
        .map(|m| {
            let challenge_id = m.challenge_id;
            let dto = InstancesDto::from(m);
            dto.with_names(
                challenge_titles.get(&challenge_id).cloned(),
                Some(event_title.clone()),
                Some(user_name.clone()),
            )
        })
        .collect();

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(dtos), query_params.into()).into()
}

/// GET /api/instances/{instance_id}
#[get("/{instance_id}")]
pub async fn get_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    instance_id: Path<Uuid>,
) -> UniResult<InstancesDto> {
    let instance_id = instance_id.into_inner();
    let user = user.into_inner();

    let mut model = challenge_instances::Entity::find_by_id(instance_id)
        .filter(challenge_instances::Column::UserId.eq(user.id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", instance_id)))?;

    model.flag.clear();

    UniResponse::ok(Some(model.into())).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LaunchInstanceRequest {
    event_id: Option<Uuid>,
    challenge_id: Uuid,
}

/// POST /api/instances/launch
#[post("/launch")]
pub async fn launch_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    lir: Json<LaunchInstanceRequest>,
) -> UniResult<InstancesDto> {
    let user = user.into_inner();
    let lir = lir.into_inner();

    // 练习启动可省略 event_id；显式解析系统练习赛事（Context 不再自动回落）。
    let event = match lir.event_id {
        Some(event_id) => events::Entity::find_by_id(event_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound("no event".into()))?,
        None => require_practice_jeopardy_event(ctx.db.get_ref())
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?,
    };

    let event_ctx = EventContextBuilder::new()
        .db(ctx.db.clone())
        .docker(ctx.docker.clone())
        .user(user.clone())
        .event(event)
        .config(ctx.config.clone())
        .build()
        .await
        .map_err(|e| AppError::BadRequest(format!("build event context error: {}", e)))?;

    let instance = jeopardy_instance::launch_instance(&event_ctx, lir.challenge_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("when launch instance:{}", e)))?;

    ctx.log
        .add_log(
            "INFO",
            "INSTANCE",
            "LAUNCH",
            format!("启动题目 {} 的实例", lir.challenge_id).as_str(),
            json!({"event_id": lir.event_id, "resolved_event_id": event_ctx.event.id}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(instance.into())).into()
}

/// DELETE /api/instances/{instance_id}
#[delete("/{instance_id}")]
pub async fn destroy_instance(
    user: UserJwtGuard,
    ctx: ReqCtx,
    instance_id: Path<Uuid>,
) -> UniResult<()> {
    let user = user.into_inner();
    let instance_id = instance_id.into_inner();

    // 加载实例以解析所属赛事（练习或竞赛）。
    let instance = challenge_instances::Entity::find_by_id(instance_id)
        .filter(challenge_instances::Column::UserId.eq(user.id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", instance_id)))?;

    let event = events::Entity::find_by_id(instance.event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event for instance not found".into()))?;

    let event_ctx = EventContextBuilder::new()
        .db(ctx.db.clone())
        .docker(ctx.docker.clone())
        .user(user.clone())
        .event(event)
        .build()
        .await
        .map_err(|e| AppError::BadRequest(format!("build event context:{}", e)))?;
    jeopardy_instance::destroy_instance(&event_ctx, instance_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("destroy_instance:{}", e)))?;

    ctx.log
        .add_log(
            "INFO",
            "INSTANCE",
            "DESTROY",
            format!("销毁实例 {}", instance_id).as_str(),
            json!({}),
            user.id.into(),
            None,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
