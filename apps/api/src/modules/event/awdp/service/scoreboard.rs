//! AWDP 赛事积分榜（player + admin 共用）。
//!
//! 按 `participant_mode` 聚合 `awdp_score_events`：
//! - Individual → 主体 = event_users（已注册用户），名称 = users.nickname；
//! - Team → 主体 = event_teams，名称 = event_teams.name。
//! 每主体分项 break / fix / total；按 total 降序排名（同分并列，下一名跳过）。
//!
//! `get_scoreboard_detail`：选手端明细矩阵（Break 攻破状态 + Fix 每回合官方结果 +
//! 每题 fix 得分），供 Scoreboard 页展示"所有题目的情况"。

use chrono::{DateTime, FixedOffset};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::entity::{
    event_teams, event_users, events,
    sea_orm_active_enums::{AwdpEvaluationKind, AwdpEvaluationStatus, ParticipantMode},
    users,
};
use crate::modules::event::awdp::{
    AwdpError, AwdpResult,
    repo::{
        break_repo, evaluation_repo, event_gamebox_repo, instance_repo, round_repo, run_repo,
        score_repo,
    },
};

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

// ────────────────────────────────────────────────────────────────────────────
// 明细矩阵（选手端 Scoreboard 页：Break / Fix 全题目展示）
// ────────────────────────────────────────────────────────────────────────────

/// 明细矩阵中的一道题（索引与 rows 内数组对齐）。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpSbGameBox {
    pub id: Uuid,
    pub name: String,
    pub category: String,
}

/// 明细矩阵中的一轮（索引与 fix_round_status 内层数组对齐）。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpSbRound {
    pub sequence: i32,
    /// round 状态：pending / evaluating / completed。
    pub status: String,
    pub cutoff_at: DateTime<FixedOffset>,
}

/// 明细矩阵行（与聚合榜同序，追加每题明细）。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpSbRow {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub rank: u32,
    pub break_score: i64,
    pub fix_score: i64,
    pub total_score: i64,
    /// 当前登录用户/队伍（自己的行高亮）。
    pub is_me: bool,
    /// 每题是否已攻破（对齐 gameboxes）。
    pub break_status: Vec<bool>,
    /// 每题 fix 实际计分（score events 权威；对齐 gameboxes）。
    pub fix_gamebox_score: Vec<i64>,
    /// 每题 × 每回合官方评估终态（[gamebox][round]；无实例/未评估 = None）。
    pub fix_round_status: Vec<Vec<Option<AwdpEvaluationStatus>>>,
}

/// Scoreboard 明细：汇总行 + 题目 + 回合 + 每题矩阵。
#[derive(Debug, Clone, Serialize)]
pub struct AwdpScoreboardDetail {
    /// individual / team（前端按赛制区分"人数"/"队伍数"文案）。
    pub participant_mode: ParticipantMode,
    pub gameboxes: Vec<AwdpSbGameBox>,
    pub rounds: Vec<AwdpSbRound>,
    pub rows: Vec<AwdpSbRow>,
}

/// 构建明细矩阵（展示 run：active 优先否则最新；无 run → 空明细）。
/// `me_user_id` / `me_team_id`：当前登录主体（用于 is_me 高亮；未知传 None）。
pub async fn get_scoreboard_detail(
    db: &DatabaseConnection,
    event: &events::Model,
    me_user_id: Option<Uuid>,
    me_team_id: Option<Uuid>,
) -> AwdpResult<AwdpScoreboardDetail> {
    let event_id = event.id;
    let rows = get_scoreboard(db, event).await?;

    // 题目集合：当前挂载（hidden 排除）∪ 本场 run 实际运行的题。
    // 注意：detach_gamebox 只删挂载行，实例/break/计分仍在——中途 detach 的题
    // 也必须在积分榜如实展示，否则已发生的 break 计数会"消失"（用户反馈）。
    let mut gameboxes: Vec<AwdpSbGameBox> = Vec::new();
    let mut gamebox_index: HashMap<Uuid, usize> = HashMap::new();
    for eg in event_gamebox_repo::list_for_event(db, event_id).await? {
        if eg.hidden {
            continue;
        }
        if gamebox_index.contains_key(&eg.gamebox_id) {
            continue;
        }
        let gb = event_gamebox_repo::find_gamebox_identity(db, eg.gamebox_id).await?;
        gamebox_index.insert(eg.gamebox_id, gameboxes.len());
        gameboxes.push(AwdpSbGameBox {
            id: eg.gamebox_id,
            name: gb.name,
            category: gb.category,
        });
    }

    // 无 run（未开始）→ 空明细（页面显示空态）。
    let Some(run) = run_repo::find_display_run_for_event(db, event_id).await? else {
        return Ok(AwdpScoreboardDetail {
            participant_mode: event.participant_mode.clone(),
            gameboxes,
            rounds: Vec::new(),
            rows: Vec::new(),
        });
    };

    // 补充 run 实例涉及的题（已不在挂载列表，如 detach 残留）。
    for (_inst, ext) in instance_repo::list_for_run(db, run.id).await? {
        if gamebox_index.contains_key(&ext.gamebox_id) {
            continue;
        }
        let gb = event_gamebox_repo::find_gamebox_identity(db, ext.gamebox_id).await?;
        gamebox_index.insert(ext.gamebox_id, gameboxes.len());
        gameboxes.push(AwdpSbGameBox {
            id: ext.gamebox_id,
            name: gb.name,
            category: gb.category,
        });
    }

    // 回合（索引对齐）。
    let rounds = round_repo::list_for_run(db, run.id).await?;
    let round_index: HashMap<Uuid, usize> =
        rounds.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
    let round_dtos: Vec<AwdpSbRound> = rounds
        .iter()
        .map(|r| AwdpSbRound {
            sequence: r.sequence,
            status: r.status.clone(),
            cutoff_at: r.cutoff_at,
        })
        .collect();

    // Break 攻破状态：subject × gamebox。
    let mut broken: HashSet<(Option<Uuid>, Option<Uuid>, Uuid)> = HashSet::new();
    for b in break_repo::list_for_run(db, run.id).await? {
        broken.insert((b.user_id, b.team_id, b.gamebox_id));
    }

    // 官方评估终态：subject × gamebox × round（manual 不计入榜单）。
    let mut eval_map: HashMap<(Option<Uuid>, Option<Uuid>, Uuid, usize), AwdpEvaluationStatus> =
        HashMap::new();
    for (ev, ext, _inst) in evaluation_repo::list_for_run_with_instances(db, run.id).await? {
        if ev.kind != AwdpEvaluationKind::Official {
            continue;
        }
        let Some(rid) = ev.fix_round_id else {
            continue;
        };
        let Some(&ri) = round_index.get(&rid) else {
            continue;
        };
        eval_map.insert(
            (ext.owner_user_id, ext.owner_team_id, ext.gamebox_id, ri),
            ev.status.clone(),
        );
    }

    // 每题 fix 得分（score events 权威计分）。
    let mut fix_score: HashMap<(Option<Uuid>, Option<Uuid>, Uuid), i64> = HashMap::new();
    for (u, t, gb, total) in score_repo::fix_score_by_gamebox(db, run.id).await? {
        fix_score.insert((u, t, gb), total);
    }

    let gamebox_ids: Vec<Uuid> = gameboxes.iter().map(|g| g.id).collect();
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        // 主体键：individual → (user, None)；team → (None, team)。
        let (uid, tid) = match event.participant_mode {
            ParticipantMode::Individual => (Some(r.subject_id), None),
            ParticipantMode::Team => (None, Some(r.subject_id)),
        };
        let is_me = match event.participant_mode {
            ParticipantMode::Individual => Some(r.subject_id) == me_user_id,
            ParticipantMode::Team => Some(r.subject_id) == me_team_id,
        };
        let mut break_status = Vec::with_capacity(gamebox_ids.len());
        let mut fix_gamebox_score = Vec::with_capacity(gamebox_ids.len());
        let mut fix_round_status = Vec::with_capacity(gamebox_ids.len());
        for gid in &gamebox_ids {
            break_status.push(broken.contains(&(uid, tid, *gid)));
            fix_gamebox_score.push(fix_score.get(&(uid, tid, *gid)).copied().unwrap_or(0));
            let per_round: Vec<Option<AwdpEvaluationStatus>> = round_dtos
                .iter()
                .enumerate()
                .map(|(ri, _)| eval_map.get(&(uid, tid, *gid, ri)).cloned())
                .collect();
            fix_round_status.push(per_round);
        }
        out.push(AwdpSbRow {
            subject_id: r.subject_id,
            subject_name: r.subject_name,
            rank: r.rank,
            break_score: r.break_score,
            fix_score: r.fix_score,
            total_score: r.total_score,
            is_me,
            break_status,
            fix_gamebox_score,
            fix_round_status,
        });
    }
    Ok(AwdpScoreboardDetail {
        participant_mode: event.participant_mode.clone(),
        gameboxes,
        rounds: round_dtos,
        rows: out,
    })
}
