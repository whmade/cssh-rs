//! Encoding of keystrokes into the terminal bytes written to the PTY master.

use cssh_rs_protocol::v1::input::InputEvent;
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use windows::Win32::System::Console::KEY_EVENT_RECORD;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_DELETE, VK_DOWN, VK_END, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4,
    VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT,
    VK_UP,
};

/// Encode a daemon-delivered input event.
///
/// # Arguments
///
/// * `event` - The input event.
///
/// # Returns
///
/// The terminal bytes, empty if the event produces none.
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

/// Encode a key event typed at this window.
///
/// # Arguments
///
/// * `key` - The key-down event.
///
/// # Returns
///
/// The composed character, else the encoding of the virtual key, else nothing.
pub(crate) fn key_event_record_to_bytes(key: &KEY_EVENT_RECORD) -> Vec<u8> {
    let unicode_char = unsafe { key.uChar.UnicodeChar };
    if let Some(character) =
        char::from_u32(u32::from(unicode_char)).filter(|character| return *character != '\0')
    {
        return character.to_string().into_bytes();
    }
    return virtual_key_to_keycode(VIRTUAL_KEY(key.wVirtualKeyCode))
        .map(|code| return key_to_bytes(&code, Modifiers::NONE, None))
        .unwrap_or_default();
}

/// Encode a key with its modifiers and optional composed text.
fn key_to_bytes(code: &KeyCode, modifiers: Modifiers, text: Option<&str>) -> Vec<u8> {
    let is_chord = modifiers.contains(Modifiers::CTRL)
        || modifiers.contains(Modifiers::ALT)
        || modifiers.contains(Modifiers::META);
    if let Some(text) = text.filter(|text| return !is_chord && !text.is_empty()) {
        return text.as_bytes().to_vec();
    }

    let bytes = match code {
        KeyCode::Char(text) if modifiers.contains(Modifiers::CTRL) => ctrl_byte(text)
            .map_or_else(|| return text.as_bytes().to_vec(), |byte| return vec![byte]),
        KeyCode::Char(text) => text.as_bytes().to_vec(),
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
        KeyCode::F(number) => function_key_bytes(*number),
        KeyCode::Modifier(_) | KeyCode::Unknown => Vec::new(),
    };

    // Terminals send Alt as an ESC prefix.
    if modifiers.contains(Modifiers::ALT) && !bytes.is_empty() {
        return [b"\x1b".as_slice(), &bytes].concat();
    }
    return bytes;
}

/// Return the control byte for Ctrl plus a single character in `@`..`_`, case-insensitive.
fn ctrl_byte(text: &str) -> Option<u8> {
    let mut characters = text.chars();
    let character = characters.next()?.to_ascii_uppercase();
    if characters.next().is_some() || !('@'..='_').contains(&character) {
        return None;
    }
    return Some(character as u8 & 0x1f);
}

/// Return the xterm sequence for function key `number`.
fn function_key_bytes(number: u8) -> Vec<u8> {
    match number {
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

pub(crate) fn virtual_key_to_keycode(virtual_key: VIRTUAL_KEY) -> Option<KeyCode> {
    match virtual_key {
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
