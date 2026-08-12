use sea_orm::{DatabaseConnection, DbErr};

use crate::modules::event::jeopardy::api::InstancesDto;
use crate::{
    api::prelude::*,
    entity::{
        awdp_instances, awdp_runs, challenges, event_challenge_instance, event_instances, events,
        gameboxes, users,
    },
    modules::event::{
        common::domain::practice_event::require_practice_jeopardy_event,
        jeopardy::{
            application::context::EventContextBuilder, application::instance as jeopardy_instance,
        },
    },
};

/// 汇总当前用户的全部系统练习实例（Jeopardy 练习 + AWDP 练习），
/// 供 GET /api/instances 与 DB-gated 测试复用。
/// - 基础行：`event_instances`（归一化单根）中 owner_user_id = 用户 且 running；
/// - 家族分类：`event_challenge_instance`（id 1:1）→ 挑战练习实例；
///   `awdp_instances`（instance_id 关联且 run 为练习）→ AWDP 练习实例；
/// - 批量补充展示名：挑战名 / GameBox 名 / 赛事标题 / 用户昵称；
/// - AWDP 实例不返回 flag（flag 在容器内部，列表不暴露）；
/// - 竞赛/团队实例（owner_user_id 为空）不会出现在练习视图。
pub async fn collect_practice_instances(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<InstancesDto>, DbErr> {
    use std::collections::HashMap;

    let rows = event_instances::Entity::find()
        .filter(event_instances::Column::OwnerUserId.eq(user_id))
        .filter(event_instances::Column::RuntimeState.eq("running"))
        .order_by_desc(event_instances::Column::UpdatedAt)
        .all(db)
        .await?;

    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    if ids.is_empty() {
        return Ok(vec![]);
    }

    // 家族关联行（批量拉取，避免逐行查询）。
    let challenge_rows = event_challenge_instance::Entity::find()
        .filter(event_challenge_instance::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await?;
    let awdp_pairs = awdp_instances::Entity::find()
        .filter(awdp_instances::Column::InstanceId.is_in(ids.iter().copied()))
        .find_also_related(awdp_runs::Entity)
        .filter(awdp_runs::Column::GameboxId.is_not_null())
        .all(db)
        .await?;

    // 批量展示名。
    let challenge_titles: HashMap<Uuid, String> = {
        let challenge_ids: Vec<Uuid> = challenge_rows.iter().map(|m| m.challenge_id).collect();
        if challenge_ids.is_empty() {
            HashMap::new()
        } else {
            challenges::Entity::find()
                .filter(challenges::Column::Id.is_in(challenge_ids))
                .all(db)
                .await?
                .into_iter()
                .map(|c| (c.id, c.name))
                .collect()
        }
    };
    let gamebox_titles: HashMap<Uuid, String> = {
        let gamebox_ids: Vec<Uuid> = awdp_pairs.iter().map(|(m, _)| m.gamebox_id).collect();
        if gamebox_ids.is_empty() {
            HashMap::new()
        } else {
            gameboxes::Entity::find()
                .filter(gameboxes::Column::Id.is_in(gamebox_ids))
                .all(db)
                .await?
                .into_iter()
                .map(|g| (g.id, g.name))
                .collect()
        }
    };
    let event_titles: HashMap<Uuid, String> = {
        let event_ids: Vec<Uuid> = rows.iter().map(|r| r.event_id).collect();
        events::Entity::find()
            .filter(events::Column::Id.is_in(event_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|e| (e.id, e.title))
            .collect()
    };
    let user_name = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .map(|u| u.nickname);

    let mut dtos = Vec::with_capacity(rows.len());
    for runtime in &rows {
        if let Some(inst) = challenge_rows.iter().find(|m| m.id == runtime.id) {
            let mut dto = InstancesDto::from_pair(inst, runtime);
            dto.flag.clear();
            dto.challenge_title = challenge_titles.get(&inst.challenge_id).cloned();
            dto.event_title = event_titles.get(&runtime.event_id).cloned();
            dto.user_name = user_name.clone();
            dtos.push(dto);
        } else if let Some((ext, run)) =
            awdp_pairs.iter().find(|(m, _)| m.instance_id == runtime.id)
        {
            if run.is_none() {
                continue;
            }
            let dto = InstancesDto {
                id: runtime.id,
                status: runtime.runtime_state.clone(),
                flag: String::new(),
                content: None,
                challenge_id: None,
                event_id: runtime.event_id,
                team_id: ext.owner_team_id,
                user_id,
                identifier: runtime.container_name.clone(),
                created_at: runtime.created_at,
                updated_at: runtime.updated_at,
                destroy_at: runtime.expires_at,
                challenge_title: None,
                event_title: event_titles.get(&runtime.event_id).cloned(),
                user_name: user_name.clone(),
                run_id: Some(ext.run_id),
                gamebox_id: Some(ext.gamebox_id),
                gamebox_title: gamebox_titles.get(&ext.gamebox_id).cloned(),
            };
            dtos.push(dto);
        }
        // 无家族关联的孤儿行跳过（不属于当前用户的练习视图）。
    }
    Ok(dtos)
}

/// 内存版过滤匹配（与 build_filter_condition 同款 token 语义：& 与 |，空格并入 value）。
fn match_instances_filter(dto: &InstancesDto, filter: &str) -> bool {
    let tokens: Vec<&str> = filter.split_whitespace().collect();
    let mut or_groups: Vec<Vec<(&str, String)>> = Vec::new();
    let mut and_group: Vec<(&str, String)> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        if token == "&" {
            i += 1;
            continue;
        }
        if token == "|" {
            if !and_group.is_empty() {
                or_groups.push(std::mem::take(&mut and_group));
            }
            i += 1;
            continue;
        }
        if let Some(pos) = token.find(':') {
            let key = &token[..pos];
            let mut value = token[pos + 1..].to_string();
            i += 1;
            while i < tokens.len() && tokens[i] != "&" && tokens[i] != "|" {
                value.push(' ');
                value.push_str(tokens[i]);
                i += 1;
            }
            and_group.push((key, value));
        } else {
            i += 1;
        }
    }
    if !and_group.is_empty() {
        or_groups.push(and_group);
    }
    if or_groups.is_empty() {
        return true;
    }

    let matches = |key: &str, value: &str| -> bool {
        match key {
            "id" => dto.id.to_string() == value,
            "status" => dto.status == value,
            "identifier" => dto.identifier.contains(value),
            "challenge_id" => dto.challenge_id.is_some_and(|id| id.to_string() == value),
            "event_id" => dto.event_id.to_string() == value,
            "gamebox_id" => dto.gamebox_id.is_some_and(|id| id.to_string() == value),
            "run_id" => dto.run_id.is_some_and(|id| id.to_string() == value),
            _ => true,
        }
    };
    or_groups
        .iter()
        .any(|group| group.iter().all(|(k, v)| matches(k, v)))
}

/// GET /api/instances — 当前用户的系统练习实例列表（挑战练习 + AWDP 练习）
#[get("")]
pub async fn get_instances(
    user: UserJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<InstancesDto>> {
    let user = user.into_inner();
    let mut query_params = query_params.0;

    let mut items = collect_practice_instances(ctx.db.get_ref(), user.id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if let Some(filter) = query_params.filter.clone() {
        if !filter.trim().is_empty() {
            items.retain(|dto| match_instances_filter(dto, &filter));
        }
    }
    let total = items.len();

    let items = if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
        items
            .into_iter()
            .skip((page.saturating_sub(1) * limit) as usize)
            .take(limit as usize)
            .collect::<Vec<_>>()
    } else {
        items
    };

    query_params.total = Some(total);
    UniResponse::ok_meta(Some(items), query_params.into()).into()
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

    let (model, runtime) = event_challenge_instance::Entity::find_by_id(instance_id)
        .filter(event_challenge_instance::Column::UserId.eq(user.id))
        .find_also_related(event_instances::Entity)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", instance_id)))?;
    let runtime = runtime.ok_or(AppError::NotFound(format!(" {} not exist", instance_id)))?;

    let mut dto = InstancesDto::from_pair(&model, &runtime);
    dto.flag.clear();

    UniResponse::ok(Some(dto)).into()
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

    // 补查运行时行构建 DTO（launch 已写入双表）。
    let (_, runtime) = event_challenge_instance::Entity::find_by_id(instance.id)
        .find_also_related(event_instances::Entity)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound("instance not found".into()))?;
    let runtime = runtime.ok_or(AppError::NotFound("instance runtime not found".into()))?;

    UniResponse::ok(Some(InstancesDto::from_pair(&instance, &runtime))).into()
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
    let instance = event_challenge_instance::Entity::find_by_id(instance_id)
        .filter(event_challenge_instance::Column::UserId.eq(user.id))
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
