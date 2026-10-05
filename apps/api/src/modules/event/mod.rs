//! 统一赛事模块（`modules/event`）。
//!
//! 身份模型：`EventFamily` × `EventPurpose` × `ParticipantMode`（[`common::domain::event_mode::EventMode`]）。
//! 引擎：`jeopardy`（解题赛）、`awd`（攻防赛）与 `awdp`（攻防 + 补丁/fix），相互独立。

pub mod common;

pub mod awd;
/// 解题（Jeopardy）引擎。`pub` 以便集成测试（tests/）直接使用其服务（与 awd/awdp 一致）。
pub mod jeopardy;

/// AWD Plus（攻防 + 补丁/fix）引擎：domain/service/repo/scheduler 独立实现。
pub mod awdp;

mod error;

pub use error::{EventError, EventResult};
