//! AWD infrastructure: persistence and host network adapters.

pub mod network;
pub mod persistence;

pub use network::{AwdNetworkRuntime, HostNetworkRuntime, NoopNetworkRuntime};
pub use persistence::{AwdPersistedEnum, Persist};
