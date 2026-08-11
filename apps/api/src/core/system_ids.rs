//! Well-known system object IDs.
//!
//! **Source of truth is Rust**, not a DB enum/registry table.
//! Bootstrap / ensure paths seed or upsert the corresponding rows; runtime
//! code may also look up by semantic key (`system_key`, `task_key`).
//!
//! ID style matches historical scheduler seeds: `Uuid::from_u128(n)` →
//! `00000000-0000-0000-0000-00000000000n`.
//!
//! Numbers are **per table** (different relations may reuse the same `n`).

use uuid::Uuid;

// ── scheduled_tasks (startup / platform maintenance) ───────────────────────

/// `CHECK_PRACTICE_EVENT` — ensure practice:jeopardy on startup.
pub const SCHED_CHECK_PRACTICE_EVENT: Uuid = Uuid::from_u128(0);

/// `CLEAN_INSTANCES` — destroy expired challenge instances.
pub const SCHED_CLEAN_INSTANCES: Uuid = Uuid::from_u128(1);

/// `CLEAN_RUSTFS` — GC unused object storage files.
pub const SCHED_CLEAN_RUSTFS: Uuid = Uuid::from_u128(2);

// ── events (system-managed competitions) ───────────────────────────────────

/// Jeopardy Practice system event (`system_key = practice:jeopardy`).
///
/// Same numeric slot as [`SCHED_CLEAN_INSTANCES`] but on table `events`
/// (no cross-table uniqueness). Prefer semantic lookup via system_key in
/// application code; this constant is for ensure / docs / ops.
pub const EVENT_PRACTICE_JEOPARDY: Uuid = Uuid::from_u128(1);

/// Semantic key for [`EVENT_PRACTICE_JEOPARDY`] (unique on `events.system_key`).
pub const EVENT_PRACTICE_JEOPARDY_SYSTEM_KEY: &str = "practice:jeopardy";

/// All platform startup scheduled-task seeds: (id, display name, task_key str, trigger).
///
/// `task_key` strings must match [`crate::scheduler::task_key::TaskKey`] wire form.
pub fn startup_scheduled_task_seeds() -> &'static [(Uuid, &'static str, &'static str, &'static str)]
{
    &[
        (
            SCHED_CHECK_PRACTICE_EVENT,
            "检查练习event",
            "CHECK_PRACTICE_EVENT",
            "startup",
        ),
        (
            SCHED_CLEAN_INSTANCES,
            "实例清理",
            "CLEAN_INSTANCES",
            "startup",
        ),
        (SCHED_CLEAN_RUSTFS, "RUSTFS文件清理", "CLEAN_RUSTFS", "cron"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_ids_match_nil_style_strings() {
        assert_eq!(
            SCHED_CHECK_PRACTICE_EVENT.to_string(),
            "00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(
            SCHED_CLEAN_INSTANCES.to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
        assert_eq!(
            SCHED_CLEAN_RUSTFS.to_string(),
            "00000000-0000-0000-0000-000000000002"
        );
        assert_eq!(
            EVENT_PRACTICE_JEOPARDY.to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
        // Same u128 on different tables is intentional.
        assert_eq!(EVENT_PRACTICE_JEOPARDY, SCHED_CLEAN_INSTANCES);
    }

    #[test]
    fn startup_seed_list_covers_three_platform_tasks() {
        let seeds = startup_scheduled_task_seeds();
        assert_eq!(seeds.len(), 3);
        assert_eq!(seeds[0].0, SCHED_CHECK_PRACTICE_EVENT);
        assert_eq!(seeds[1].0, SCHED_CLEAN_INSTANCES);
        assert_eq!(seeds[2].0, SCHED_CLEAN_RUSTFS);
    }
}
