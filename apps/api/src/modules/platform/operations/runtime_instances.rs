use std::{collections::HashMap, str::FromStr};

use sea_orm::entity::prelude::DateTimeWithTimeZone;
use sea_orm::{Condition, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;

use crate::modules::event::jeopardy::api::InstancesDto;
use crate::{
    api::prelude::*,
    entity::{
        awd_event_gameboxes, awd_gamebox_instances, awdp_instances, challenge_instances,
        challenges, event_teams, events, gameboxes, instances,
        sea_orm_active_enums::{GameboxStatus, InstanceStatus},
        users,
    },
};

/// 管理端统一实例条目（归一化视图）。
/// `instance_type` = "challenge" | "gamebox"：
/// - challenge：jeopardy challenge 实例（legacy 自包含表，尚未迁移到 instances）
/// - gamebox：AWD / AWDP GameBox 实例（AWDP 已归一化到 instances + awdp_instances）
/// 列表刻意不返回 flag（管理端不再展示实例 flag）。
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

/// ActiveEnum 序列化为 string_value（如 InstanceStatus::Running -> "running"）。
fn enum_str<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|s| s.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// challenge_instances 侧过滤（gamebox_id / team_id 只属于 gamebox 侧 → 恒 false）。
fn challenge_cond(pairs: &[(String, String)]) -> Condition {
    let mut c = Condition::all();
    for (k, v) in pairs {
        match k.as_str() {
            "id" => c = c.add(challenge_instances::Column::Id.eq(uuid_or_nil(v))),
            "status" => {
                c = c.add(
                    challenge_instances::Column::Status
                        .eq(serde_json::from_str(v).unwrap_or(InstanceStatus::Running)),
                )
            }
            "identifier" | "ref" => {
                c = c.add(challenge_instances::Column::Identifier.contains(v.clone()))
            }
            "event_id" => c = c.add(challenge_instances::Column::EventId.eq(uuid_or_nil(v))),
            "challenge_id" => {
                c = c.add(challenge_instances::Column::ChallengeId.eq(uuid_or_nil(v)))
            }
            "user_id" => c = c.add(challenge_instances::Column::UserId.eq(uuid_or_nil(v))),
            "gamebox_id" | "team_id" => c = c.add(challenge_instances::Column::Id.eq(Uuid::nil())),
            _ => {}
        }
    }
    c
}

/// awd_gamebox_instances 侧过滤（gamebox_id 经 event_gamebox 解析；user_id 恒 false）。
fn awd_cond(pairs: &[(String, String)], eg_ids: &[Uuid]) -> Condition {
    let mut c = Condition::all();
    for (k, v) in pairs {
        match k.as_str() {
            "id" => c = c.add(awd_gamebox_instances::Column::Id.eq(uuid_or_nil(v))),
            "status" => {
                c = c.add(
                    awd_gamebox_instances::Column::Status
                        .eq(serde_json::from_str(v).unwrap_or(GameboxStatus::Pending)),
                )
            }
            "identifier" | "ref" => {
                c = c.add(awd_gamebox_instances::Column::ContainerName.contains(v.clone()))
            }
            "event_id" => c = c.add(awd_gamebox_instances::Column::EventId.eq(uuid_or_nil(v))),
            "team_id" => c = c.add(awd_gamebox_instances::Column::TeamId.eq(uuid_or_nil(v))),
            "gamebox_id" => {
                c = if eg_ids.is_empty() {
                    c.add(awd_gamebox_instances::Column::Id.eq(Uuid::nil()))
                } else {
                    c.add(awd_gamebox_instances::Column::EventGameboxId.is_in(eg_ids.to_vec()))
                }
            }
            "challenge_id" | "user_id" => {
                c = c.add(awd_gamebox_instances::Column::Id.eq(Uuid::nil()))
            }
            _ => {}
        }
    }
    c
}

/// AWDP（instances + awdp_instances）侧过滤。
fn awdp_cond(pairs: &[(String, String)]) -> Condition {
    let mut c = Condition::all();
    for (k, v) in pairs {
        match k.as_str() {
            "id" => c = c.add(awdp_instances::Column::InstanceId.eq(uuid_or_nil(v))),
            "event_id" => c = c.add(awdp_instances::Column::EventId.eq(uuid_or_nil(v))),
            "team_id" => c = c.add(awdp_instances::Column::OwnerTeamId.eq(uuid_or_nil(v))),
            "user_id" => c = c.add(awdp_instances::Column::OwnerUserId.eq(uuid_or_nil(v))),
            "gamebox_id" => c = c.add(awdp_instances::Column::GameboxId.eq(uuid_or_nil(v))),
            "challenge_id" => c = c.add(awdp_instances::Column::InstanceId.eq(Uuid::nil())),
            _ => {}
        }
    }
    c
}

/// GET /api/admin/instances —— 归一化实例列表：
/// challenge 实例 + AWD/AWDP gamebox 实例合并，按 updated_at 倒序，内存分页。
/// 不返回 flag（管理端列表不再展示实例 flag）。
#[get("")]
pub async fn get_instances(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<AdminInstanceRow>> {
    let mut query_params = query_params.0;
    let db = ctx.db.get_ref();

    let pairs = parse_filter_tokens(query_params.filter.as_deref().unwrap_or(""));
    let mut rows: Vec<AdminInstanceRow> = Vec::new();

    // ── 1. challenge（jeopardy）实例 ────────────────────────────────
    let c_items = challenge_instances::Entity::find()
        .filter(challenge_cond(&pairs))
        .order_by_desc(challenge_instances::Column::UpdatedAt)
        .all(db)
        .await?;

    let mut challenge_titles: HashMap<Uuid, String> = HashMap::new();
    let mut event_titles: HashMap<Uuid, String> = HashMap::new();
    let mut user_names: HashMap<Uuid, String> = HashMap::new();
    let mut team_names: HashMap<Uuid, String> = HashMap::new();

    if !c_items.is_empty() {
        let cids: Vec<Uuid> = c_items.iter().map(|i| i.challenge_id).collect();
        challenges::Entity::find()
            .filter(challenges::Column::Id.is_in(cids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|c| {
                challenge_titles.insert(c.id, c.name);
            });

        let eids: Vec<Uuid> = c_items.iter().map(|i| i.event_id).collect();
        events::Entity::find()
            .filter(events::Column::Id.is_in(eids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|e| {
                event_titles.insert(e.id, e.title);
            });

        let uids: Vec<Uuid> = c_items.iter().map(|i| i.user_id).collect();
        users::Entity::find()
            .filter(users::Column::Id.is_in(uids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|u| {
                user_names.insert(u.id, u.nickname);
            });
    }

    for it in c_items {
        rows.push(AdminInstanceRow {
            id: it.id,
            instance_type: "challenge".into(),
            status: enum_str(&it.status),
            identifier: it.identifier.clone(),
            event_id: Some(it.event_id),
            event_title: event_titles.get(&it.event_id).cloned(),
            user_id: Some(it.user_id),
            user_name: user_names.get(&it.user_id).cloned(),
            team_id: it.team_id,
            team_name: None,
            content_title: challenge_titles.get(&it.challenge_id).cloned(),
            challenge_id: Some(it.challenge_id),
            gamebox_id: None,
            runtime_generation: None,
            created_at: it.created_at,
            updated_at: it.updated_at,
            destroy_at: Some(it.destroy_at),
        });
    }

    // ── 2. AWD gamebox 实例 ────────────────────────────────────────
    // gamebox_id 过滤先解析成 awd_event_gameboxes.id 集合。
    let mut awd_eg_ids: Vec<Uuid> = Vec::new();
    if let Some((_, gv)) = pairs.iter().find(|(k, _)| k == "gamebox_id") {
        awd_eg_ids = awd_event_gameboxes::Entity::find()
            .filter(awd_event_gameboxes::Column::GameboxId.eq(uuid_or_nil(gv)))
            .all(db)
            .await?
            .into_iter()
            .map(|eg| eg.id)
            .collect();
    }

    let a_items = awd_gamebox_instances::Entity::find()
        .filter(awd_cond(&pairs, &awd_eg_ids))
        .order_by_desc(awd_gamebox_instances::Column::UpdatedAt)
        .all(db)
        .await?;

    if !a_items.is_empty() {
        let eids: Vec<Uuid> = a_items.iter().map(|i| i.event_id).collect();
        events::Entity::find()
            .filter(events::Column::Id.is_in(eids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|e| {
                event_titles.insert(e.id, e.title);
            });

        let tids: Vec<Uuid> = a_items.iter().map(|i| i.team_id).collect();
        event_teams::Entity::find()
            .filter(event_teams::Column::Id.is_in(tids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|t| {
                team_names.insert(t.id, t.name);
            });
    }

    let mut awd_gb_info: HashMap<Uuid, (Uuid, String)> = HashMap::new();
    if !a_items.is_empty() {
        let eg_ids: Vec<Uuid> = a_items.iter().map(|i| i.event_gamebox_id).collect();
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

    for it in a_items {
        let gb_info = awd_gb_info.get(&it.event_gamebox_id);
        rows.push(AdminInstanceRow {
            id: it.id,
            instance_type: "gamebox".into(),
            status: enum_str(&it.status),
            identifier: it.container_name.clone(),
            event_id: Some(it.event_id),
            event_title: event_titles.get(&it.event_id).cloned(),
            user_id: None,
            user_name: None,
            team_id: Some(it.team_id),
            team_name: team_names.get(&it.team_id).cloned(),
            content_title: gb_info.map(|(_, n)| n.clone()),
            challenge_id: None,
            gamebox_id: gb_info.map(|(id, _)| *id),
            runtime_generation: Some(it.runtime_generation),
            created_at: it.created_at,
            updated_at: it.updated_at,
            destroy_at: None,
        });
    }

    // ── 3. AWDP gamebox 实例（instances + awdp_instances 归一化关联） ──
    // status/identifier 过滤针对 instances 表：先解析出实例 id 集合再过滤。
    let has_i_filter = pairs
        .iter()
        .any(|(k, _)| matches!(k.as_str(), "status" | "identifier" | "ref"));
    let mut awdp_iids: Option<Vec<Uuid>> = None;
    if has_i_filter {
        let mut s = instances::Entity::find();
        for (k, v) in &pairs {
            match k.as_str() {
                "status" => s = s.filter(instances::Column::RuntimeState.contains(v.clone())),
                "identifier" | "ref" => {
                    s = s.filter(instances::Column::ContainerName.contains(v.clone()))
                }
                _ => {}
            }
        }
        awdp_iids = Some(s.all(db).await?.into_iter().map(|i| i.id).collect());
    }
    let mut w_cond = awdp_cond(&pairs);
    if let Some(ids) = &awdp_iids {
        w_cond = if ids.is_empty() {
            w_cond.add(awdp_instances::Column::InstanceId.eq(Uuid::nil()))
        } else {
            w_cond.add(awdp_instances::Column::InstanceId.is_in(ids.clone()))
        };
    }
    let w_exts = awdp_instances::Entity::find()
        .filter(w_cond)
        .order_by_desc(awdp_instances::Column::CreatedAt)
        .all(db)
        .await?;

    let mut awdp_instances_map: HashMap<Uuid, instances::Model> = HashMap::new();
    let mut awdp_gb_titles: HashMap<Uuid, String> = HashMap::new();
    if !w_exts.is_empty() {
        let iids: Vec<Uuid> = w_exts.iter().map(|e| e.instance_id).collect();
        instances::Entity::find()
            .filter(instances::Column::Id.is_in(iids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|i| {
                awdp_instances_map.insert(i.id, i);
            });

        let gids: Vec<Uuid> = w_exts.iter().map(|e| e.gamebox_id).collect();
        gameboxes::Entity::find()
            .filter(gameboxes::Column::Id.is_in(gids))
            .all(db)
            .await?
            .into_iter()
            .for_each(|g| {
                awdp_gb_titles.insert(g.id, g.name);
            });

        let uids: Vec<Uuid> = w_exts.iter().filter_map(|e| e.owner_user_id).collect();
        if !uids.is_empty() {
            users::Entity::find()
                .filter(users::Column::Id.is_in(uids))
                .all(db)
                .await?
                .into_iter()
                .for_each(|u| {
                    user_names.insert(u.id, u.nickname);
                });
        }

        let tids: Vec<Uuid> = w_exts.iter().filter_map(|e| e.owner_team_id).collect();
        if !tids.is_empty() {
            event_teams::Entity::find()
                .filter(event_teams::Column::Id.is_in(tids))
                .all(db)
                .await?
                .into_iter()
                .for_each(|t| {
                    team_names.insert(t.id, t.name);
                });
        }

        let eids: Vec<Uuid> = w_exts.iter().filter_map(|e| e.event_id).collect();
        if !eids.is_empty() {
            events::Entity::find()
                .filter(events::Column::Id.is_in(eids))
                .all(db)
                .await?
                .into_iter()
                .for_each(|e| {
                    event_titles.insert(e.id, e.title);
                });
        }
    }

    for ext in w_exts {
        let Some(instance) = awdp_instances_map.get(&ext.instance_id) else {
            continue;
        };
        rows.push(AdminInstanceRow {
            id: instance.id,
            instance_type: "gamebox".into(),
            status: instance.runtime_state.clone(),
            identifier: instance.container_name.clone(),
            event_id: ext.event_id,
            event_title: ext.event_id.and_then(|e| event_titles.get(&e).cloned()),
            user_id: ext.owner_user_id,
            user_name: ext.owner_user_id.and_then(|u| user_names.get(&u).cloned()),
            team_id: ext.owner_team_id,
            team_name: ext.owner_team_id.and_then(|t| team_names.get(&t).cloned()),
            content_title: awdp_gb_titles.get(&ext.gamebox_id).cloned(),
            challenge_id: None,
            gamebox_id: Some(ext.gamebox_id),
            runtime_generation: Some(instance.runtime_generation),
            created_at: instance.created_at,
            updated_at: instance.updated_at,
            destroy_at: None,
        });
    }

    // ── 合并排序 + 内存分页（跨三表无法用 SQL 分页） ──
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

/// GET /api/admin/instances/{instance_id} —— challenge 实例详情（legacy）。
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
