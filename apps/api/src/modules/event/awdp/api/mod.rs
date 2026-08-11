//! AWDP API 层。

pub mod admin;
pub mod dto;
pub mod player;
pub mod training;

pub use admin::admin_events_routes;
pub use player::player_routes;
pub use training::configure_training_routes;
