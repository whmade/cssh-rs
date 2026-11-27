//! Conversion of daemon keystrokes into v1 messages.

use cssh_rs_protocol::v1::input::{InputEvent, SignalKind};
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use cssh_rs_protocol::v1::message::DaemonToClient;
use windows::Win32::System::Console::SHIFT_PRESSED;

use crate::client::input_bytes::{Keystroke, ALT_PRESSED, CTRL_PRESSED};

/// Return the message that broadcasts a keystroke to the clients.
///
/// # Arguments
/// * `keystroke` - The keystroke.
///
/// # Returns
/// A `Break` signal for Ctrl+Break, otherwise the key, its modifiers read
/// from [`Keystroke::control_key_state_without_altgr`].
pub fn keystroke_to_message(keystroke: &Keystroke) -> DaemonToClient {
    if keystroke.is_ctrl_break() {
        return DaemonToClient::Signal(SignalKind::Break);
    }
    return DaemonToClient::Input {
        event: InputEvent::Key {
            code: keystroke.key_code().unwrap_or(KeyCode::Unknown),
            modifiers: modifiers_from_state(keystroke.control_key_state_without_altgr()),
            text: keystroke.composed_text.map(String::from),
        },
    };
}

fn modifiers_from_state(state: u32) -> Modifiers {
    let mut modifiers = Modifiers::NONE;
    if state & SHIFT_PRESSED != 0 {
        modifiers = modifiers | Modifiers::SHIFT;
    }
    if state & CTRL_PRESSED != 0 {
        modifiers = modifiers | Modifiers::CTRL;
    }
    if state & ALT_PRESSED != 0 {
        modifiers = modifiers | Modifiers::ALT;
    }
    return modifiers;
}

#[cfg(test)]
#[path = "../tests/daemon/test_input.rs"]
mod tests;
