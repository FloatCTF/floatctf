//! AWDP 赛事积分榜（player + admin 共用）。
//!
//! 按 `participant_mode` 聚合 `awdp_score_events`：
//! - Individual → 主体 = event_users（已注册用户），名称 = users.nickname；
//! - Team → 主体 = event_teams，名称 = event_teams.name。
//! 每主体分项 break / fix / total；按 total 降序排名（同分并列，下一名跳过）。

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Serialize;
use uuid::Uuid;

use crate::entity::{
    event_teams, event_users, events, sea_orm_active_enums::ParticipantMode, users,
};
use crate::modules::event::awdp::{AwdpError, AwdpResult, repo::score_repo};

/// 积分榜条目（与 AWD TeamScore 同构；主体为 user 或 team）。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpScoreRow {
    /// 主体 id（individual → user_id；team → team_id）。
    pub subject_id: Uuid,
    /// 主体名称（individual → users.nickname；team → event_teams.name）。
    pub subject_name: String,
    pub break_score: i64,
    pub fix_score: i64,
    pub total_score: i64,
    pub rank: u32,
}

/// 构建赛事官方积分榜（已注册主体全列出，0 分也上榜）。
pub async fn get_scoreboard(
    db: &DatabaseConnection,
    event: &events::Model,
) -> AwdpResult<Vec<AwdpScoreRow>> {
    let event_id = event.id;
    let agg = score_repo::scoreboard_aggregate(db, event_id).await?;
    let agg_map: std::collections::HashMap<(Option<Uuid>, Option<Uuid>), (i64, i64)> = agg
        .into_iter()
        .map(|(u, t, b, f)| ((u, t), (b, f)))
        .collect();

    // 主体清单（已注册者全列出，含 0 分）。
    let mut rows: Vec<AwdpScoreRow> = match event.participant_mode {
        ParticipantMode::Individual => {
            let users = event_users::Entity::find()
                .filter(event_users::Column::EventId.eq(event_id))
                .find_also_related(users::Entity)
                .all(db)
                .await
                .map_err(|e| AwdpError::Database(e.to_string()))?;
            users
                .into_iter()
                .map(|(eu, u)| {
                    let (b, f) = agg_map
                        .get(&(Some(eu.user_id), None))
                        .copied()
                        .unwrap_or((0, 0));
                    AwdpScoreRow {
                        subject_id: eu.user_id,
                        subject_name: u.map(|u| u.nickname).unwrap_or_default(),
                        break_score: b,
                        fix_score: f,
                        total_score: b + f,
                        rank: 0,
                    }
                })
                .collect()
        }
        ParticipantMode::Team => {
            let teams = event_teams::Entity::find()
                .filter(event_teams::Column::EventId.eq(event_id))
                .all(db)
                .await
                .map_err(|e| AwdpError::Database(e.to_string()))?;
            teams
                .into_iter()
                .map(|t| {
                    let (b, f) = agg_map.get(&(None, Some(t.id))).copied().unwrap_or((0, 0));
                    AwdpScoreRow {
                        subject_id: t.id,
                        subject_name: t.name,
                        break_score: b,
                        fix_score: f,
                        total_score: b + f,
                        rank: 0,
                    }
                })
                .collect()
        }
    };

    // 按 total 降序排名（同分并列）。
    rows.sort_by(|a, b| b.total_score.cmp(&a.total_score));
    let mut rank = 1u32;
    let mut prev: Option<i64> = None;
    for (i, r) in rows.iter_mut().enumerate() {
        if let Some(p) = prev {
            if r.total_score < p {
                rank = (i + 1) as u32;
            }
        }
        r.rank = rank;
        prev = Some(r.total_score);
    }
    Ok(rows)
}
