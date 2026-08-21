//! The top-level daemon-to-client and client-to-daemon message catalogs.

use serde_derive::{Deserialize, Serialize};

use crate::v1::input::{ClientRunState, InputEvent, SignalKind};
use crate::v1::window::WindowHandle;

/// A steady-state message sent from the daemon to a client.
///
/// Adjacently tagged with a trailing [`DaemonToClient::Unknown`] so a message
/// kind added in a later minor version is skipped rather than fatal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum DaemonToClient {
    /// Forwarded input to replay into the client's PTY.
    Input {
        /// The input event to replay.
        event: InputEvent,
    },
    /// New authoritative run-state for the client.
    StateChange {
        /// The state the daemon assigned to the client.
        state: ClientRunState,
    },
    /// Visual selection-highlight toggle.
    Highlight {
        /// `true` to highlight the client, `false` to clear it.
        on: bool,
    },
    /// Hand off focus using a window-manager activation token.
    Activate {
        /// The opaque activation token to present to the compositor.
        token: String,
    },
    /// Orderly shutdown request.
    Terminate {
        /// Human-readable reason for the shutdown.
        reason: String,
    },
    /// Out-of-band process-group signal (Ctrl+Break, Ctrl+C).
    Signal(SignalKind),
    /// Idle liveness probe.
    KeepAlive,
    /// A message not defined in this protocol minor version.
    #[serde(other)]
    Unknown,
}

/// A message sent from a client to the daemon.
///
/// Adjacently tagged with a trailing [`ClientToDaemon::Unknown`] so a message
/// kind added in a later minor version is skipped rather than fatal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum ClientToDaemon {
    /// Client is up; reports the child process and its window handle.
    Ready {
        /// Process id of the spawned child (the program run in the client,
        /// typically `ssh`).
        child_pid: u32,
        /// The client's window handle.
        window: WindowHandle,
    },
    /// The client's window identity changed.
    WindowChanged {
        /// The new window handle.
        window: WindowHandle,
    },
    /// The child process exited.
    ChildExited {
        /// Exit status code of the child process.
        code: i32,
    },
    /// Idle liveness probe.
    KeepAlive,
    /// A message not defined in this protocol minor version.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
#[path = "../tests/v1/test_message.rs"]
mod tests;
