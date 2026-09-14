//! 统一赛事模块（`modules/event`）。
//!
//! 身份模型：`EventFamily` × `EventPurpose` × `ParticipantMode`（[`common::domain::event_mode::EventMode`]）。
//! 引擎：`jeopardy`（解题赛）与 `awd`（攻防赛），相互独立。

pub mod common;

pub mod awd;
/// 解题（Jeopardy）引擎。`pub` 以便集成测试（tests/）直接使用其服务（与 awd/awdp 一致）。
pub mod jeopardy;

/// AWD Plus 模块骨架（引擎未实现，见模块内文档）。
pub mod awdp;

mod error;

pub use error::{EventError, EventResult};
