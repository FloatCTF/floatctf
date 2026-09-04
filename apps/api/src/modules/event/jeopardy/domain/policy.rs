//! Jeopardy 运行时策略：Purpose × Participant 规则的唯一权威来源。

use crate::entity::events;
use crate::entity::sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode};
use crate::modules::event::common::domain::event_mode::{EventMode, EventModeError};

/// 已校验 Jeopardy 赛事的规则集（family 必须为 Jeopardy）。
#[derive(Debug, Clone)]
pub struct JeopardyPolicy {
    purpose: EventPurpose,
    participant_mode: ParticipantMode,
}

impl JeopardyPolicy {
    /// 由已校验的 [`EventMode`] 构造（非 Jeopardy 返回 `None`）。
    pub fn from_mode(mode: &EventMode) -> Option<Self> {
        if !mode.is_jeopardy() {
            return None;
        }
        Some(Self {
            purpose: mode.purpose.clone(),
            participant_mode: mode.participant_mode.clone(),
        })
    }

    /// 由持久化赛事行构造。
    pub fn from_event(event: &events::Model) -> Result<Self, EventModeError> {
        let mode = event.mode()?;
        Self::from_mode(&mode).ok_or(EventModeError::InvalidCombination {
            family: mode.family,
            purpose: mode.purpose,
            participant_mode: mode.participant_mode,
        })
    }

    /// 赛制族门禁：调用方已加载赛事行时，要求 `family = Jeopardy`。
    pub fn require_jeopardy_family(event: &events::Model) -> anyhow::Result<()> {
        if event.family != EventFamily::Jeopardy {
            return Err(anyhow::anyhow!(
                "UnsupportedForFamily: AWD events do not support this via the Jeopardy API; use /api/events/{{id}}/awd"
            ));
        }
        Ok(())
    }

    pub fn purpose(&self) -> &EventPurpose {
        &self.purpose
    }

    pub fn participant_mode(&self) -> &ParticipantMode {
        &self.participant_mode
    }

    pub fn is_practice(&self) -> bool {
        self.purpose == EventPurpose::Practice
    }

    pub fn is_competition(&self) -> bool {
        self.purpose == EventPurpose::Competition
    }

    pub fn is_team(&self) -> bool {
        self.participant_mode == ParticipantMode::Team
    }

    pub fn is_individual(&self) -> bool {
        self.participant_mode == ParticipantMode::Individual
    }

    /// 竞赛要求题目已挂载到该赛事（`jeopardy_event_challenges`）。
    pub fn requires_event_challenge(&self) -> bool {
        self.is_competition()
    }

    /// 解题是否计入官方积分。
    pub fn contributes_to_official_score(&self) -> bool {
        self.is_competition()
    }

    /// 是否提供官方积分榜 / 趋势接口。
    pub fn supports_official_scoreboard(&self) -> bool {
        self.is_competition()
    }

    /// 练习在已有正式解题记录后仍允许再次开环境复练。
    /// **不是**允许插入重复的 solve 行。
    pub fn allows_retraining_after_solve(&self) -> bool {
        self.is_practice()
    }

    /// 当前参赛作用域内允许的最大并发运行实例数。
    ///
    /// - 练习 + 个人 → 1
    /// - 竞赛 + 个人 → 2
    /// - 竞赛 + 战队 → `team_member_count * 2`（至少 2）
    pub fn max_concurrent_instances(&self, team_member_count: Option<u64>) -> u64 {
        match (&self.purpose, &self.participant_mode) {
            (EventPurpose::Practice, _) => 1,
            (EventPurpose::Competition, ParticipantMode::Individual) => 2,
            (EventPurpose::Competition, ParticipantMode::Team) => {
                team_member_count.unwrap_or(1).saturating_mul(2).max(2)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::common::domain::event_mode::EventMode;

    #[test]
    fn practice_policy_rules() {
        let p = JeopardyPolicy::from_mode(&EventMode::jeopardy_practice()).unwrap();
        assert!(!p.requires_event_challenge());
        assert!(!p.contributes_to_official_score());
        assert!(!p.supports_official_scoreboard());
        assert!(p.allows_retraining_after_solve());
        assert_eq!(p.max_concurrent_instances(None), 1);
        assert_eq!(p.max_concurrent_instances(Some(10)), 1);
    }

    #[test]
    fn individual_competition_policy_rules() {
        let p = JeopardyPolicy::from_mode(&EventMode::jeopardy_individual_competition()).unwrap();
        assert!(p.requires_event_challenge());
        assert!(p.contributes_to_official_score());
        assert!(p.supports_official_scoreboard());
        assert!(!p.allows_retraining_after_solve());
        assert_eq!(p.max_concurrent_instances(None), 2);
        assert_eq!(p.max_concurrent_instances(Some(99)), 2);
    }

    #[test]
    fn team_competition_policy_rules() {
        let p = JeopardyPolicy::from_mode(&EventMode::jeopardy_team_competition()).unwrap();
        assert!(p.requires_event_challenge());
        assert!(p.contributes_to_official_score());
        assert!(p.supports_official_scoreboard());
        assert_eq!(p.max_concurrent_instances(Some(1)), 2);
        assert_eq!(p.max_concurrent_instances(Some(3)), 6);
        assert_eq!(p.max_concurrent_instances(None), 2);
    }

    #[test]
    fn awd_mode_has_no_jeopardy_policy() {
        assert!(JeopardyPolicy::from_mode(&EventMode::awd_team_competition()).is_none());
    }
}
