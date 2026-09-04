//! AWD 网络运行时与密钥管理。

pub mod keys;
pub mod runtime;

pub use keys::{WgKeyPair, generate_keypair, public_from_private};
pub use runtime::{
    AwdNetworkRuntime, EventNetworkIdentity, HostNetworkRuntime, NetworkObservedState,
    NoopNetworkRuntime, PeerIdentity, TeamNetworkIdentity, WireGuardDesiredState,
};

// Compatibility re-exports of the system command layer.
pub use crate::modules::event::awd::system::{command, conntrack, wireguard};
