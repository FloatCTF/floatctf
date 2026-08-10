//! Repository layer for AWD operations — encapsulates database access.
//!
//! Each module wraps SeaORM queries on the generated entities, keeping
//! raw query construction out of handlers and services.

pub mod ban_repo;
pub mod event_gamebox_repo;
pub mod event_network_repo;
pub mod event_repo;
pub mod flag_repo;
pub mod gamebox_lib_repo;
pub mod gamebox_repo;
pub mod gamebox_revision_repo;
pub mod judge_repo;
pub mod network_allocation_repo;
pub mod network_settings_repo;
pub mod round_repo;
pub mod score_repo;
pub mod wireguard_repo;
