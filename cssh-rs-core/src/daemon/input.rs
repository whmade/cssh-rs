//! Convert daemon-captured Windows console key events into v1 protocol input.
//!
//! The daemon emits semantic [`InputEvent::Key`] values (never `Raw` for a
//! keystroke); the client re-encodes them to terminal bytes. Ctrl+C rides the
//! input stream as its composed `0x03` byte; only Ctrl+Break, which has no
//! input-byte encoding, is classified out into a `Signal` delivered to the
//! child's process group.

use cssh_rs_protocol::v1::input::{InputEvent, SignalKind};
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use windows::Win32::System::Console::{
    INPUT_RECORD_0, KEY_EVENT_RECORD, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED,
    RIGHT_CTRL_PRESSED, SHIFT_PRESSED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_BACK, VK_CANCEL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F10, VK_F11,
    VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LEFT,
    VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
};

/// Convert a captured key event into a v1 [`InputEvent::Key`].
///
/// A key that produced a composed character (`UnicodeChar != 0`, covering
/// printable keys, IME, dead keys, AltGr, and control-char combinations) is
/// emitted as [`KeyCode::Char`] with that text; keys with no character
/// (arrows, function, navigation) map through the virtual key code.
///
/// # Arguments
///
/// * `record` - The captured key event record.
///
/// # Returns
///
/// The decoded [`InputEvent::Key`].
pub fn input_record_to_event(record: &INPUT_RECORD_0) -> InputEvent {
    let key = unsafe { record.KeyEvent };
    let modifiers = modifiers_from_state(key.dwControlKeyState);
    let unicode = unsafe { key.uChar.UnicodeChar };
    if unicode != 0 {
        if let Some(ch) = char::from_u32(u32::from(unicode)) {
            let text = ch.to_string();
            return InputEvent::Key {
                code: KeyCode::Char(text.clone()),
                modifiers,
                text: Some(text),
            };
        }
    }
    let code = virtual_key_to_keycode(VIRTUAL_KEY(key.wVirtualKeyCode)).unwrap_or(KeyCode::Unknown);
    return InputEvent::Key {
        code,
        modifiers,
        text: None,
    };
}

/// Classify a key event as the out-of-band Ctrl+Break signal.
///
/// Ctrl+Break has no input-byte encoding, so it is delivered to the child's
/// process group rather than as bytes. Ctrl+C is deliberately not classified
/// here: it carries the composed `0x03` byte and rides the input stream so the
/// client forwards it to the `ssh` child like any other keystroke, which also
/// means a synthetic Ctrl from AltGr can never be misread as an interrupt.
///
/// # Arguments
///
/// * `key` - The captured key event record.
///
/// # Returns
///
/// `Some(SignalKind::Break)` for Ctrl+Break, otherwise `None`.
pub fn signal_for_key(key: &KEY_EVENT_RECORD) -> Option<SignalKind> {
    if key.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) == 0 {
        return None;
    }
    if VIRTUAL_KEY(key.wVirtualKeyCode) == VK_CANCEL {
        return Some(SignalKind::Break);
    }
    return None;
}

/// Map a console control-key-state bitmask to protocol [`Modifiers`].
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

/// Map a virtual key code with no composed character to a [`KeyCode`].
fn virtual_key_to_keycode(vk: VIRTUAL_KEY) -> Option<KeyCode> {
    match vk {
        VK_RETURN => return Some(KeyCode::Enter),
        VK_TAB => return Some(KeyCode::Tab),
        VK_BACK => return Some(KeyCode::Backspace),
        VK_ESCAPE => return Some(KeyCode::Escape),
        VK_DELETE => return Some(KeyCode::Delete),
        VK_INSERT => return Some(KeyCode::Insert),
        VK_UP => return Some(KeyCode::Up),
        VK_DOWN => return Some(KeyCode::Down),
        VK_LEFT => return Some(KeyCode::Left),
        VK_RIGHT => return Some(KeyCode::Right),
        VK_HOME => return Some(KeyCode::Home),
        VK_END => return Some(KeyCode::End),
        VK_PRIOR => return Some(KeyCode::PageUp),
        VK_NEXT => return Some(KeyCode::PageDown),
        VK_F1 => return Some(KeyCode::F(1)),
        VK_F2 => return Some(KeyCode::F(2)),
        VK_F3 => return Some(KeyCode::F(3)),
        VK_F4 => return Some(KeyCode::F(4)),
        VK_F5 => return Some(KeyCode::F(5)),
        VK_F6 => return Some(KeyCode::F(6)),
        VK_F7 => return Some(KeyCode::F(7)),
        VK_F8 => return Some(KeyCode::F(8)),
        VK_F9 => return Some(KeyCode::F(9)),
        VK_F10 => return Some(KeyCode::F(10)),
        VK_F11 => return Some(KeyCode::F(11)),
        VK_F12 => return Some(KeyCode::F(12)),
        _ => return None,
    }
}

#[cfg(test)]
#[path = "../tests/daemon/test_input.rs"]
mod tests;
