//! Conntrack cleanup — flush connection tracking entries for AWD events.
//!
//! # When to clean conntrack
//!
//! 1. Phase transition (hardening ↔ attack)
//! 2. Pause/resume
//! 3. Ban/unban
//! 4. Round end
//!
//! # Safety
//!
//! - Always use specific CIDR, never bare `conntrack -F`
//! - Only clean entries belonging to the event's GameBox CIDR
//! - Conntrack flush may disrupt established connections (intentional during phase transitions)

use crate::modules::event::awd::{
    AwdError, AwdResult,
    system::command::{CommandRunner, conntrack_cmd},
};

/// Flush conntrack entries for a specific CIDR.
pub async fn flush_for_cidr(runner: &dyn CommandRunner, cidr: &str) -> AwdResult<()> {
    conntrack_cmd::flush_event(runner, cidr)
        .await
        .map_err(|e| AwdError::Network(format!("Conntrack flush failed for {}: {}", cidr, e)))?;
    Ok(())
}

/// Flush conntrack entries for an entire event's GameBox subnet.
pub async fn flush_event_gamebox_traffic(
    runner: &dyn CommandRunner,
    gamebox_cidr: &str,
) -> AwdResult<()> {
    flush_for_cidr(runner, gamebox_cidr).await
}
