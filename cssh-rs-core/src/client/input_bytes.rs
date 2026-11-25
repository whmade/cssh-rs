//! Encoding of keystrokes into the terminal bytes written to the PTY master,
//! and the key-event record helpers the daemon shares.

use std::ops::RangeInclusive;

use cssh_rs_protocol::v1::input::InputEvent;
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use log::warn;
use windows::Win32::System::Console::{
    KEY_EVENT_RECORD, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_DELETE, VK_DOWN, VK_END, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3, VK_F4,
    VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR,
    VK_RIGHT, VK_UP,
};

/// The `dwControlKeyState` flags of either Ctrl key.
pub(crate) const CTRL_PRESSED: u32 = LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED;

/// The `dwControlKeyState` flags of either Alt key.
pub(crate) const ALT_PRESSED: u32 = LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED;

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

const HIGH_SURROGATES: RangeInclusive<u16> = 0xd800..=0xdbff;

/// A keystroke read from a key-event record.
pub(crate) struct Keystroke {
    /// The virtual key of the record that completed the keystroke.
    pub(crate) virtual_key: VIRTUAL_KEY,
    /// The `dwControlKeyState` of the record that completed the keystroke.
    control_key_state: u32,
    /// The composed text, `None` for a key such as an arrow.
    pub(crate) composed_text: Option<char>,
}

impl Keystroke {
    /// Return the key code of the keystroke.
    ///
    /// # Returns
    ///
    /// The composed text as a character key, else the code of the virtual
    /// key, else `None`.
    pub(crate) fn key_code(&self) -> Option<KeyCode> {
        match self.composed_text {
            Some(character) => return Some(KeyCode::Char(character.to_string())),
            None => return virtual_key_to_keycode(self.virtual_key),
        }
    }

    /// Return whether the keystroke's Ctrl and Alt only selected an AltGr character.
    ///
    /// Windows reports AltGr as Ctrl+Alt and treats any Ctrl+Alt as AltGr, so
    /// this follows Windows Terminal's rule:
    /// <https://github.com/microsoft/terminal/blob/v1.25.3131.0/src/terminal/input/terminalInput.cpp#L390>.
    fn is_altgr_character(&self) -> bool {
        let state = self.control_key_state;
        return state & CTRL_PRESSED != 0
            && state & ALT_PRESSED != 0
            && self
                .composed_text
                .is_some_and(|character| return character > ' ' && character != '\u{7f}');
    }

    /// Return `dwControlKeyState` without a Ctrl and Alt that only selected an AltGr character.
    ///
    /// As in Windows Terminal, whose rule [`Keystroke::is_altgr_character`]
    /// follows, holding both Ctrl keys keeps Ctrl and holding both Alt keys
    /// keeps Alt. Unlike it, right Ctrl with AltGr's right Alt keeps Ctrl too:
    /// AltGr only ever fakes the left Ctrl, and when right Ctrl is pressed
    /// first Windows reports no left Ctrl at all, as this `ReadConsoleInputW`
    /// capture of right Ctrl, then AltGr + sharp-s on a German layout shows:
    ///
    /// ```text
    /// down  vk=0x11 (CONTROL)  scan=0x1d  char=U+0000  state=0x0104 RIGHT_CTRL|ENHANCED
    /// down  vk=0x12 (MENU)     scan=0x38  char=U+0000  state=0x0105 RIGHT_ALT|RIGHT_CTRL|ENHANCED
    /// down  vk=0xdb (OEM_4)    scan=0x0c  char=U+005C '\\'  state=0x0005 RIGHT_ALT|RIGHT_CTRL
    /// ```
    ///
    /// # Returns
    ///
    /// The `dwControlKeyState` of the record that completed the keystroke,
    /// minus its Ctrl and Alt flags if [`Keystroke::is_altgr_character`]
    /// matches, except a Ctrl or Alt kept as above.
    pub(crate) fn control_key_state_without_altgr(&self) -> u32 {
        let state = self.control_key_state;
        if !self.is_altgr_character() {
            return state;
        }
        let holds = |flags: u32| return state & flags == flags;
        let keeps_ctrl = holds(RIGHT_CTRL_PRESSED | RIGHT_ALT_PRESSED)
            || holds(LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED);
        let keeps_alt = holds(LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED);
        let mut kept = state;
        if !keeps_ctrl {
            kept &= !CTRL_PRESSED;
        }
        if !keeps_alt {
            kept &= !ALT_PRESSED;
        }
        return kept;
    }
}

/// A decoder of the keystrokes in the key-event records one console delivers.
///
/// The console splits a character outside the BMP into its two UTF-16
/// surrogates, each in its own record, which Windows Terminal pairs up too:
/// <https://github.com/microsoft/terminal/blob/v1.25.3131.0/src/terminal/input/terminalInput.cpp#L244-L255>.
///
/// The Alt release that ends an Alt code carries composed text too: the
/// inbox conhost pastes a UTF-16 unit that no key types and that is no
/// letter, each half of an emoji among them, as an Alt code:
/// <https://github.com/microsoft/terminal/blob/v1.25.3131.0/src/interactivity/base/EventSynthesis.cpp#L35-L63>,
/// <https://github.com/microsoft/terminal/blob/v1.25.3131.0/src/interactivity/base/EventSynthesis.cpp#L125-L156>.
/// An Alt code typed on the numeric keypad ends in the same release:
/// <https://github.com/microsoft/terminal/blob/v1.25.3131.0/src/terminal/input/terminalInput.cpp#L333-L338>.
#[derive(Default)]
pub(crate) struct KeystrokeDecoder {
    high_surrogate: Option<u16>,
}

impl KeystrokeDecoder {
    /// Return the keystroke a key-event record completes.
    ///
    /// The record holds that keystroke [`keystroke_count`] times, so decode
    /// each record once, not once per keystroke. A record without composed
    /// text leaves a pending high surrogate in place; a surrogate without its
    /// other half is dropped.
    ///
    /// # Arguments
    ///
    /// * `key` - The next key-event record read from the console.
    ///
    /// # Returns
    ///
    /// The keystroke of a key-down record or an Alt code's release, or
    /// `None` for any other key release or a record holding the first half
    /// of a character.
    pub(crate) fn decode(&mut self, key: &KEY_EVENT_RECORD) -> Option<Keystroke> {
        let unit = unsafe { key.uChar.UnicodeChar };
        let is_alt_code_release = key.wVirtualKeyCode == VK_MENU.0 && unit != 0;
        if !key.bKeyDown.as_bool() && !is_alt_code_release {
            return None;
        }
        if HIGH_SURROGATES.contains(&unit) {
            self.high_surrogate = Some(unit);
            return None;
        }
        let composed_text = match unit {
            0 => None,
            _ => char::decode_utf16(self.high_surrogate.take().into_iter().chain([unit]))
                .last()
                .and_then(Result::ok),
        };
        return Some(Keystroke {
            virtual_key: VIRTUAL_KEY(key.wVirtualKeyCode),
            control_key_state: key.dwControlKeyState,
            composed_text,
        });
    }
}

/// Encode every keystroke a key-event record read at this window holds.
///
/// # Arguments
///
/// * `key` - The key-event record.
/// * `decoder` - The decoder of the records read before it at this window.
///
/// # Returns
///
/// The bytes of the keystroke `decoder` reads from `key`, repeated
/// [`keystroke_count`] times: the composed text, else the encoding of the
/// virtual key, else nothing.
pub(crate) fn key_event_record_to_bytes(
    key: &KEY_EVENT_RECORD,
    decoder: &mut KeystrokeDecoder,
) -> Vec<u8> {
    let Some(keystroke) = decoder.decode(key) else {
        return Vec::new();
    };
    let bytes = keystroke
        .key_code()
        .map(|code| return key_to_bytes(&code, Modifiers::NONE, None))
        .unwrap_or_default();
    return bytes.repeat(keystroke_count(key));
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

fn virtual_key_to_keycode(virtual_key: VIRTUAL_KEY) -> Option<KeyCode> {
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
pub(crate) mod tests;
