//! EventMode — Family × Purpose × ParticipantMode value object.
//!
//! Source of truth for event identity/semantics (replaces legacy EventType).

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::entity::sea_orm_active_enums::{EventFamily, EventPurpose, ParticipantMode};

/// Validated combination of family / purpose / participant_mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMode {
    pub family: EventFamily,
    pub purpose: EventPurpose,
    pub participant_mode: ParticipantMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EventModeError {
    #[error(
        "invalid event mode combination: family={family:?} purpose={purpose:?} participant_mode={participant_mode:?}"
    )]
    InvalidCombination {
        family: EventFamily,
        purpose: EventPurpose,
        participant_mode: ParticipantMode,
    },
}

impl EventMode {
    /// Construct and validate a mode triple.
    pub fn new(
        family: EventFamily,
        purpose: EventPurpose,
        participant_mode: ParticipantMode,
    ) -> Result<Self, EventModeError> {
        let mode = Self {
            family,
            purpose,
            participant_mode,
        };
        mode.validate()?;
        Ok(mode)
    }

    /// Validate current allowed combinations (must match DB CHECK).
    pub fn validate(&self) -> Result<(), EventModeError> {
        let ok = matches!(
            (&self.family, &self.purpose, &self.participant_mode),
            (
                EventFamily::Jeopardy,
                EventPurpose::Practice,
                ParticipantMode::Individual
            ) | (
                EventFamily::Jeopardy,
                EventPurpose::Competition,
                ParticipantMode::Individual
            ) | (
                EventFamily::Jeopardy,
                EventPurpose::Competition,
                ParticipantMode::Team
            ) | (
                EventFamily::Awd,
                EventPurpose::Competition,
                ParticipantMode::Team
            )
        );
        if ok {
            Ok(())
        } else {
            Err(EventModeError::InvalidCombination {
                family: self.family.clone(),
                purpose: self.purpose.clone(),
                participant_mode: self.participant_mode.clone(),
            })
        }
    }

    pub fn jeopardy_practice() -> Self {
        Self {
            family: EventFamily::Jeopardy,
            purpose: EventPurpose::Practice,
            participant_mode: ParticipantMode::Individual,
        }
    }

    pub fn jeopardy_individual_competition() -> Self {
        Self {
            family: EventFamily::Jeopardy,
            purpose: EventPurpose::Competition,
            participant_mode: ParticipantMode::Individual,
        }
    }

    pub fn jeopardy_team_competition() -> Self {
        Self {
            family: EventFamily::Jeopardy,
            purpose: EventPurpose::Competition,
            participant_mode: ParticipantMode::Team,
        }
    }

    pub fn awd_team_competition() -> Self {
        Self {
            family: EventFamily::Awd,
            purpose: EventPurpose::Competition,
            participant_mode: ParticipantMode::Team,
        }
    }

    pub fn is_practice(&self) -> bool {
        self.purpose == EventPurpose::Practice
    }

    pub fn is_competition(&self) -> bool {
        self.purpose == EventPurpose::Competition
    }

    pub fn is_individual(&self) -> bool {
        self.participant_mode == ParticipantMode::Individual
    }

    pub fn is_team(&self) -> bool {
        self.participant_mode == ParticipantMode::Team
    }

    pub fn is_jeopardy(&self) -> bool {
        self.family == EventFamily::Jeopardy
    }

    pub fn is_awd(&self) -> bool {
        self.family == EventFamily::Awd
    }
}

/// System-managed Practice event key (semantic lookup).
pub const PRACTICE_JEOPARDY_SYSTEM_KEY: &str = "practice:jeopardy";

/// Well-known Practice event primary key.
///
/// Same style as scheduler startup tasks (`…0000` / `…0001` / `…0002` on
/// `scheduled_tasks`). `events` uses `…0001` so the id is stable across
/// ensure / fresh bootstrap / rebuild (still resolve by `system_key` in code).
pub const PRACTICE_JEOPARDY_EVENT_ID: Uuid = Uuid::from_u128(1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_modes_validate() {
        for mode in [
            EventMode::jeopardy_practice(),
            EventMode::jeopardy_individual_competition(),
            EventMode::jeopardy_team_competition(),
            EventMode::awd_team_competition(),
        ] {
            assert!(mode.validate().is_ok(), "{mode:?}");
            assert!(
                EventMode::new(
                    mode.family.clone(),
                    mode.purpose.clone(),
                    mode.participant_mode.clone()
                )
                .is_ok()
            );
        }
    }

    #[test]
    fn illegal_modes_reject() {
        let illegal = [
            (
                EventFamily::Jeopardy,
                EventPurpose::Practice,
                ParticipantMode::Team,
            ),
            (
                EventFamily::Awd,
                EventPurpose::Practice,
                ParticipantMode::Team,
            ),
            (
                EventFamily::Awd,
                EventPurpose::Competition,
                ParticipantMode::Individual,
            ),
            (
                EventFamily::Awd,
                EventPurpose::Practice,
                ParticipantMode::Individual,
            ),
            (
                EventFamily::Jeopardy,
                EventPurpose::Practice,
                ParticipantMode::Team, // duplicate explicit for clarity
            ),
        ];
        // Dedup last duplicate is fine; assert all reject.
        for (family, purpose, participant_mode) in illegal {
            let err = EventMode::new(family, purpose, participant_mode);
            assert!(err.is_err(), "expected invalid: {err:?}");
        }
    }
}
