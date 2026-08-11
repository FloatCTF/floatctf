//! `events::Model` 扩展：模式解析与系统托管身份判定。

use crate::entity::events;
use crate::modules::event::common::domain::event_mode::{
    EventMode, EventModeError, PRACTICE_JEOPARDY_SYSTEM_KEY,
};

impl events::Model {
    /// 由持久化身份字段构造并校验 [`EventMode`]。
    pub fn mode(&self) -> Result<EventMode, EventModeError> {
        EventMode::new(
            self.family.clone(),
            self.purpose.clone(),
            self.participant_mode.clone(),
        )
    }

    /// 不校验的模式三元组（字段已由库 CHECK 约束保证合法）。
    pub fn mode_unchecked(&self) -> EventMode {
        EventMode {
            family: self.family.clone(),
            purpose: self.purpose.clone(),
            participant_mode: self.participant_mode.clone(),
        }
    }

    /// 是否系统托管赛事（`system_key` 非空）。
    pub fn is_system_managed(&self) -> bool {
        self.system_key.is_some()
    }

    /// 是否 Jeopardy 系统练习赛事（`system_key = practice:jeopardy`）。
    pub fn is_practice_jeopardy(&self) -> bool {
        self.system_key.as_deref() == Some(PRACTICE_JEOPARDY_SYSTEM_KEY)
    }
}
