//! AWD Plus（`EventFamily::Awdp`）—— 独立 bounded context。
//!
//! 与 AWD 是 siblings：共享 GameBox 库 / fcmc 运行时 / scheduler / realtime，
//! 但 Break / Fix / Patch / Evaluation / Score 全部属于本模块自己的领域语义。
//!
//! 依赖方向：
//!   awdp ──> modules::gamebox / fcmc / scheduler / common
//!   awdp 绝不 import modules::event::awd::{round_service, judge_service, flag_service, ...}

pub mod api;
pub mod domain;
pub mod realtime;
pub mod repo;
pub mod service;

mod error;

pub use error::{AwdpError, AwdpResult};
pub mod scheduler;
