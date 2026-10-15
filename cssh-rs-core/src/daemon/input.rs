//! Conversion of daemon keystrokes into v1 messages.

use cssh_rs_protocol::v1::input::{InputEvent, SignalKind};
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use cssh_rs_protocol::v1::message::DaemonToClient;
use windows::Win32::System::Console::{
    KEY_EVENT_RECORD, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED,
    SHIFT_PRESSED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_CANCEL};

use crate::client::input_bytes::virtual_key_to_keycode;

/// Return the message that broadcasts a key-down event to the clients.
///
/// # Arguments
/// * `key` - The key-down event.
///
/// # Returns
/// A `Break` signal for Ctrl+Break, which has no input byte, otherwise the key.
pub fn key_event_to_message(key: &KEY_EVENT_RECORD) -> DaemonToClient {
    let virtual_key = VIRTUAL_KEY(key.wVirtualKeyCode);
    if virtual_key == VK_CANCEL {
        return DaemonToClient::Signal(SignalKind::Break);
    }
    let unicode_char = unsafe { key.uChar.UnicodeChar };
    let text = char::from_u32(u32::from(unicode_char))
        .filter(|character| return *character != '\0')
        .map(String::from);
    let code = match &text {
        Some(text) => KeyCode::Char(text.clone()),
        None => virtual_key_to_keycode(virtual_key).unwrap_or(KeyCode::Unknown),
    };
    return DaemonToClient::Input {
        event: InputEvent::Key {
            code,
            modifiers: modifiers_from_state(key.dwControlKeyState),
            text,
        },
    };
}

fn modifiers_from_state(state: u32) -> Modifiers {
    let mut modifiers = Modifiers::NONE;
    if state & SHIFT_PRESSED != 0 {
        modifiers = modifiers | Modifiers::SHIFT;
    }
    if state & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0 {
        modifiers = modifiers | Modifiers::CTRL;
    }
    if state & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0 {
        modifiers = modifiers | Modifiers::ALT;
    }
    return modifiers;
}

#[cfg(test)]
#[path = "../tests/daemon/test_input.rs"]
mod tests;
