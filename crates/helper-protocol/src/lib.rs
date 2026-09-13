use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_CONTROL_SOCKET_PATH: &str = "/run/floatctf/helper-control.sock";
pub const DEFAULT_DOCKER_SOCKET_PATH: &str = "/run/floatctf/helper-docker.sock";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Ping,
    EnsureWireguard {
        interface: String,
        private_key: String,
        listen_port: u16,
        address: String,
    },
    RemoveWireguard {
        interface: String,
    },
    AddWireguardPeer {
        interface: String,
        public_key: String,
        allowed_ips: String,
    },
    RemoveWireguardPeer {
        interface: String,
        public_key: String,
    },
    FlushConntrack {
        cidr: String,
    },
    InspectEventNetwork {
        interface: String,
    },
    ListHostRouteCidrs,
    ListNftTable {
        table: String,
    },
    ApplyNftTable {
        table: String,
        ruleset: String,
        ensure_bridge_netfilter: bool,
    },
    DeleteNftTable {
        table: String,
    },
    EnsureDockerForward {
        wg_interface: String,
        bridge_name: String,
        gamebox_cidr: String,
    },
    CheckDockerForward {
        wg_interface: String,
        bridge_name: String,
        gamebox_cidr: String,
    },
    RemoveDockerForward {
        wg_interface: String,
        bridge_name: String,
        gamebox_cidr: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn success<T: Serialize>(data: T) -> Self {
        Self {
            ok: true,
            data: serde_json::to_value(data).ok(),
            error: None,
        }
    }

    pub fn empty() -> Self {
        Self {
            ok: true,
            data: None,
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventNetworkObservation {
    pub wireguard_interface_up: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerForwardCheck {
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingData {
    pub version: String,
}
