//! AWD host networking: WireGuard, firewall, conntrack.

pub mod keys;
pub mod runtime;

pub use keys::{WgKeyPair, generate_keypair, public_from_private};
pub use runtime::{
    AwdNetworkRuntime, EventNetworkIdentity, EventNetworkPolicy, HostNetworkRuntime,
    NetworkObservedState, NoopNetworkRuntime, PeerIdentity, TeamNetworkIdentity,
    WireGuardDesiredState,
};

// Compatibility re-exports of the system command layer.
pub use crate::modules::event::awd_team::system::{command, conntrack, firewall, wireguard};
