//! Admin challenge set handlers.

use std::str::FromStr;

use sea_orm::Condition;

use crate::api::dto::map_dto_vec;

use crate::modules::challenge::catalog::ChallengesDto;
use crate::modules::challenge::set::ChallengeSetsDto;
use crate::{
    api::{FilterMapping, dto::DeleteItemsRequest, prelude::*, sea_orm_utils::query_query},
    entity::{challenge_set_items, challenge_sets, challenges},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateChallengeSetRequest {
    pub name: String,
    pub description: Option<String>,
    pub challenge_id_list: Option<Vec<Uuid>>,
}
/// POST /api/admin/challenge_sets
#[post("")]
pub async fn create_challenge_set(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    csr: Json<CreateChallengeSetRequest>,
) -> UniResult<ChallengeSetsDto> {
    let user = user.into_inner();
    let csr = csr.into_inner();
    let challenge_set = challenge_sets::ActiveModel {
        name: Set(csr.name),
        description: Set(csr.description),
        ..Default::default()
    };
    let challenge_set = challenge_set.insert(ctx.db.get_ref()).await?;

    if let Some(challenge_id_list) = csr.challenge_id_list {
        for challenge_id in challenge_id_list {
            challenge_set_items::ActiveModel {
                set_id: Set(challenge_set.id),
                challenge_id: Set(challenge_id),
                ..Default::default()
            }
            .insert(ctx.db.get_ref())
            .await?;
        }
    }

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGE_SETS",
            "CREATE",
            format!("{} 创建题目集: {}", user.username, challenge_set.name).as_str(),
            json!({"name": challenge_set.name}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchChallengeSetRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// PATCH /api/admin/challenge_sets/{challenge_set_id}
#[patch("/{challenge_set_id}")]
pub async fn patch_challenge_set(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    challenge_set_id: Path<Uuid>,
    psr: Json<PatchChallengeSetRequest>,
) -> UniResult<ChallengeSetsDto> {
    let user = user.into_inner();
    let challenge_set_id = challenge_set_id.into_inner();
    let psr = psr.into_inner();
    let challenge_set = challenge_sets::Entity::find_by_id(challenge_set_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(
            "challenge set {} not found",
            challenge_set_id
        )))?;
    let mut m_challenge_set = challenge_set.into_active_model();

    psr.name.map(|name| m_challenge_set.name = Set(name));
    psr.description
        .map(|description| m_challenge_set.description = Set(Some(description)));

    let challenge_set = m_challenge_set.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGE_SETS",
            "UPDATE",
            format!("{} 更新题目集: {}", user.username, challenge_set.name).as_str(),
            json!({"challenge_set_id": challenge_set.id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(challenge_set.into())).into()
}

/// DELETE /api/admin/challenge_sets
#[delete("")]
pub async fn delete_challenge_set(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();
    let deleted_count = challenge_sets::Entity::delete_many()
        .filter(challenge_sets::Column::Id.is_in(dir.id_list))
        .exec(ctx.db.get_ref())
        .await?
        .rows_affected;

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGE_SETS",
            "DELETE",
            format!("{} 删除 {} 个题目集", user.username, deleted_count).as_str(),
            json!({"deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}

/// GET /api/admin/challenge_sets
#[get("")]
pub async fn get_challenge_sets(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<ChallengeSetsDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(challenge_sets::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "name",
            column: Box::new(|v| Condition::all().add(challenge_sets::Column::Name.contains(v))),
        },
        FilterMapping {
            key: "description",
            column: Box::new(|v| {
                Condition::all().add(challenge_sets::Column::Description.contains(v))
            }),
        },
    ];
    let (items, total_items) = query_query::<challenge_sets::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| {
            stmt.order_by_desc(challenge_sets::Column::CreatedAt)
        })),
    )
    .await?;

    query_params.total = Some(total_items);
    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/admin/challenge_sets/{challenge_set_id}
#[get("/{challenge_set_id}")]
pub async fn get_challenge_set(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    challenge_set_id: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<ChallengesDto>> {
    let challenge_set_id = challenge_set_id.into_inner();
    let mut query_params = query_params.0;

    let challenge_set_items = challenge_set_items::Entity::find()
        .filter(challenge_set_items::Column::SetId.eq(challenge_set_id))
        .all(ctx.db.get_ref())
        .await?;
    let challenge_ids: Vec<Uuid> = challenge_set_items
        .iter()
        .map(|item| item.challenge_id)
        .collect();
    let total_items = challenge_ids.len();

    let (items, _total) = if let (Some(limit), Some(page)) = (query_params.limit, query_params.page)
    {
        let stmt = challenges::Entity::find().filter(challenges::Column::Id.is_in(challenge_ids));
        let total_pages = (total_items + limit as usize - 1) / limit as usize;
        let page_index = page
            .saturating_sub(1)
            .min(total_pages.saturating_sub(1) as u64);
        let items = stmt
            .paginate(ctx.db.get_ref(), limit)
            .fetch_page(page_index)
            .await?;
        (items, total_items)
    } else {
        let items = challenges::Entity::find()
            .filter(challenges::Column::Id.is_in(challenge_ids))
            .all(ctx.db.get_ref())
            .await?;
        (items, total_items)
    };

    query_params.total = Some(total_items);
    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// DELETE /api/admin/challenge_sets/{challenge_set_id}/challenges
#[delete("/{challenge_set_id}/challenges")]
pub async fn delete_challenge_from_set(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    challenge_set_id: Path<Uuid>,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let challenge_set_id = challenge_set_id.into_inner();
    let dir = dir.into_inner();

    let deleted_count = challenge_set_items::Entity::delete_many()
        .filter(challenge_set_items::Column::SetId.eq(challenge_set_id))
        .filter(challenge_set_items::Column::ChallengeId.is_in(dir.id_list))
        .exec(ctx.db.get_ref())
        .await?
        .rows_affected;

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGE_SETS",
            "DELETE",
            format!("{} 从题目集移除 {} 道题目", user.username, deleted_count).as_str(),
            json!({"challenge_set_id": challenge_set_id, "deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddChallengeToSetRequest {
    pub challenge_id_list: Vec<Uuid>,
}
/// POST /api/admin/challenge_sets/{challenge_set_id}/challenges
#[post("/{challenge_set_id}/challenges")]
pub async fn add_challenge_to_set(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    challenge_set_id: Path<Uuid>,
    acr: Json<AddChallengeToSetRequest>,
) -> UniResult<()> {
    let user = user.into_inner();
    let challenge_set_id = challenge_set_id.into_inner();
    let acr = acr.into_inner();
    let count = acr.challenge_id_list.len();
    for challenge_id in acr.challenge_id_list {
        challenge_set_items::ActiveModel {
            set_id: Set(challenge_set_id),
            challenge_id: Set(challenge_id),
            ..Default::default()
        }
        .insert(ctx.db.get_ref())
        .await?;
    }

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGE_SETS",
            "ADD_CHALLENGES",
            format!("{} 向题目集添加 {} 道题目", user.username, count).as_str(),
            json!({"challenge_set_id": challenge_set_id, "count": count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
