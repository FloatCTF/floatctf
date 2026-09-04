//! AWDP 官方积分趋势（仅 Competition）。
//!
//! 与 Jeopardy 趋势同构：按 `awdp_score_events` 账本累计每个主体（用户/队伍）
//! 的 Break/Fix 得分，输出 `TrendItem { name, points: [{ time, score }] }`，
//! 前端可直接复用现有 TrendChart。

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, FixedOffset};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;
use uuid::Uuid;

use crate::entity::{
    awdp_score_events, event_teams, events,
    sea_orm_active_enums::{EventPurpose, ParticipantMode},
    users,
};
use crate::modules::event::awdp::{AwdpError, AwdpResult, repo::run_repo};

/// AWDP 积分趋势点（与 Jeopardy TrendChart 数据结构一致）。
#[derive(Debug, Clone, Serialize)]
pub struct TrendPoint {
    pub name: String,
    pub score: f64,
    pub time: DateTime<FixedOffset>,
}

/// AWDP 积分趋势条目。
#[derive(Debug, Clone, Serialize)]
pub struct TrendItem {
    pub name: String,
    pub points: Vec<TrendPoint>,
}

/// 构造每个主体的累计趋势点。
fn build_trend(
    subject_events: HashMap<Uuid, Vec<awdp_score_events::Model>>,
    name_of: impl Fn(&Uuid) -> String,
) -> Vec<TrendItem> {
    let mut all_times = BTreeSet::new();
    for rows in subject_events.values() {
        for r in rows {
            all_times.insert(r.created_at);
        }
    }

    let mut scores: HashMap<Uuid, i64> = HashMap::new();
    let mut trend_map: HashMap<Uuid, Vec<TrendPoint>> = HashMap::new();

    for &time in &all_times {
        for (&subject_id, rows) in &subject_events {
            let score = scores.entry(subject_id).or_insert(0);
            for r in rows.iter().filter(|r| r.created_at == time) {
                *score += r.delta;
            }
            trend_map.entry(subject_id).or_default().push(TrendPoint {
                name: String::new(),
                score: *score as f64,
                time,
            });
        }
    }

    let mut items: Vec<TrendItem> = subject_events
        .keys()
        .map(|subject_id| TrendItem {
            name: name_of(subject_id),
            points: trend_map.get(subject_id).cloned().unwrap_or_default(),
        })
        .collect();

    // 按最终总分降序，便于前端默认展示领先者。
    items.sort_by(|a, b| {
        let sa = a.points.last().map(|p| p.score).unwrap_or(0.0);
        let sb = b.points.last().map(|p| p.score).unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    items
}

pub async fn get_trend(
    db: &DatabaseConnection,
    event: &events::Model,
) -> AwdpResult<Vec<TrendItem>> {
    if event.purpose != EventPurpose::Competition {
        return Err(AwdpError::Validation(
            "UnsupportedForPurpose: practice has no official event trend".into(),
        ));
    }

    let runs = run_repo::list_for_event(db, event.id).await?;
    if runs.is_empty() {
        return Ok(Vec::new());
    }
    let run_ids: Vec<Uuid> = runs.iter().map(|r| r.id).collect();
    let score_events = awdp_score_events::Entity::find()
        .filter(awdp_score_events::Column::RunId.is_in(run_ids))
        .order_by_asc(awdp_score_events::Column::CreatedAt)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    match event.participant_mode {
        ParticipantMode::Individual => {
            let user_ids: Vec<Uuid> = score_events.iter().filter_map(|s| s.user_id).collect();
            let users_map: HashMap<Uuid, users::Model> = users::Entity::find()
                .filter(users::Column::Id.is_in(user_ids))
                .all(db)
                .await
                .map_err(|e| AwdpError::Database(e.to_string()))?
                .into_iter()
                .map(|u| (u.id, u))
                .collect();

            let mut subject_events: HashMap<Uuid, Vec<awdp_score_events::Model>> = HashMap::new();
            for s in score_events {
                if let Some(uid) = s.user_id {
                    subject_events.entry(uid).or_default().push(s);
                }
            }

            Ok(build_trend(subject_events, |uid| {
                users_map
                    .get(uid)
                    .map(|u| u.nickname.clone())
                    .unwrap_or_default()
            }))
        }
        ParticipantMode::Team => {
            let team_ids: Vec<Uuid> = score_events.iter().filter_map(|s| s.team_id).collect();
            let teams_map: HashMap<Uuid, event_teams::Model> = event_teams::Entity::find()
                .filter(event_teams::Column::Id.is_in(team_ids))
                .all(db)
                .await
                .map_err(|e| AwdpError::Database(e.to_string()))?
                .into_iter()
                .map(|t| (t.id, t))
                .collect();

            let mut subject_events: HashMap<Uuid, Vec<awdp_score_events::Model>> = HashMap::new();
            for s in score_events {
                if let Some(tid) = s.team_id {
                    subject_events.entry(tid).or_default().push(s);
                }
            }

            Ok(build_trend(subject_events, |tid| {
                teams_map
                    .get(tid)
                    .map(|t| t.name.clone())
                    .unwrap_or_default()
            }))
        }
    }
}
