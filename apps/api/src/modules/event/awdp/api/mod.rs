//! AWDP API 层。

pub mod admin;
pub mod dto;
pub mod player;

pub use admin::admin_events_routes;
pub use player::player_routes;
