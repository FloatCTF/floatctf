//! Score event types and idempotency key construction.

use serde::{Deserialize, Serialize};

/// Re-export SeaORM-generated score event type as canonical.
pub use crate::entity::sea_orm_active_enums::ScoreEventType;

/// Builder for idempotency keys to prevent duplicate scoring.
pub struct IdempotencyKey;

impl IdempotencyKey {
    /// Attack score: one per attacker per instance per round.
    pub fn attack(
        event_id: &str,
        round_id: &str,
        attacker_team: &str,
        instance_id: &str,
    ) -> String {
        format!(
            "attack:{}:{}:{}:{}",
            event_id, round_id, attacker_team, instance_id
        )
    }

    /// Victim loss: paired with attack.
    pub fn victim_loss(
        event_id: &str,
        round_id: &str,
        attacker_team: &str,
        instance_id: &str,
    ) -> String {
        format!(
            "victim-loss:{}:{}:{}:{}",
            event_id, round_id, attacker_team, instance_id
        )
    }

    /// First blood bonus: once per EventGameBox per event（§29）。
    /// 同一全局 GameBox 在两个 Event 中各自拥有 first blood。
    pub fn first_bonus(event_id: &str, event_gamebox_id: &str) -> String {
        format!("first-bonus:{}:{}", event_id, event_gamebox_id)
    }

    /// Judge check result: once per task.
    pub fn judge(event_id: &str, round_id: &str, team_id: &str, instance_id: &str) -> String {
        format!(
            "judge:{}:{}:{}:{}",
            event_id, round_id, team_id, instance_id
        )
    }

    /// Reset penalty: once per reset record.
    pub fn reset(reset_id: &str) -> String {
        format!("reset:{}", reset_id)
    }

    /// Manual adjustment: once per adjustment.
    pub fn adjustment(adjustment_id: &str) -> String {
        format!("adjustment:{}", adjustment_id)
    }
}

/// Scoreboard entry for aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamScore {
    pub team_id: uuid::Uuid,
    pub team_name: String,
    pub attack_score: i64,
    pub defense_score: i64,
    pub total_score: i64,
    pub rank: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idempotency_key_uniqueness() {
        let k1 = IdempotencyKey::attack("evt-1", "rnd-1", "team-a", "inst-1");
        let k2 = IdempotencyKey::attack("evt-1", "rnd-1", "team-b", "inst-1");
        assert_ne!(k1, k2, "Different attackers must have different keys");
    }

    #[test]
    fn test_attack_and_victim_loss_different_keys() {
        let attack = IdempotencyKey::attack("evt-1", "rnd-1", "team-a", "inst-1");
        let loss = IdempotencyKey::victim_loss("evt-1", "rnd-1", "team-a", "inst-1");
        assert_ne!(attack, loss);
    }

    #[test]
    fn test_first_bonus_once_per_event_gamebox() {
        let k1 = IdempotencyKey::first_bonus("evt-1", "eg-1");
        let k2 = IdempotencyKey::first_bonus("evt-1", "eg-1");
        assert_eq!(k1, k2, "Same event+event_gamebox must have same key");
        // §29：同 GameBox 在不同 Event 各自独立 first blood
        let other_event = IdempotencyKey::first_bonus("evt-2", "eg-1");
        assert_ne!(k1, other_event, "Event 之间 first blood 独立");
    }
}
