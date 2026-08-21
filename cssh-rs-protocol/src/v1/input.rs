//! Input events, client run states, and out-of-band signals.

use serde_derive::{Deserialize, Serialize};

use crate::v1::keycode::{KeyCode, Modifiers};

/// A single input event forwarded from the daemon to a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum InputEvent {
    /// A decoded keypress.
    Key {
        /// The logical key pressed.
        code: KeyCode,
        /// The modifier bits held during the keypress.
        modifiers: Modifiers,
        /// Composed Unicode text for IME and dead-key input, when the
        /// keypress produced any; omitted from the wire when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// A raw byte sequence (paste, terminal control) with no key decoding.
    Raw {
        /// The literal bytes to write to the client's PTY.
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    /// An event kind not defined in this protocol minor version.
    #[serde(other)]
    Unknown,
}

/// Authoritative run-state the daemon assigns to a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientRunState {
    /// The client receives and replays broadcast input.
    Active,
    /// The daemon suppresses input forwarding to the client.
    Disabled,
    /// A state not defined in this protocol minor version.
    #[serde(other)]
    Unknown,
}

/// An out-of-band control signal delivered to a client's process group.
///
/// These cannot ride the input stream: Ctrl+Break has no input-byte
/// encoding, and Ctrl+C is delivered as a process-group signal rather than a
/// `0x03` byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    /// `CTRL_BREAK_EVENT` - no input byte maps to it.
    Break,
    /// `CTRL_C_EVENT` / SIGINT.
    Interrupt,
    /// A signal not defined in this protocol minor version.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
#[path = "../tests/v1/test_input.rs"]
mod tests;
