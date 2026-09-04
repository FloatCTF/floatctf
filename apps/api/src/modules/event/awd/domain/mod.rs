//! AWD 领域类型与扩展。

pub mod execution;
pub mod firewall_state;
pub mod flag;
pub mod network;
pub mod score;
pub mod timing;

// Persistence representation of domain state (CLI-generated ActiveEnum).
pub use crate::entity::sea_orm_active_enums::{
    AwdEventStatus, AwdPhase, BanStatus, GameboxStatus, JudgeTaskStatus, PrecheckStatus,
    RoundStatus, ScoreEventType, WgPeerStatus,
};

mod event_ext;
mod gamebox_ext;
mod round_ext;

pub use event_ext::*;
pub use execution::ExecutionContext;
pub use firewall_state::{DesiredEventPolicy, DesiredFirewallState, DesiredTeamPolicy, IpNet};
pub use flag::{generate_flag, hash_flag, verify_flag};
pub use gamebox_ext::*;
pub use network::Ipv4Cidr;
pub use round_ext::*;
pub use score::{IdempotencyKey, TeamScore};
