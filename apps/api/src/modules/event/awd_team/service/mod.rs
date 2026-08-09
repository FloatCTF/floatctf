//! AWD service layer — orchestrates repositories, domain logic, and external systems.

pub mod archive_service;
pub mod ban_service;
pub mod deploy_service;
pub mod event_network_service;
pub mod event_service;
pub mod firewall_service;
pub mod flag_service;
pub mod gamebox_service;
pub mod judge_service;
pub mod platform_network_service;
pub mod precheck_service;
pub mod recovery_service;
pub mod reset_service;
pub mod round_service;
pub mod score_service;
pub mod submission_service;
pub mod team_network_allocator;
pub mod wireguard_service;
