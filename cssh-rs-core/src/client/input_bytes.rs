//! Encoding of keystrokes into the terminal bytes written to the PTY master.

use cssh_rs_protocol::v1::input::InputEvent;
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use log::warn;
use windows::Win32::System::Console::{
    KEY_EVENT_RECORD, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED,
};
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

/// Encode every keystroke a key-event record read at this window holds.
///
/// # Arguments
///
/// * `key` - The key-down record.
///
/// # Returns
///
/// The bytes of one keystroke, repeated [`keystroke_count`] times: the
/// composed character, else the encoding of the virtual key, else nothing.
pub(crate) fn key_event_record_to_bytes(key: &KEY_EVENT_RECORD) -> Vec<u8> {
    let unicode_char = unsafe { key.uChar.UnicodeChar };
    let keystroke = match char::from_u32(u32::from(unicode_char))
        .filter(|character| return *character != '\0')
    {
        Some(character) => character.to_string().into_bytes(),
        None => virtual_key_to_keycode(VIRTUAL_KEY(key.wVirtualKeyCode))
            .map(|code| return key_to_bytes(&code, Modifiers::NONE, None))
            .unwrap_or_default(),
    };
    return keystroke.repeat(keystroke_count(key));
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

/// Return the number of keystrokes a key-event record holds.
///
/// The console host merges unread auto-repeats of a held key into one record:
/// <https://learn.microsoft.com/en-us/windows/console/key-event-record-str>.
/// Only programs that inject input send a repeat count of 0, so it logs a warning.
///
/// # Arguments
///
/// * `key` - The key-event record.
///
/// # Returns
///
/// `wRepeatCount`, or 1 if it is 0.
pub(crate) fn keystroke_count(key: &KEY_EVENT_RECORD) -> usize {
    if key.wRepeatCount == 0 {
        warn!("Key-event record has a repeat count of 0; treating it as one keystroke");
        return 1;
    }
    return usize::from(key.wRepeatCount);
}

/// Return whether a key-event record's Ctrl and Alt only selected an AltGr character.
///
/// Windows reports AltGr as Ctrl+Alt, treats any Ctrl+Alt as AltGr, and
/// presses AltGr for each pasted character that needs it on the active
/// layout. The rule matches Windows Terminal's and leaves Alt+x,
/// Ctrl+letter and Ctrl+Alt+Esc as chords:
/// <https://github.com/microsoft/terminal/blob/main/src/terminal/input/terminalInput.cpp>.
///
/// # Arguments
///
/// * `key` - The key-event record.
///
/// # Returns
///
/// `true` if a Ctrl and an Alt are held and the composed text is a code
/// point above `0x20` other than `0x7F`.
pub(crate) fn is_altgr_character(key: &KEY_EVENT_RECORD) -> bool {
    let state = key.dwControlKeyState;
    let unicode_char = unsafe { key.uChar.UnicodeChar };
    return state & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0
        && state & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0
        && unicode_char > 0x20
        && unicode_char != 0x7f;
}

/// Return the Ctrl and Alt flags of a key-event record that only selected an AltGr character.
///
/// Holding the right Ctrl key with AltGr's right Alt, or both Ctrl keys,
/// keeps Ctrl; holding both Alt keys keeps Alt. Windows Terminal, whose
/// rule [`is_altgr_character`] follows, keeps Ctrl only when both Ctrl
/// flags are set. But AltGr only ever fakes the left Ctrl, and when right
/// Ctrl is pressed first Windows reports no left Ctrl at all, as this
/// `ReadConsoleInputW` capture of right Ctrl, then AltGr + sharp-s on a
/// German layout shows:
///
/// ```text
/// down  vk=0x11 (CONTROL)  scan=0x1d  char=U+0000  state=0x0104 RIGHT_CTRL|ENHANCED
/// down  vk=0x12 (MENU)     scan=0x38  char=U+0000  state=0x0105 RIGHT_ALT|RIGHT_CTRL|ENHANCED
/// down  vk=0xdb (OEM_4)    scan=0x0c  char=U+005C '\\'  state=0x0005 RIGHT_ALT|RIGHT_CTRL
/// ```
///
/// # Arguments
///
/// * `key` - The key-event record.
///
/// # Returns
///
/// The `dwControlKeyState` flags to leave out of the keystroke's modifiers,
/// or `0` unless [`is_altgr_character`] matches.
pub(crate) fn altgr_selector_flags(key: &KEY_EVENT_RECORD) -> u32 {
    if !is_altgr_character(key) {
        return 0;
    }
    let state = key.dwControlKeyState;
    let holds = |flags: u32| return state & flags == flags;
    let keeps_ctrl = holds(RIGHT_CTRL_PRESSED | RIGHT_ALT_PRESSED)
        || holds(LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED);
    let keeps_alt = holds(LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED);
    let mut flags = 0;
    if !keeps_ctrl {
        flags |= LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED;
    }
    if !keeps_alt {
        flags |= LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED;
    }
    return flags;
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
