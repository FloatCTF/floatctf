//! AWD 计分服务。

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::ScoreEventType;
use crate::modules::event::awd::{
    AwdError, AwdResult,
    domain::{IdempotencyKey, TeamScore},
    repo::score_repo,
};

/// 从账本聚合得到赛事当前积分榜。
pub async fn get_scoreboard(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_names: &[(Uuid, String)],
) -> AwdResult<Vec<TeamScore>> {
    let mut scores = Vec::new();

    for (team_id, team_name) in team_names {
        let attack = score_repo::team_score_for_types(
            db,
            event_id,
            *team_id,
            &[ScoreEventType::Attack, ScoreEventType::FirstBonus],
        )
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

        // Defense: SLA fix rewards and down penalties (and victim losses on own boxes).
        let defense = score_repo::team_score_for_types(
            db,
            event_id,
            *team_id,
            &[
                ScoreEventType::JudgeFix,
                ScoreEventType::JudgeDown,
                ScoreEventType::VictimLoss,
            ],
        )
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

        let penalties = score_repo::team_score_for_types(
            db,
            event_id,
            *team_id,
            &[ScoreEventType::ResetPenalty, ScoreEventType::Adjustment],
        )
        .await
        .map_err(|e| AwdError::Database(e.to_string()))?;

        let total = attack + defense + penalties;
        scores.push(TeamScore {
            team_id: *team_id,
            team_name: team_name.clone(),
            attack_score: attack,
            defense_score: defense,
            total_score: total,
            rank: 0, // assigned after sorting
        });
    }

    // Sort by total_score descending, assign ranks
    scores.sort_by(|a, b| b.total_score.cmp(&a.total_score));

    let mut rank = 1u32;
    let mut prev_score = None;
    for (i, s) in scores.iter_mut().enumerate() {
        if let Some(prev) = prev_score {
            if s.total_score < prev {
                rank = (i + 1) as u32;
            }
        }
        s.rank = rank;
        prev_score = Some(s.total_score);
    }

    Ok(scores)
}

/// 获取指定战队的得分历史。
pub async fn get_team_score_history(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> AwdResult<Vec<crate::entity::awd_score_events::Model>> {
    score_repo::find_score_events_by_team(db, event_id, team_id)
        .await
        .map_err(|e| AwdError::Database(e.to_string()))
}

/// 记录人工调分（管理端操作）。
pub async fn record_adjustment(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
    delta: i64,
    reason: &str,
    created_by: Uuid,
) -> AwdResult<()> {
    let key = IdempotencyKey::adjustment(&Uuid::new_v4().to_string());

    // Try once; if collision, regenerate UUID once
    let result = score_repo::create_score_event(
        db,
        event_id,
        None, // not tied to a round
        team_id,
        ScoreEventType::Adjustment,
        delta,
        &key,
        None,
        None,
        None,
        Some(reason),
    )
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("duplicate") || msg.contains("unique") {
                // Retry with new UUID
                let retry_key = IdempotencyKey::adjustment(&Uuid::new_v4().to_string());
                score_repo::create_score_event(
                    db,
                    event_id,
                    None,
                    team_id,
                    ScoreEventType::Adjustment,
                    delta,
                    &retry_key,
                    None,
                    None,
                    None,
                    Some(reason),
                )
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;
                Ok(())
            } else {
                Err(AwdError::Database(msg))
            }
        }
    }
}
