//! 统一赛事模块（`modules/event`）。
//!
//! 身份模型：`EventFamily` × `EventPurpose` × `ParticipantMode`（[`common::domain::event_mode::EventMode`]）。
//! 引擎：`jeopardy`（解题赛）与 `awd`（攻防赛），相互独立。

pub mod common;

pub mod awd;
pub(crate) mod jeopardy;

mod error;

pub use error::{EventError, EventResult};
