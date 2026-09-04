//! 赛事公共应用服务（CRUD、报名/退赛、战队、积分榜适配等）。

pub mod admin_service;
pub mod event_log_service;
pub mod player_service;
pub mod team_service;

pub use admin_service as admin;
pub use player_service as player;
