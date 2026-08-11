//! 管理端仪表盘汇总——`GET /api/admin/dashboard/summary`。
//!
//! 只读聚合运营人员一屏所需信息。

use std::collections::HashMap;

use actix_web::get;
use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::Serialize;
use uuid::Uuid;

use crate::api::{extractor::auth::SuperAdminJwtGuard, prelude::*};
use crate::entity::{
    announcements, awd_events, challenge_instances, challenges, discussions, events, gameboxes,
    jeopardy_challenge_solves, logs, scheduled_tasks, sea_orm_active_enums::AwdEventStatus, users,
    weapons,
};

/// AWD 状态机中表示“出问题、需要管理员介入”的异常态。
const ALERT_AWD_STATUSES: [AwdEventStatus; 4] = [
    AwdEventStatus::DeployFailed,
    AwdEventStatus::NetworkError,
    AwdEventStatus::VerificationFailed,
    AwdEventStatus::StartBlocked,
];

/// 枚举 → snake_case 字符串（实体枚举带 #[serde(rename_all = "snake_case")]）。
fn snake_str<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|x| x.as_str().map(str::to_owned))
        .unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// DTOs
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DashboardSummaryDto {
    pub stats: DashboardStatsDto,
    pub attention: DashboardAttentionDto,
    pub events: Vec<DashboardEventDto>,
    pub activity: DashboardActivityDto,
}

#[derive(Debug, Serialize)]
pub struct DashboardStatsDto {
    pub users: usize,
    pub events: usize,
    pub challenges: usize,
    pub weapons: usize,
    pub announcements: usize,
    pub discussions: usize,
    pub instances: usize,
    pub gameboxes: usize,
}

#[derive(Debug, Serialize)]
pub struct DashboardAttentionDto {
    pub failed_tasks: Vec<FailedTaskDto>,
    pub error_logs_24h: usize,
    pub awd_alerts: Vec<AwdAlertDto>,
}

#[derive(Debug, Serialize)]
pub struct FailedTaskDto {
    pub task_name: String,
    pub task_key: String,
    pub error_msg: Option<String>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub updated_at: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize)]
pub struct AwdAlertDto {
    pub event_id: Uuid,
    pub title: String,
    pub status: String,
    pub phase: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardEventDto {
    pub event_id: Uuid,
    pub title: String,
    pub family: String,
    pub purpose: String,
    pub participant_mode: String,
    pub start_time: DateTime<FixedOffset>,
    pub end_time: Option<DateTime<FixedOffset>>,
    pub hidden: bool,
    pub awd: Option<DashboardAwdDto>,
}

#[derive(Debug, Serialize)]
pub struct DashboardAwdDto {
    pub status: String,
    pub phase: String,
    pub started_at: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Serialize)]
pub struct DashboardActivityDto {
    pub recent_solves: Vec<SolveActivityDto>,
    pub recent_signups: Vec<SignupActivityDto>,
}

#[derive(Debug, Serialize)]
pub struct SolveActivityDto {
    pub nickname: String,
    pub avatar: Option<String>,
    pub challenge_name: String,
    pub solved_at: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize)]
pub struct SignupActivityDto {
    pub nickname: String,
    pub username: String,
    pub avatar: Option<String>,
    pub created_at: DateTime<FixedOffset>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler
// ─────────────────────────────────────────────────────────────────────────────

/// GET /api/admin/dashboard/summary
#[get("/summary")]
pub async fn get_dashboard_summary(
    _admin: SuperAdminJwtGuard,
    ctx: ReqCtx,
) -> UniResult<DashboardSummaryDto> {
    let db = ctx.db.get_ref();

    // ── 平台规模 ──
    let stats = DashboardStatsDto {
        users: users::Entity::find().count(db).await? as usize,
        events: events::Entity::find().count(db).await? as usize,
        challenges: challenges::Entity::find().count(db).await? as usize,
        weapons: weapons::Entity::find().count(db).await? as usize,
        announcements: announcements::Entity::find().count(db).await? as usize,
        discussions: discussions::Entity::find().count(db).await? as usize,
        instances: challenge_instances::Entity::find().count(db).await? as usize,
        gameboxes: gameboxes::Entity::find().count(db).await? as usize,
    };

    // ── 赛事（含 AWD 生命周期 join）──
    let event_rows = events::Entity::find()
        .order_by_desc(events::Column::StartTime)
        .limit(50)
        .all(db)
        .await?;
    let awd_map: HashMap<Uuid, awd_events::Model> = awd_events::Entity::find()
        .all(db)
        .await?
        .into_iter()
        .map(|m| (m.event_id, m))
        .collect();

    let events_dto: Vec<DashboardEventDto> = event_rows
        .into_iter()
        .map(|e| DashboardEventDto {
            event_id: e.id,
            title: e.title,
            family: snake_str(&e.family),
            purpose: snake_str(&e.purpose),
            participant_mode: snake_str(&e.participant_mode),
            start_time: e.start_time,
            end_time: e.end_time,
            hidden: e.hidden,
            awd: awd_map.get(&e.id).map(|a| DashboardAwdDto {
                status: snake_str(&a.status),
                phase: snake_str(&a.phase),
                started_at: a.started_at,
            }),
        })
        .collect();

    // ── 需处理事项 ──
    let failed_tasks = scheduled_tasks::Entity::find()
        .filter(scheduled_tasks::Column::Status.eq("failed"))
        .order_by_desc(scheduled_tasks::Column::UpdatedAt)
        .limit(10)
        .all(db)
        .await?
        .into_iter()
        .map(|t| FailedTaskDto {
            task_name: t.task_name,
            task_key: t.task_key,
            error_msg: t.error_msg,
            attempt_count: t.attempt_count,
            max_attempts: t.max_attempts,
            updated_at: t.updated_at,
        })
        .collect();

    let cutoff: DateTime<FixedOffset> = (Utc::now() - ChronoDuration::hours(24)).into();
    let error_logs_24h = logs::Entity::find()
        .filter(logs::Column::Level.eq("ERROR"))
        .filter(logs::Column::CreatedAt.gte(cutoff))
        .count(db)
        .await? as usize;

    let alert_rows = awd_events::Entity::find()
        .filter(awd_events::Column::Status.is_in(ALERT_AWD_STATUSES))
        .all(db)
        .await?;
    let mut awd_alerts = Vec::with_capacity(alert_rows.len());
    if !alert_rows.is_empty() {
        let ids: Vec<Uuid> = alert_rows.iter().map(|a| a.event_id).collect();
        let title_map: HashMap<Uuid, String> = events::Entity::find()
            .filter(events::Column::Id.is_in(ids))
            .all(db)
            .await?
            .into_iter()
            .map(|e| (e.id, e.title))
            .collect();
        awd_alerts = alert_rows
            .into_iter()
            .map(|a| AwdAlertDto {
                event_id: a.event_id,
                title: title_map
                    .get(&a.event_id)
                    .cloned()
                    .unwrap_or_else(|| "（赛事已删除）".to_string()),
                status: snake_str(&a.status),
                phase: snake_str(&a.phase),
            })
            .collect();
    }

    let attention = DashboardAttentionDto {
        failed_tasks,
        error_logs_24h,
        awd_alerts,
    };

    // ── 近期活动 ──
    let solve_rows = jeopardy_challenge_solves::Entity::find()
        .order_by_desc(jeopardy_challenge_solves::Column::CreatedAt)
        .limit(8)
        .all(db)
        .await?;

    let mut recent_solves = Vec::with_capacity(solve_rows.len());
    if !solve_rows.is_empty() {
        let user_ids: Vec<Uuid> = solve_rows.iter().map(|s| s.user_id).collect();
        let challenge_ids: Vec<Uuid> = solve_rows.iter().map(|s| s.challenge_id).collect();
        let user_map: HashMap<Uuid, users::Model> = users::Entity::find()
            .filter(users::Column::Id.is_in(user_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();
        let challenge_map: HashMap<Uuid, challenges::Model> = challenges::Entity::find()
            .filter(challenges::Column::Id.is_in(challenge_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|c| (c.id, c))
            .collect();
        recent_solves = solve_rows
            .into_iter()
            .map(|s| SolveActivityDto {
                nickname: user_map
                    .get(&s.user_id)
                    .map(|u| u.nickname.clone())
                    .unwrap_or_else(|| "已删除用户".to_string()),
                avatar: user_map.get(&s.user_id).and_then(|u| u.avatar.clone()),
                challenge_name: challenge_map
                    .get(&s.challenge_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| "已删除题目".to_string()),
                solved_at: s.created_at,
            })
            .collect();
    }

    let recent_signups = users::Entity::find()
        .order_by_desc(users::Column::CreatedAt)
        .limit(8)
        .all(db)
        .await?
        .into_iter()
        .map(|u| SignupActivityDto {
            nickname: u.nickname,
            username: u.username,
            avatar: u.avatar,
            created_at: u.created_at,
        })
        .collect();

    let summary = DashboardSummaryDto {
        stats,
        attention,
        events: events_dto,
        activity: DashboardActivityDto {
            recent_solves,
            recent_signups,
        },
    };

    UniResponse::ok(Some(summary)).into()
}
