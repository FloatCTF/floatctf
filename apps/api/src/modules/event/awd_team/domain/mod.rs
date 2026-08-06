//! AWD domain models.
//!
//! # Layering
//!
//! - **State machines**: extension traits on generated ActiveEnum
//!   (`event_ext`, `gamebox_ext`, `round_ext`) — single source of transition rules.
//! - **Value objects**: pure types without SeaORM (`Ipv4Cidr`, flag helpers, scores).
//! - **Persistence mapping**: `crate::modules::event::awd_team::infrastructure::persistence::mapping`.
//!
//! Generated entity enums are re-exported here for call-site convenience; they must
//! never be edited by hand (sea-orm-cli only).

pub mod flag;
pub mod network;
pub mod score;

// Persistence representation of domain state (CLI-generated ActiveEnum).
pub use crate::entity::sea_orm_active_enums::{
    AwdEventStatus, AwdPhase, BanStatus, GameboxStatus, JudgeTaskStatus, PrecheckStatus,
    RoundStatus, ScoreEventType, WgPeerStatus,
};

mod event_ext;
mod gamebox_ext;
mod round_ext;

pub use event_ext::*;
pub use flag::{generate_flag, hash_flag, verify_flag};
pub use gamebox_ext::*;
pub use network::Ipv4Cidr;
pub use round_ext::*;
pub use score::{IdempotencyKey, TeamScore};
