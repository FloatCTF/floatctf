//! Explicit mapping between domain concepts and SeaORM ActiveEnum values.
//!
//! # Canonical rules
//!
//! - **Business rules** (transitions, phase permissions) live only on
//!   `domain::{AwdEventStatusExt, AwdPhaseExt, GameboxStatusExt, ...}`.
//! - **Persistence** uses generated `entity::sea_orm_active_enums::*` (never hand-edited).
//! - Services/repos accept ActiveEnum at the DB boundary; do not invent parallel enums
//!   in handlers. Pure value objects (`Ipv4Cidr`, flag hashes, idempotency keys) stay
//!   in `domain/` without SeaORM deps.
//!
//! If a future split introduces pure domain enums, add `TryFrom` pairs here only.

use crate::entity::sea_orm_active_enums::{
    AwdEventStatus, AwdPhase, BanStatus, GameboxStatus, JudgeTaskStatus, PrecheckStatus,
    RoundStatus, ScoreEventType, WgPeerStatus,
};

/// Marker: these ActiveEnum types are the persistence representation of AWD domain state.
pub trait AwdPersistedEnum: Sized + Clone + PartialEq + Eq + Send + Sync + 'static {}

impl AwdPersistedEnum for AwdEventStatus {}
impl AwdPersistedEnum for AwdPhase {}
impl AwdPersistedEnum for GameboxStatus {}
impl AwdPersistedEnum for RoundStatus {}
impl AwdPersistedEnum for JudgeTaskStatus {}
impl AwdPersistedEnum for PrecheckStatus {}
impl AwdPersistedEnum for BanStatus {}
impl AwdPersistedEnum for ScoreEventType {}
impl AwdPersistedEnum for WgPeerStatus {}

/// Identity map used at boundaries that already hold ActiveEnum values.
/// Exists so call sites can document intent: `status.persist()` vs free-form casts.
pub trait Persist: AwdPersistedEnum {
    fn persist(self) -> Self {
        self
    }
}

impl<T: AwdPersistedEnum> Persist for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::awd::domain::AwdEventStatusExt;

    #[test]
    fn active_enum_carries_domain_transitions() {
        // Domain rules are attached to the persisted enum via Ext traits.
        assert!(
            AwdEventStatus::Draft
                .can_transition_to(AwdEventStatus::Configuring)
                .is_ok()
        );
        assert_eq!(AwdEventStatus::Draft.persist(), AwdEventStatus::Draft);
    }
}
