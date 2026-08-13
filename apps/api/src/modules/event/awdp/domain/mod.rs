//! AWDP 领域纯逻辑（无 DB / 无 IO）。

pub mod config;
pub mod flag;
pub mod judge;
pub mod phase;
pub mod score;
pub mod timing;

pub use config::{
    AwdpConfig, AwdpConfigPatch, DEFAULT_BREAK_DURATION_SECS, DEFAULT_BREAK_SCORE,
    DEFAULT_FIX_DURATION_SECS, DEFAULT_FIX_ROUND_INTERVAL_SECS, DEFAULT_FIX_ROUND_SCORE,
};
pub use phase::AwdpPhaseExt;
pub use score::{break_idempotency_key, fix_idempotency_key, subject_key};
pub use timing::{RoundWindow, round_windows, total_rounds};
