//! Conversion of daemon keystrokes into v1 messages.

use cssh_rs_protocol::v1::input::SignalKind;
use cssh_rs_protocol::v1::message::DaemonToClient;

use crate::client::input_bytes::Keystroke;

/// Return the message that broadcasts a keystroke to the clients.
///
/// # Arguments
/// * `keystroke` - The keystroke.
///
/// # Returns
/// A `Break` signal for Ctrl+Break, otherwise its [`Keystroke::input_event`].
pub fn keystroke_to_message(keystroke: &Keystroke) -> DaemonToClient {
    if keystroke.is_ctrl_break() {
        return DaemonToClient::Signal(SignalKind::Break);
    }
    return DaemonToClient::Input {
        event: keystroke.input_event(),
    };
}

#[cfg(test)]
#[path = "../tests/daemon/test_input.rs"]
mod tests;
