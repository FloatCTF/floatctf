//! Admin challenge catalog CRUD handlers.

use crate::api::dto::map_dto_vec;

use crate::modules::challenge::catalog::ChallengesDto;
use crate::{
    api::{FilterMapping, dto::DeleteItemsRequest, prelude::*, sea_orm_utils::query_query},
    entity::{challenges, prelude::Challenges},
};

use sea_orm::Condition;
use std::str::FromStr;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateChallengeRequest {
    pub name: String,
    pub category: String,
    pub description: String,
    pub hidden: bool,
    pub attachment: Option<String>,
    pub toml_str: String,
}
// POST /api/admin/challenges
#[post("")]
pub async fn create_challenge(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    ccr: Json<CreateChallengeRequest>,
) -> UniResult<ChallengesDto> {
    let user = user.into_inner();
    let ccr = ccr.into_inner();

    let new_challenge = challenges::ActiveModel {
        name: Set(ccr.name),
        category: Set(ccr.category),
        description: Set(ccr.description),
        attachment: Set(ccr.attachment),
        toml_str: Set(ccr.toml_str),
        hidden: Set(ccr.hidden),
        ..Default::default()
    };

    let challenge = new_challenge.insert(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGES",
            "CREATE",
            format!("{} 创建题目: {}", user.username, challenge.name).as_str(),
            json!({"name": challenge.name, "category": challenge.category}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(challenge.into())).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchChallengeRequest {
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub attachment: Option<String>,
    pub hidden: Option<bool>,
    pub toml_str: Option<String>,
}
/// PATCH /api/admin/challenges/{challenge_id}
#[patch("/{challenge_id}")]
pub async fn patch_challenge(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    pcr: Json<PatchChallengeRequest>,
    challenge_id: Path<Uuid>,
) -> UniResult<ChallengesDto> {
    let user = user.into_inner();
    let pcr = pcr.into_inner();
    let challenge_id = challenge_id.into_inner();
    let challenge = Challenges::find_by_id(challenge_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

    let mut m_challenge = challenge.into_active_model();

    pcr.name.map(|n| {
        m_challenge.name = Set(n);
    });

    pcr.category.map(|c| {
        m_challenge.category = Set(c);
    });

    pcr.description.map(|d| {
        m_challenge.description = Set(d);
    });

    pcr.attachment.map(|a| {
        m_challenge.attachment = Set(a.into());
    });

    pcr.hidden.map(|h| {
        m_challenge.hidden = Set(h);
    });

    pcr.toml_str.map(|t| m_challenge.toml_str = Set(t));
    m_challenge.updated_at = Set(Utc::now().into());

    let challenge = m_challenge.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGES",
            "UPDATE",
            format!("{} 更新题目: {}", user.username, challenge.name).as_str(),
            json!({"challenge_id": challenge.id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(challenge.into())).into()
}

/// GET /api/admin/challenges
#[get("")]
pub async fn get_challenges(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<ChallengesDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(challenges::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "name",
            column: Box::new(|v| Condition::all().add(challenges::Column::Name.contains(v))),
        },
        FilterMapping {
            key: "category",
            column: Box::new(|v| Condition::all().add(challenges::Column::Category.contains(v))),
        },
        FilterMapping {
            key: "hidden",
            column: Box::new(|v| {
                Condition::all()
                    .add(challenges::Column::Hidden.eq(v.parse::<bool>().unwrap_or(true)))
            }),
        },
        FilterMapping {
            key: "description",
            column: Box::new(|v| Condition::all().add(challenges::Column::Description.contains(v))),
        },
    ];
    let (items, total_items) = query_query::<challenges::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| {
            stmt.order_by_desc(challenges::Column::UpdatedAt)
        })),
    )
    .await?;

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/admin/challenges/{challenge_id}
#[get("/{challenge_id}")]
pub async fn get_challenge(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    id: Path<Uuid>,
) -> UniResult<ChallengesDto> {
    let model = Challenges::find_by_id(*id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", id)))?;

    UniResponse::ok(Some(model.into())).into()
}

/// DELETE /api/admin/challenges
#[delete("")]
pub async fn delete_challenge(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();

    let mut deleted_count = 0;
    let challenges_path = get_setting(&ctx.db, "CHALLENGES_DIR")
        .await
        .map_err(|e| AppError::BadRequest(format!("get setting error: {}", e)))?;

    for challenge_id in dir.id_list {
        let challenge = Challenges::find_by_id(challenge_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

        let del_challenge_path = std::path::Path::new(&challenges_path).join(&challenge.safe_name);
        if del_challenge_path.exists() {
            std::fs::remove_dir_all(&del_challenge_path)
                .map_err(|e| AppError::BadRequest(format!("delete challenge dir error: {}", e)))?;
        }
        let r = challenge.delete(ctx.db.get_ref()).await?;
        deleted_count += r.rows_affected;
    }

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGES",
            "DELETE",
            format!("{} 删除 {} 道题目", user.username, deleted_count).as_str(),
            json!({"deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}
