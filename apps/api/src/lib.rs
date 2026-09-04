//! FloatCTF — CTF 竞赛/实训平台库 crate。
//!
//! 对外暴露各业务模块与 bootstrap，供二进制与集成测试共用。

pub mod api;
pub mod bootstrap;
pub mod core;
pub mod entity;
pub mod infrastructure;
pub mod modules;
pub mod scheduler;
