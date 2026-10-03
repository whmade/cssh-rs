//! Pure conversion of decoded input into the terminal bytes written to the
//! PTY master.
//!
//! The daemon forwards keystrokes as semantic [`InputEvent::Key`] values; the
//! client is the terminal-facing side and turns each one into the VT byte
//! sequence a program running under the PTY expects. Local keystrokes typed
//! directly at the client window are encoded the same way so both input
//! sources merge into one stream. This module holds the single source of truth
//! for that mapping and is exercised entirely by unit tests.

use cssh_rs_protocol::v1::input::InputEvent;
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use windows::Win32::System::Console::KEY_EVENT_RECORD;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_DELETE, VK_DOWN, VK_END, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4,
    VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT,
    VK_UP,
};

/// Convert a daemon-delivered [`InputEvent`] into the bytes to write to the
/// PTY master.
///
/// # Arguments
///
/// * `event` - The decoded input event forwarded by the daemon.
///
/// # Returns
///
/// The terminal byte sequence, empty for events that produce no output.
pub(crate) fn input_event_to_bytes(event: &InputEvent) -> Vec<u8> {
    match event {
        InputEvent::Key {
            code,
            modifiers,
            text,
        } => return key_to_bytes(code, *modifiers, text.as_deref()),
        InputEvent::Raw { bytes } => return bytes.clone(),
        InputEvent::Unknown => return Vec::new(),
    }
}

/// Convert a locally-typed key event from this window's console into bytes.
///
/// The composed `UnicodeChar` (already resolved for layout, AltGr, dead keys,
/// and control combinations by the console) is written verbatim; keys with no
/// character (arrows, function, navigation) are encoded from the virtual key
/// code through the same table as daemon-delivered keys.
///
/// # Arguments
///
/// * `key` - A key-down `KEY_EVENT_RECORD` read from the local console.
///
/// # Returns
///
/// The terminal byte sequence, empty when the key produces no output.
pub(crate) fn key_event_record_to_bytes(key: &KEY_EVENT_RECORD) -> Vec<u8> {
    // A composed character (including control bytes like 0x03 for Ctrl+C) is
    // delivered verbatim; only keys that produce no character (UnicodeChar 0)
    // fall through to the virtual-key table.
    let unicode = unsafe { key.uChar.UnicodeChar };
    if unicode != 0 {
        if let Some(ch) = char::from_u32(u32::from(unicode)).filter(|c| return *c != '\0') {
            let mut buf = [0u8; 4];
            return ch.encode_utf8(&mut buf).as_bytes().to_vec();
        }
    }
    if let Some(code) = virtual_key_to_keycode(VIRTUAL_KEY(key.wVirtualKeyCode)) {
        return key_to_bytes(&code, Modifiers::NONE, None);
    }
    return Vec::new();
}

/// Map a logical key plus its modifiers to a terminal byte sequence.
fn key_to_bytes(code: &KeyCode, modifiers: Modifiers, text: Option<&str>) -> Vec<u8> {
    // Composed text with no control/alt/meta held is the primary path: it
    // carries the final Unicode for printable keys, IME, dead keys, and AltGr.
    let has_ctrl_alt_meta = modifiers.contains(Modifiers::CTRL)
        || modifiers.contains(Modifiers::ALT)
        || modifiers.contains(Modifiers::META);
    if !has_ctrl_alt_meta {
        if let Some(text) = text {
            if !text.is_empty() {
                return text.as_bytes().to_vec();
            }
        }
    }

    let base: Vec<u8> = match code {
        KeyCode::Char(s) => {
            if modifiers.contains(Modifiers::CTRL) {
                match ctrl_byte(s) {
                    Some(byte) => vec![byte],
                    None => s.as_bytes().to_vec(),
                }
            } else {
                s.as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Escape => vec![0x1b],
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::F(n) => function_key_bytes(*n),
        KeyCode::Modifier(_) => Vec::new(),
        KeyCode::Unknown => Vec::new(),
    };

    // Alt is delivered as an ESC prefix on the key's byte sequence.
    if modifiers.contains(Modifiers::ALT) && !base.is_empty() {
        let mut out = Vec::with_capacity(base.len() + 1);
        out.push(0x1b);
        out.extend_from_slice(&base);
        return out;
    }
    return base;
}

/// Return the control byte for `Ctrl + <s>` when `s` is a single ASCII
/// character in the `@`..`_` range (case-insensitive for letters), else `None`.
fn ctrl_byte(s: &str) -> Option<u8> {
    let mut chars = s.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let upper = c.to_ascii_uppercase();
    if ('@'..='_').contains(&upper) {
        return Some((upper as u8) & 0x1f);
    }
    return None;
}

/// Return the xterm byte sequence for function key `n` (`F1`..`F12`).
fn function_key_bytes(n: u8) -> Vec<u8> {
    match n {
        1 => return b"\x1bOP".to_vec(),
        2 => return b"\x1bOQ".to_vec(),
        3 => return b"\x1bOR".to_vec(),
        4 => return b"\x1bOS".to_vec(),
        5 => return b"\x1b[15~".to_vec(),
        6 => return b"\x1b[17~".to_vec(),
        7 => return b"\x1b[18~".to_vec(),
        8 => return b"\x1b[19~".to_vec(),
        9 => return b"\x1b[20~".to_vec(),
        10 => return b"\x1b[21~".to_vec(),
        11 => return b"\x1b[23~".to_vec(),
        12 => return b"\x1b[24~".to_vec(),
        _ => return Vec::new(),
    }
}

/// Map a virtual key code with no composed character to a [`KeyCode`].
fn virtual_key_to_keycode(vk: VIRTUAL_KEY) -> Option<KeyCode> {
    match vk {
        VK_UP => return Some(KeyCode::Up),
        VK_DOWN => return Some(KeyCode::Down),
        VK_LEFT => return Some(KeyCode::Left),
        VK_RIGHT => return Some(KeyCode::Right),
        VK_HOME => return Some(KeyCode::Home),
        VK_END => return Some(KeyCode::End),
        VK_PRIOR => return Some(KeyCode::PageUp),
        VK_NEXT => return Some(KeyCode::PageDown),
        VK_INSERT => return Some(KeyCode::Insert),
        VK_DELETE => return Some(KeyCode::Delete),
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
#[path = "../tests/client/test_input_bytes.rs"]
mod tests;
