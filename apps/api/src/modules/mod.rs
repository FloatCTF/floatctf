//! Application modules.
//!
//! - `event`: unified competition types (common / jeopardy / modes / awd_team)
//! - `challenge`: challenge catalog, sets, writeups, build/import
//! - `identity`: authentication, users, administrators
//! - `community`: discussions, comments, likes
//! - `weapon`: weapon catalog
//! - `platform`: operational admin + player (announcements, settings, logs, docker, …)
//!
//! OOB (`oob_tokens` / `oob_records`): entity-only dead code — no business
//! handlers. Do not create empty `modules/oob`; table cleanup needs a later migration.

pub mod challenge;
pub mod community;
pub mod event;
pub mod identity;
pub mod platform;
pub mod weapon;
