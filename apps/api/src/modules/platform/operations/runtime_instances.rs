//! 管理端统一实例视图（归一化后：单一根表 event_instances）。
//!
//! 所有实例（challenge / AWD gamebox / AWDP gamebox）都挂在 `event_instances`
//! 根表上，各自经关联表（event_challenge_instance / event_gamebox_instances /
//! awdp_instances）挂靠。列表以 event_instances 为主查询，批量加载三张关联表
//! 分类 + 水合展示字段，内存分页（跨 family 无法用 SQL 分页）。
//! 刻意不返回 flag（管理端列表不再展示实例 flag）。

use std::{collections::HashMap, str::FromStr};

use sea_orm::entity::prelude::DateTimeWithTimeZone;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;

use crate::modules::event::jeopardy::api::InstancesDto;
use crate::{
    api::prelude::*,
    entity::{
        awd_event_gameboxes, awdp_instances, challenges, event_challenge_instance,
        event_gamebox_instances, event_instances, event_teams, events, gameboxes, users,
    },
};

/// 管理端统一实例条目（归一化视图）。
/// `instance_type` = "challenge" | "gamebox"。
#[derive(Debug, Serialize)]
pub struct AdminInstanceRow {
    pub id: Uuid,
    pub instance_type: String,
    pub status: String,
    pub identifier: String,
    pub event_id: Option<Uuid>,
    pub event_title: Option<String>,
    pub user_id: Option<Uuid>,
    pub user_name: Option<String>,
    pub team_id: Option<Uuid>,
    pub team_name: Option<String>,
    /// 对应的 title：challenge 名或 GameBox 名。
    pub content_title: Option<String>,
    pub challenge_id: Option<Uuid>,
    pub gamebox_id: Option<Uuid>,
    pub runtime_generation: Option<i64>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub destroy_at: Option<DateTimeWithTimeZone>,
}

/// 解析 FilterBar 过滤 token（与 apply_filters 同款：`key:value` 空格分隔、
/// `&`/`|` 为逻辑符、值可含空格）。
fn parse_filter_tokens(filter: &str) -> Vec<(String, String)> {
    let tokens: Vec<&str> = filter.split_whitespace().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if let Some(pos) = tokens[i].find(':') {
            let key = tokens[i][..pos].to_string();
            let mut value = tokens[i][pos + 1..].to_string();
            i += 1;
            while i < tokens.len() && tokens[i] != "&" && tokens[i] != "|" {
                value.push(' ');
                value.push_str(tokens[i]);
                i += 1;
            }
            out.push((key, value));
        } else {
            i += 1;
        }
    }
    out
}

fn uuid_or_nil(v: &str) -> Uuid {
    Uuid::from_str(v).unwrap_or(Uuid::nil())
}

/// GET /api/admin/instances —— 归一化实例列表：
/// 根表 event_instances 过滤 + 三张关联表分类，按 updated_at 倒序，内存分页。
#[get("")]
pub async fn get_instances(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<AdminInstanceRow>> {
    let mut query_params = query_params.0;
    let db = ctx.db.get_ref();

    let pairs = parse_filter_tokens(query_params.filter.as_deref().unwrap_or(""));

    // ── 1. 根表过滤（event_instances 列；family 专属键留到分类后内存过滤） ──
    let mut q = event_instances::Entity::find();
    let mut family_keys: Vec<(String, String)> = Vec::new();
    for (k, v) in &pairs {
        match k.as_str() {
            "id" => q = q.filter(event_instances::Column::Id.eq(uuid_or_nil(v))),
            "status" => q = q.filter(event_instances::Column::RuntimeState.contains(v.clone())),
            "identifier" | "ref" => {
                q = q.filter(event_instances::Column::ContainerName.contains(v.clone()))
            }
            "event_id" => q = q.filter(event_instances::Column::EventId.eq(uuid_or_nil(v))),
            "user_id" => q = q.filter(event_instances::Column::OwnerUserId.eq(uuid_or_nil(v))),
            "team_id" => q = q.filter(event_instances::Column::OwnerTeamId.eq(uuid_or_nil(v))),
            _ => family_keys.push((k.clone(), v.clone())),
        }
    }
    let root_rows = q
        .order_by_desc(event_instances::Column::UpdatedAt)
        .all(db)
        .await?;
    let mut rows = build_admin_rows(db, root_rows, &family_keys).await?;

    // ── 排序 + 内存分页（跨 family 无法 SQL 分页） ──
    rows.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let total: usize = rows.len();
    let total_u64 = total as u64;
    let page = query_params.page.unwrap_or(1).max(1);
    let limit = query_params.limit.unwrap_or(total_u64.max(1)).max(1);
    let start = ((page - 1) as usize) * (limit as usize);
    let page_rows = rows
        .into_iter()
        .skip(start)
        .take(limit as usize)
        .collect::<Vec<_>>();

    query_params.total = Some(total);
    UniResponse::ok_meta(page_rows.into(), query_params.into()).into()
}

/// GET /api/admin/events/{event_id}/instances —— 某赛事的实例列表（admin 赛事 Instance Tab）。
#[get("/{event_id}/instances")]
pub async fn get_event_instances(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    event_id: Path<Uuid>,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<AdminInstanceRow>> {
    let event_id = event_id.into_inner();
    let db = ctx.db.get_ref();

    let pairs = parse_filter_tokens(query_params.filter.as_deref().unwrap_or(""));
    let mut q =
        event_instances::Entity::find().filter(event_instances::Column::EventId.eq(event_id));
    let mut family_keys: Vec<(String, String)> = Vec::new();
    for (k, v) in &pairs {
        match k.as_str() {
            "id" => q = q.filter(event_instances::Column::Id.eq(uuid_or_nil(v))),
            "status" => q = q.filter(event_instances::Column::RuntimeState.contains(v.clone())),
            "identifier" | "ref" => {
                q = q.filter(event_instances::Column::ContainerName.contains(v.clone()))
            }
            "user_id" => q = q.filter(event_instances::Column::OwnerUserId.eq(uuid_or_nil(v))),
            "team_id" => q = q.filter(event_instances::Column::OwnerTeamId.eq(uuid_or_nil(v))),
            _ => family_keys.push((k.clone(), v.clone())),
        }
    }
    let root_rows = q
        .order_by_desc(event_instances::Column::UpdatedAt)
        .all(db)
        .await?;
    let rows = build_admin_rows(db, root_rows, &family_keys).await?;

    UniResponse::ok(Some(rows)).into()
}

/// 根据根表行批量加载关联表 + 水合展示字段 + 组装 AdminInstanceRow。
async fn build_admin_rows(
    db: &DatabaseConnection,
    root_rows: Vec<event_instances::Model>,
    family_keys: &[(String, String)],
) -> Result<Vec<AdminInstanceRow>, sea_orm::DbErr> {
    let ids: Vec<Uuid> = root_rows.iter().map(|r| r.id).collect();

    // ── 2. 三张关联表批量加载（分类） ──
    let challenge_map: HashMap<Uuid, event_challenge_instance::Model> = if ids.is_empty() {
        HashMap::new()
    } else {
        event_challenge_instance::Entity::find()
            .filter(event_challenge_instance::Column::Id.is_in(ids.clone()))
            .all(db)
            .await?
            .into_iter()
            .map(|m| (m.id, m))
            .collect()
    };
    let awd_map: HashMap<Uuid, event_gamebox_instances::Model> = if ids.is_empty() {
        HashMap::new()
    } else {
        event_gamebox_instances::Entity::find()
            .filter(event_gamebox_instances::Column::InstanceId.is_in(ids.clone()))
            .all(db)
            .await?
            .into_iter()
            .map(|m| (m.instance_id, m))
            .collect()
    };
    let awdp_map: HashMap<Uuid, awdp_instances::Model> = if ids.is_empty() {
        HashMap::new()
    } else {
        awdp_instances::Entity::find()
            .filter(awdp_instances::Column::InstanceId.is_in(ids.clone()))
            .all(db)
            .await?
            .into_iter()
            .map(|m| (m.instance_id, m))
            .collect()
    };

    // ── 3. 展示字段水合 ──
    let mut challenge_titles: HashMap<Uuid, String> = HashMap::new();
    let mut event_titles: HashMap<Uuid, String> = HashMap::new();
    let mut user_names: HashMap<Uuid, String> = HashMap::new();
    let mut team_names: HashMap<Uuid, String> = HashMap::new();

    let c_ids: Vec<Uuid> = challenge_map.values().map(|m| m.challenge_id).collect();
    if !c_ids.is_empty() {
        challenges::Entity::find()
            .filter(challenges::Column::Id.is_in(c_ids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|c| {
                challenge_titles.insert(c.id, c.name);
            });
    }

    // AWD：event_gamebox_id → (gamebox_id, name)。
    let eg_ids: Vec<Uuid> = awd_map.values().map(|m| m.event_gamebox_id).collect();
    let mut awd_gb_info: HashMap<Uuid, (Uuid, String)> = HashMap::new();
    if !eg_ids.is_empty() {
        let eg_rows = awd_event_gameboxes::Entity::find()
            .filter(awd_event_gameboxes::Column::Id.is_in(eg_ids))
            .all(db)
            .await?;
        let gb_ids: Vec<Uuid> = eg_rows.iter().map(|eg| eg.gamebox_id).collect();
        let gb_rows = gameboxes::Entity::find()
            .filter(gameboxes::Column::Id.is_in(gb_ids))
            .all(db)
            .await?;
        for eg in eg_rows {
            if let Some(gb) = gb_rows.iter().find(|g| g.id == eg.gamebox_id) {
                awd_gb_info.insert(eg.id, (eg.gamebox_id, gb.name.clone()));
            }
        }
    }

    // AWDP gamebox 名。
    let w_gids: Vec<Uuid> = awdp_map.values().map(|m| m.gamebox_id).collect();
    let mut awdp_gb_titles: HashMap<Uuid, String> = HashMap::new();
    if !w_gids.is_empty() {
        gameboxes::Entity::find()
            .filter(gameboxes::Column::Id.is_in(w_gids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|g| {
                awdp_gb_titles.insert(g.id, g.name);
            });
    }

    let e_ids: Vec<Uuid> = root_rows.iter().map(|r| r.event_id).collect();
    if !e_ids.is_empty() {
        events::Entity::find()
            .filter(events::Column::Id.is_in(e_ids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|e| {
                event_titles.insert(e.id, e.title);
            });
    }
    let u_ids: Vec<Uuid> = root_rows.iter().filter_map(|r| r.owner_user_id).collect();
    if !u_ids.is_empty() {
        users::Entity::find()
            .filter(users::Column::Id.is_in(u_ids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|u| {
                user_names.insert(u.id, u.nickname);
            });
    }
    let t_ids: Vec<Uuid> = root_rows.iter().filter_map(|r| r.owner_team_id).collect();
    if !t_ids.is_empty() {
        event_teams::Entity::find()
            .filter(event_teams::Column::Id.is_in(t_ids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|t| {
                team_names.insert(t.id, t.name);
            });
    }

    // ── 4. 分类 + 组装（challenge → awd → awdp；family 专属键内存过滤） ──
    let filter_gamebox_id = family_keys
        .iter()
        .find(|(k, _)| k == "gamebox_id")
        .map(|(_, v)| uuid_or_nil(v));
    let filter_challenge_id = family_keys
        .iter()
        .find(|(k, _)| k == "challenge_id")
        .map(|(_, v)| uuid_or_nil(v));

    let mut rows: Vec<AdminInstanceRow> = Vec::new();
    for root in root_rows {
        if let Some(m) = challenge_map.get(&root.id) {
            if let Some(cid) = filter_challenge_id {
                if m.challenge_id != cid {
                    continue;
                }
            }
            if filter_gamebox_id.is_some() {
                continue;
            }
            rows.push(AdminInstanceRow {
                id: root.id,
                instance_type: "challenge".into(),
                status: root.runtime_state.clone(),
                identifier: root.container_name.clone(),
                event_id: Some(root.event_id),
                event_title: event_titles.get(&root.event_id).cloned(),
                user_id: root.owner_user_id,
                user_name: root.owner_user_id.and_then(|u| user_names.get(&u).cloned()),
                team_id: root.owner_team_id,
                team_name: root.owner_team_id.and_then(|t| team_names.get(&t).cloned()),
                content_title: challenge_titles.get(&m.challenge_id).cloned(),
                challenge_id: Some(m.challenge_id),
                gamebox_id: None,
                runtime_generation: Some(root.runtime_generation),
                created_at: root.created_at,
                updated_at: root.updated_at,
                destroy_at: root.expires_at,
            });
        } else if let Some(m) = awd_map.get(&root.id) {
            let gb_info = awd_gb_info.get(&m.event_gamebox_id);
            if let Some(gid) = filter_gamebox_id {
                if gb_info.map(|(id, _)| *id) != Some(gid) {
                    continue;
                }
            }
            if filter_challenge_id.is_some() {
                continue;
            }
            rows.push(AdminInstanceRow {
                id: root.id,
                instance_type: "gamebox".into(),
                status: root.runtime_state.clone(),
                identifier: root.container_name.clone(),
                event_id: Some(root.event_id),
                event_title: event_titles.get(&root.event_id).cloned(),
                user_id: None,
                user_name: None,
                team_id: Some(m.team_id),
                team_name: team_names.get(&m.team_id).cloned(),
                content_title: gb_info.map(|(_, n)| n.clone()),
                challenge_id: None,
                gamebox_id: gb_info.map(|(id, _)| *id),
                runtime_generation: Some(root.runtime_generation),
                created_at: root.created_at,
                updated_at: root.updated_at,
                destroy_at: None,
            });
        } else if let Some(m) = awdp_map.get(&root.id) {
            if let Some(gid) = filter_gamebox_id {
                if m.gamebox_id != gid {
                    continue;
                }
            }
            if filter_challenge_id.is_some() {
                continue;
            }
            rows.push(AdminInstanceRow {
                id: root.id,
                instance_type: "gamebox".into(),
                status: root.runtime_state.clone(),
                identifier: root.container_name.clone(),
                event_id: Some(root.event_id),
                event_title: event_titles.get(&root.event_id).cloned(),
                user_id: m.owner_user_id,
                user_name: m.owner_user_id.and_then(|u| user_names.get(&u).cloned()),
                team_id: m.owner_team_id,
                team_name: m.owner_team_id.and_then(|t| team_names.get(&t).cloned()),
                content_title: awdp_gb_titles.get(&m.gamebox_id).cloned(),
                challenge_id: None,
                gamebox_id: Some(m.gamebox_id),
                runtime_generation: Some(root.runtime_generation),
                created_at: root.created_at,
                updated_at: root.updated_at,
                destroy_at: None,
            });
        }
    }

    Ok(rows)
}

/// GET /api/admin/instances/{instance_id} —— 实例详情（challenge 实例，含 flag 兼容）。
#[get("/{instance_id}")]
pub async fn get_instance(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    instance_id: Path<Uuid>,
) -> UniResult<InstancesDto> {
    let instance_id = instance_id.into_inner();
    let (model, runtime) = event_challenge_instance::Entity::find_by_id(instance_id)
        .find_also_related(event_instances::Entity)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", instance_id)))?;
    let runtime = runtime.ok_or(AppError::NotFound(format!(" {} not exist", instance_id)))?;

    UniResponse::ok(Some(InstancesDto::from_pair(&model, &runtime))).into()
}
