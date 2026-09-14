//! AWD Flag 提交服务。

use sea_orm::{DatabaseConnection, TransactionTrait};
use uuid::Uuid;

use crate::modules::event::awd::{
    AwdError, AwdResult,
    domain::{IdempotencyKey, score::ScoreEventType},
    repo::{flag_repo, score_repo},
};

/// a successful flag submission的结果。
pub struct SubmissionResult {
    pub attack_score_delta: i64,
    pub victim_loss_delta: i64,
    pub first_bonus_delta: i64,
    pub was_first_blood: bool,
}

/// 在单一事务内原子处理 Flag 提交：
/// 1. 检查重复提交（清晰错误并提前返回）
/// 2. 插入提交记录（唯一约束作最终防护）
/// 3. 插入攻击得分事件
/// 4. 插入被攻击失分事件
/// 5. 尝试一血加分
///
/// 任一步失败则整事务回滚。
/// 全部操作使用幂等键，防止重复计分。
pub async fn process_submission(
    db: &DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
    flag_issue_id: Uuid,
    attacker_team_id: Uuid,
    victim_team_id: Uuid,
    gamebox_instance_id: Uuid,
    submitted_by_user_id: Uuid,
    attack_score: i64,
    first_bonus: i64,
    event_gamebox_id: Uuid,
    publisher: &dyn crate::infrastructure::realtime::EventPublisher,
) -> AwdResult<SubmissionResult> {
    let result = db
        .transaction(|tx| {
            Box::pin(async move {
                // 1. Check if this team already submitted for this instance this round
                let already_submitted = flag_repo::has_submission(
                    tx,
                    event_id,
                    round_id,
                    attacker_team_id,
                    gamebox_instance_id,
                )
                .await
                .map_err(|e| AwdError::Database(e.to_string()))?;

                if already_submitted {
                    return Err(AwdError::Conflict(
                        "Already submitted this flag for this target this round".into(),
                    ));
                }

                // 2. Insert submission (unique constraint protects against races)
                flag_repo::create_submission(
                    tx,
                    event_id,
                    round_id,
                    flag_issue_id,
                    attacker_team_id,
                    victim_team_id,
                    gamebox_instance_id,
                    submitted_by_user_id,
                )
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("duplicate") || msg.contains("unique") {
                        AwdError::Conflict("Already submitted (concurrent request)".into())
                    } else {
                        AwdError::Database(msg)
                    }
                })?;

                // 3. Insert attack score event (§10: symmetric, +attack_score)
                let attack_key = IdempotencyKey::attack(
                    &event_id.to_string(),
                    &round_id.to_string(),
                    &attacker_team_id.to_string(),
                    &gamebox_instance_id.to_string(),
                );

                score_repo::create_score_event(
                    tx,
                    event_id,
                    Some(round_id),
                    attacker_team_id,
                    ScoreEventType::Attack,
                    attack_score,
                    &attack_key,
                    Some(victim_team_id),
                    Some(gamebox_instance_id),
                    None,
                    Some("flag capture"),
                )
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("duplicate") || msg.contains("unique") {
                        AwdError::Conflict("Attack already scored".into())
                    } else {
                        AwdError::Database(msg)
                    }
                })?;

                // 4. Insert victim loss score event (§10: symmetric, -attack_score)
                let loss_key = IdempotencyKey::victim_loss(
                    &event_id.to_string(),
                    &round_id.to_string(),
                    &attacker_team_id.to_string(),
                    &gamebox_instance_id.to_string(),
                );

                let _loss_event = score_repo::create_score_event(
                    tx,
                    event_id,
                    Some(round_id),
                    victim_team_id,
                    ScoreEventType::VictimLoss,
                    -attack_score,
                    &loss_key,
                    Some(attacker_team_id),
                    Some(gamebox_instance_id),
                    None,
                    Some("flag stolen"),
                )
                .await
                .map_err(|e| AwdError::Database(format!("Victim loss scoring failed: {}", e)))?;

                // 5. Try first-blood bonus (once per template per event)
                //
                // 用 ON CONFLICT DO NOTHING 幂等写入：裸 INSERT 的唯一冲突会把整个
                // 事务置为 aborted（PostgreSQL 语义），随后 COMMIT 静默变 ROLLBACK——
                // 败者的攻击分 + 受害者损失会被一起丢掉。DO NOTHING 则把「bonus 已被
                // 别人抢先」当作正常结果，不影响同一事务里已成功的 attack/loss 写入。
                let bonus_key = IdempotencyKey::first_bonus(
                    &event_id.to_string(),
                    &event_gamebox_id.to_string(),
                );

                let was_first_blood = score_repo::create_score_event_if_absent(
                    tx,
                    event_id,
                    Some(round_id),
                    attacker_team_id,
                    ScoreEventType::FirstBonus,
                    first_bonus,
                    &bonus_key,
                    None,
                    None,
                    Some(event_gamebox_id),
                    Some("first blood"),
                )
                .await
                .map_err(|e| {
                    AwdError::Database(format!("First blood bonus write failed: {}", e))
                })?;
                Ok(SubmissionResult {
                    attack_score_delta: attack_score,
                    victim_loss_delta: attack_score,
                    first_bonus_delta: if was_first_blood { first_bonus } else { 0 },
                    was_first_blood,
                })
            })
        })
        .await
        .map_err(|e| AwdError::Database(format!("Transaction failed: {}", e)))?;

    // P3-7：DB commit 后发布 score.changed（best-effort，不回滚业务）
    let _ = publisher
        .publish(
            crate::modules::event::awd::websocket::score_changed(
                event_id,
                attacker_team_id,
                attack_score,
                attack_score,
            )
            .into_realtime(),
        )
        .await;

    Ok(result)
}
