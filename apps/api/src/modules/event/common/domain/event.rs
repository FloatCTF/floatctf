//! Event model helpers (mode / system identity).

use crate::entity::events;
use crate::modules::event::common::domain::event_mode::{
    EventMode, EventModeError, PRACTICE_JEOPARDY_SYSTEM_KEY,
};

impl events::Model {
    /// Validated EventMode derived from persisted identity fields.
    pub fn mode(&self) -> Result<EventMode, EventModeError> {
        EventMode::new(
            self.family.clone(),
            self.purpose.clone(),
            self.participant_mode.clone(),
        )
    }

    /// Unchecked mode triple (fields already DB-constrained).
    pub fn mode_unchecked(&self) -> EventMode {
        EventMode {
            family: self.family.clone(),
            purpose: self.purpose.clone(),
            participant_mode: self.participant_mode.clone(),
        }
    }

    pub fn is_system_managed(&self) -> bool {
        self.system_key.is_some()
    }

    pub fn is_practice_jeopardy(&self) -> bool {
        self.system_key.as_deref() == Some(PRACTICE_JEOPARDY_SYSTEM_KEY)
    }
}
