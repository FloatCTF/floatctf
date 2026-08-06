//! Common event application services (CRUD, join/leave, teams, scoreboard adapters).

pub mod admin_service;
pub mod player_service;
pub mod team_service;

pub use admin_service as admin;
pub use player_service as player;
