//! EventGameBox repository — 某场 AWD 赛事选择的 GameBox + 钉住的 Revision + 计分配置。
//!
//! 关键不变式：
//!   - Event 内一个 GameBox 只能有一个选择（UNIQUE(event_id, gamebox_id)）
//!   - `gamebox_revision_id` NOT NULL：Deploy/Reset/Recovery/Judge 只读 pinned revision
//!   - host_offset 决定 instance_ip，部署后禁改
//!   - 计分属于 Event × GameBox，与全局 GameBox 无关

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::{awd_event_gameboxes, gamebox_revisions, gameboxes};

pub async fn find_event_gameboxes_by_event(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Vec<awd_event_gameboxes::Model>, sea_orm::DbErr> {
    awd_event_gameboxes::Entity::find()
        .filter(awd_event_gameboxes::Column::EventId.eq(event_id))
        .order_by_asc(awd_event_gameboxes::Column::HostOffset)
        .all(db)
        .await
}

pub async fn find_event_gamebox_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<awd_event_gameboxes::Model>, sea_orm::DbErr> {
    awd_event_gameboxes::Entity::find_by_id(id).one(db).await
}

/// 某 Event 对某 GameBox 的选择（UNIQUE(event_id, gamebox_id)）。
pub async fn find_event_gamebox(
    db: &DatabaseConnection,
    event_id: Uuid,
    gamebox_id: Uuid,
) -> Result<Option<awd_event_gameboxes::Model>, sea_orm::DbErr> {
    awd_event_gameboxes::Entity::find()
        .filter(awd_event_gameboxes::Column::EventId.eq(event_id))
        .filter(awd_event_gameboxes::Column::GameboxId.eq(gamebox_id))
        .one(db)
        .await
}

/// 获取 EventGameBox 关联的全局 GameBox identity（显示名等）。
pub async fn find_gamebox_identity(
    db: &DatabaseConnection,
    gamebox_id: Uuid,
) -> Result<Option<gameboxes::Model>, sea_orm::DbErr> {
    gameboxes::Entity::find_by_id(gamebox_id).one(db).await
}

pub async fn find_revision(
    db: &DatabaseConnection,
    revision_id: Uuid,
) -> Result<Option<gamebox_revisions::Model>, sea_orm::DbErr> {
    gamebox_revisions::Entity::find_by_id(revision_id)
        .one(db)
        .await
}

/// EventGameBox + GameBox identity + pinned Revision 组合视图。
pub struct EventGameBoxDetail {
    pub event_gamebox: awd_event_gameboxes::Model,
    pub gamebox: gameboxes::Model,
    pub revision: gamebox_revisions::Model,
}

pub async fn find_event_gameboxes_detail(
    db: &DatabaseConnection,
    event_id: Uuid,
) -> Result<Vec<EventGameBoxDetail>, sea_orm::DbErr> {
    let mut out = Vec::new();
    for eg in find_event_gameboxes_by_event(db, event_id).await? {
        let gamebox = match find_gamebox_identity(db, eg.gamebox_id).await? {
            Some(g) => g,
            None => continue,
        };
        let revision = match find_revision(db, eg.gamebox_revision_id).await? {
            Some(r) => r,
            None => continue,
        };
        out.push(EventGameBoxDetail {
            event_gamebox: eg,
            gamebox,
            revision,
        });
    }
    Ok(out)
}

/// 创建 EventGameBox（赛事选择，必须 pin 一个 ready revision）。
#[allow(clippy::too_many_arguments)]
pub async fn create_event_gamebox(
    db: &DatabaseConnection,
    event_id: Uuid,
    gamebox_id: Uuid,
    gamebox_revision_id: Uuid,
    host_offset: i16,
    enabled: bool,
    hidden: bool,
    cpu_millis: i64,
    memory_bytes: i64,
    pids_limit: i64,
    healthcheck_override_json: Option<serde_json::Value>,
    judge_timeout_secs: Option<i32>,
    judge_retry_interval_secs: Option<i32>,
    break_points: i64,
    loss_points: i64,
    fix_points: i64,
    down_points: i64,
    first_bonus: i64,
) -> Result<awd_event_gameboxes::Model, sea_orm::DbErr> {
    let now = chrono::Utc::now().into();
    awd_event_gameboxes::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        gamebox_id: Set(gamebox_id),
        gamebox_revision_id: Set(gamebox_revision_id),
        host_offset: Set(host_offset),
        enabled: Set(enabled),
        hidden: Set(hidden),
        cpu_millis: Set(cpu_millis),
        memory_bytes: Set(memory_bytes),
        pids_limit: Set(pids_limit),
        healthcheck_override_json: Set(healthcheck_override_json),
        judge_timeout_secs: Set(judge_timeout_secs),
        judge_retry_interval_secs: Set(judge_retry_interval_secs),
        break_points: Set(break_points),
        loss_points: Set(loss_points),
        fix_points: Set(fix_points),
        down_points: Set(down_points),
        first_bonus: Set(first_bonus),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
}

/// 更新 EventGameBox（计分/资源/判题覆盖）。调用方负责：
/// 已部署的 host_offset 禁止修改；变更后必须 touch_configuration。
pub struct EventGameBoxPatch {
    pub enabled: Option<bool>,
    pub hidden: Option<bool>,
    pub cpu_millis: Option<i64>,
    pub memory_bytes: Option<i64>,
    pub pids_limit: Option<i64>,
    pub healthcheck_override_json: Option<Option<serde_json::Value>>,
    pub judge_timeout_secs: Option<Option<i32>>,
    pub judge_retry_interval_secs: Option<Option<i32>>,
    pub break_points: Option<i64>,
    pub loss_points: Option<i64>,
    pub fix_points: Option<i64>,
    pub down_points: Option<i64>,
    pub first_bonus: Option<i64>,
}

pub async fn update_event_gamebox(
    db: &DatabaseConnection,
    id: Uuid,
    patch: EventGameBoxPatch,
) -> Result<Option<awd_event_gameboxes::Model>, sea_orm::DbErr> {
    let current = match find_event_gamebox_by_id(db, id).await? {
        Some(m) => m,
        None => return Ok(None),
    };
    let mut active: awd_event_gameboxes::ActiveModel = awd_event_gameboxes::ActiveModel {
        id: Set(id),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    };
    if let Some(v) = patch.enabled {
        active.enabled = Set(v);
    }
    if let Some(v) = patch.hidden {
        active.hidden = Set(v);
    }
    if let Some(v) = patch.cpu_millis {
        active.cpu_millis = Set(v);
    }
    if let Some(v) = patch.memory_bytes {
        active.memory_bytes = Set(v);
    }
    if let Some(v) = patch.pids_limit {
        active.pids_limit = Set(v);
    }
    if let Some(v) = patch.healthcheck_override_json {
        active.healthcheck_override_json = Set(v);
    }
    if let Some(v) = patch.judge_timeout_secs {
        active.judge_timeout_secs = Set(v);
    }
    if let Some(v) = patch.judge_retry_interval_secs {
        active.judge_retry_interval_secs = Set(v);
    }
    if let Some(v) = patch.break_points {
        active.break_points = Set(v);
    }
    if let Some(v) = patch.loss_points {
        active.loss_points = Set(v);
    }
    if let Some(v) = patch.fix_points {
        active.fix_points = Set(v);
    }
    if let Some(v) = patch.down_points {
        active.down_points = Set(v);
    }
    if let Some(v) = patch.first_bonus {
        active.first_bonus = Set(v);
    }
    active.update(db).await?;
    Ok(Some(current))
}

/// 删除 EventGameBox（移除赛事选择）。被 Instance 引用时 DB 层 RESTRICT 拒绝。
pub async fn delete_event_gamebox(db: &DatabaseConnection, id: Uuid) -> Result<(), sea_orm::DbErr> {
    let _ = awd_event_gameboxes::Entity::delete_by_id(id)
        .exec(db)
        .await?;
    Ok(())
}
