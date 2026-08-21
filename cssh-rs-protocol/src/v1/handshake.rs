//! The `Hello` / `Welcome` connection handshake.

use serde_derive::{Deserialize, Serialize};

use crate::v1::limits::DEFAULT_MAX_FRAME_LEN;
use crate::v1::version::{ProtocolVersion, Role};

/// First message each endpoint sends immediately on connect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// Protocol version this endpoint speaks.
    pub protocol_version: ProtocolVersion,
    /// Whether the sender is the daemon or a client.
    pub role: Role,
    /// Process id of the sender.
    pub pid: u32,
    /// Capability flags this endpoint advertises: open-ended, opaque
    /// namespaced strings (e.g. `"paste"`) that peers intersect. Role-neutral,
    /// since both the daemon and a client send `Hello`.
    pub capabilities: Vec<String>,
    /// Largest CBOR frame body, in bytes, this endpoint is willing to receive.
    /// Peers take the minimum of the two advertised values as the send limit,
    /// so neither sends a frame the other would reject. Defaults to
    /// `DEFAULT_MAX_FRAME_LEN` for a peer that predates the field.
    #[serde(default = "default_max_frame_len")]
    pub max_frame_len: u32,
}

fn default_max_frame_len() -> u32 {
    return DEFAULT_MAX_FRAME_LEN;
}

/// Daemon's reply to a client's [`Hello`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    /// Daemon-assigned identifier for this client.
    pub client_id: u64,
    /// Capability flags the daemon supports.
    pub server_capabilities: Vec<String>,
}

#[cfg(test)]
#[path = "../tests/v1/test_handshake.rs"]
mod tests;
