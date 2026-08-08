//! Submission service — handles flag submission, scoring, and duplicate detection.
//!
//! All operations run in a single database transaction to ensure atomicity.
//! If any step fails, the entire submission is rolled back.

use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use tracing::info;
use uuid::Uuid;

use crate::modules::event::awd_team::{
    AwdError, AwdResult,
    domain::{IdempotencyKey, score::ScoreEventType},
    repo::{flag_repo, score_repo},
};

/// Result of a successful flag submission.
pub struct SubmissionResult {
    pub attack_score_delta: i64,
    pub victim_loss_delta: i64,
    pub first_bonus_delta: i64,
    pub was_first_blood: bool,
}

/// Process a flag submission atomically within a single transaction:
/// 1. Check for duplicate submission (early exit with clear error)
/// 2. Insert submission record (unique constraint as final guard)
/// 3. Insert attack score event
/// 4. Insert victim loss score event
/// 5. Try first-blood bonus
///
/// If any step fails, the entire transaction is rolled back.
/// All operations use idempotency keys to prevent double-counting.
pub async fn process_submission(
    db: &DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
    flag_issue_id: Uuid,
    attacker_team_id: Uuid,
    victim_team_id: Uuid,
    gamebox_instance_id: Uuid,
    submitted_by_user_id: Uuid,
    break_points: i64,
    loss_points: i64,
    first_bonus: i64,
    template_id: Uuid,
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

                // 3. Insert attack score event
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
                    break_points,
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

                // 4. Insert victim loss score event
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
                    -loss_points,
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
                let bonus_key =
                    IdempotencyKey::first_bonus(&event_id.to_string(), &template_id.to_string());

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
                    Some(template_id),
                    Some("first blood"),
                )
                .await
                .map_err(|e| {
                    AwdError::Database(format!("First blood bonus write failed: {}", e))
                })?;
                Ok(SubmissionResult {
                    attack_score_delta: break_points,
                    victim_loss_delta: loss_points,
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
            crate::modules::event::awd_team::websocket::score_changed(
                event_id,
                attacker_team_id,
                break_points,
                break_points,
            )
            .into_realtime(),
        )
        .await;

    Ok(result)
}
