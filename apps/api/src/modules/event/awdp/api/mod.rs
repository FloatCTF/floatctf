//! AWDP API 层。

pub mod admin;
pub mod dto;
pub mod internal;
pub mod player;
pub mod practice_judge;
pub mod training;

pub use admin::admin_events_routes;
pub use internal::internal_routes;
pub use player::player_routes;
pub use training::configure_training_routes;
