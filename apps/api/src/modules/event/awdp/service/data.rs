//! AWDP 管理端大屏数据聚合。
//!
//! 为 admin Data Present 页提供一次取回所需的全部 AWDP 大屏数据：
//! 赛事基本信息、人数/队伍数、题目维度 Break/Fix 统计、Top10 积分榜、
//! 得分趋势、最近计分动态。

use chrono::{DateTime, FixedOffset};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

use crate::entity::{
    awdp_score_events, event_teams, event_users, events, gameboxes,
    sea_orm_active_enums::{AwdpEvaluationStatus, ParticipantMode},
    users,
};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    repo::run_repo,
    service::{
        scoreboard::{self, AwdpScoreRow},
        trend,
    },
};

/// 大屏中一道 GameBox 的统计卡。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpDataGameBox {
    pub id: Uuid,
    pub name: String,
    pub category: String,
    /// 攻破该题的主体数（Individual=人数 / Team=队伍数）。
    pub break_count: u64,
    /// 至少一轮官方 check PATCHED 的主体数。
    pub fix_count: u64,
}

/// 大屏侧栏最近计分动态（break/fix 账本）。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpDataActivity {
    pub subject_name: String,
    pub gamebox_name: String,
    pub gamebox_category: String,
    pub action: String,
    pub delta: i64,
    pub created_at: DateTime<FixedOffset>,
}

/// 管理端 AWDP Data Present 聚合响应。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpDataPresent {
    pub event: events::Model,
    pub user_count: u64,
    pub team_count: u64,
    pub gameboxes: Vec<AwdpDataGameBox>,
    pub scoreboard_top10: Vec<AwdpScoreRow>,
    pub trend: Vec<trend::TrendItem>,
    pub recent_activity: Vec<AwdpDataActivity>,
}

/// 构建管理端大屏数据。
pub async fn get_data_present(
    db: &DatabaseConnection,
    event: &events::Model,
) -> AwdpResult<AwdpDataPresent> {
    let event_id = event.id;

    let user_count = event_users::Entity::find()
        .filter(event_users::Column::EventId.eq(event_id))
        .count(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    let team_count = if event.participant_mode == ParticipantMode::Team {
        event_teams::Entity::find()
            .filter(event_teams::Column::EventId.eq(event_id))
            .count(db)
            .await
            .map_err(|e| AwdpError::Database(e.to_string()))?
    } else {
        0
    };

    // 使用 scoreboard detail 统一处理 hidden 排除 + detach 残留题在榜。
    let detail = scoreboard::get_scoreboard_detail(db, event, None, None).await?;
    let mut gameboxes = Vec::with_capacity(detail.gameboxes.len());
    for (gi, gb) in detail.gameboxes.iter().enumerate() {
        let break_count = detail
            .rows
            .iter()
            .filter(|r| r.break_status.get(gi).copied().unwrap_or(false))
            .count() as u64;
        let fix_count = detail
            .rows
            .iter()
            .filter(|r| {
                r.fix_round_status
                    .get(gi)
                    .map(|rounds| {
                        rounds
                            .iter()
                            .any(|s| matches!(s, Some(AwdpEvaluationStatus::Patched)))
                    })
                    .unwrap_or(false)
            })
            .count() as u64;
        gameboxes.push(AwdpDataGameBox {
            id: gb.id,
            name: gb.name.clone(),
            category: gb.category.clone(),
            break_count,
            fix_count,
        });
    }

    let scoreboard = scoreboard::get_scoreboard(db, event).await?;
    let scoreboard_top10 = scoreboard.into_iter().take(10).collect::<Vec<_>>();
    let trend = trend::get_trend(db, event).await?;
    let recent_activity = load_recent_activity(db, event).await?;

    Ok(AwdpDataPresent {
        event: event.clone(),
        user_count,
        team_count,
        gameboxes,
        scoreboard_top10,
        trend,
        recent_activity,
    })
}

/// 最近计分动态：取 awdp_score_events 最新 15 条，并补主体/题目名称。
async fn load_recent_activity(
    db: &DatabaseConnection,
    event: &events::Model,
) -> AwdpResult<Vec<AwdpDataActivity>> {
    let runs = run_repo::list_for_event(db, event.id).await?;
    if runs.is_empty() {
        return Ok(Vec::new());
    }
    let run_ids: Vec<Uuid> = runs.iter().map(|r| r.id).collect();
    let rows = awdp_score_events::Entity::find()
        .filter(awdp_score_events::Column::RunId.is_in(run_ids))
        .order_by_desc(awdp_score_events::Column::CreatedAt)
        .limit(15)
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?;

    let user_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.user_id).collect();
    let team_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.team_id).collect();
    let gamebox_ids: Vec<Uuid> = rows.iter().map(|r| r.gamebox_id).collect();

    let users_map: HashMap<Uuid, users::Model> = users::Entity::find()
        .filter(users::Column::Id.is_in(user_ids))
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .into_iter()
        .map(|u| (u.id, u))
        .collect();
    let teams_map: HashMap<Uuid, event_teams::Model> = event_teams::Entity::find()
        .filter(event_teams::Column::Id.is_in(team_ids))
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .into_iter()
        .map(|t| (t.id, t))
        .collect();
    let gameboxes_map: HashMap<Uuid, gameboxes::Model> = gameboxes::Entity::find()
        .filter(gameboxes::Column::Id.is_in(gamebox_ids))
        .all(db)
        .await
        .map_err(|e| AwdpError::Database(e.to_string()))?
        .into_iter()
        .map(|g| (g.id, g))
        .collect();

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let subject_name = match (r.user_id, r.team_id) {
            (Some(uid), None) => users_map
                .get(&uid)
                .map(|u| u.nickname.clone())
                .unwrap_or_default(),
            (None, Some(tid)) => teams_map
                .get(&tid)
                .map(|t| t.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let gb = gameboxes_map.get(&r.gamebox_id);
        out.push(AwdpDataActivity {
            subject_name,
            gamebox_name: gb.map(|g| g.name.clone()).unwrap_or_default(),
            gamebox_category: gb.map(|g| g.category.clone()).unwrap_or_default(),
            action: if r.score_type == "break" {
                "break".to_string()
            } else {
                "fix".to_string()
            },
            delta: r.delta,
            created_at: r.created_at,
        });
    }
    Ok(out)
}
