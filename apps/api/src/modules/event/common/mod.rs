//! Shared event operations (not mode-specific).

pub mod api;
pub mod application;
pub mod domain;
pub mod infrastructure;

pub use application::{admin_service, player_service, team_service};
pub use infrastructure::event_repository;
