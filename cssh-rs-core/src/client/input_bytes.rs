//! Encoding of keystrokes into the terminal bytes written to the PTY master,
//! and the key-event record helpers the daemon shares.

use std::ops::RangeInclusive;

use cssh_rs_protocol::v1::input::InputEvent;
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use log::warn;
use windows::Win32::Foundation::FALSE;
use windows::Win32::System::Console::{
    KEY_EVENT_RECORD, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED,
    SHIFT_PRESSED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_BACK, VK_CANCEL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F10, VK_F11,
    VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LEFT,
    VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP,
};

use crate::utils::windows::CTRL_BREAK_KEY_DOWN;

/// The `dwControlKeyState` flags of either Ctrl key.
pub(crate) const CTRL_PRESSED: u32 = LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED;

/// The `dwControlKeyState` flags of either Alt key.
pub(crate) const ALT_PRESSED: u32 = LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED;

/// Ctrl+C as a terminal sends it: ETX, which ConPTY turns back into Ctrl+C.
pub(crate) const CTRL_C_KEYSTROKE: &[u8] = b"\x03";

/// Return Ctrl+Break's key-down and key-up in win32-input-mode.
///
/// `GenerateConsoleCtrlEvent` cannot reach the SSH child on its pseudoconsole,
/// but ConPTY handles this key as if pressed in its own console and raises
/// `CTRL_BREAK_EVENT` in every process attached to it:
/// <https://github.com/microsoft/terminal/blob/v1.25.3131.0/src/host/input.cpp#L134>.
/// Windows 10's inbox ConPTY predates win32-input-mode, so there Ctrl+Break
/// does not reach the child.
///
/// # Returns
///
/// The encoding of [`CTRL_BREAK_KEY_DOWN`], then of its release.
pub(crate) fn ctrl_break_keystroke() -> Vec<u8> {
    let key_up = KEY_EVENT_RECORD {
        bKeyDown: FALSE,
        ..CTRL_BREAK_KEY_DOWN
    };
    return [
        win32_input_mode_bytes(&CTRL_BREAK_KEY_DOWN),
        win32_input_mode_bytes(&key_up),
    ]
    .concat();
}

/// Encode a key-event record as `ESC [ Vk ; Sc ; Uc ; Kd ; Cs ; Rc _`:
/// <https://github.com/microsoft/terminal/blob/v1.25.3131.0/doc/specs/%234999%20-%20Improved%20keyboard%20handling%20in%20Conpty.md>.
fn win32_input_mode_bytes(key: &KEY_EVENT_RECORD) -> Vec<u8> {
    return format!(
        "\x1b[{};{};{};{};{};{}_",
        key.wVirtualKeyCode,
        key.wVirtualScanCode,
        unsafe { key.uChar.UnicodeChar },
        u8::from(key.bKeyDown.as_bool()),
        key.dwControlKeyState,
        key.wRepeatCount
    )
    .into_bytes();
}

/// Encode an input event, delivered by the daemon or typed at this window.
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

/// Control Sequence Introducer.
const CSI: &str = "\x1b[";
/// Single Shift Three.
const SS3: &str = "\x1bO";

/// A keystroke read from a key-event record.
pub(crate) struct Keystroke {
    /// The virtual key of the record that completed the keystroke.
    virtual_key: VIRTUAL_KEY,
    /// The `dwControlKeyState` of the record that completed the keystroke.
    control_key_state: u32,
    /// The composed text, `None` for a key such as an arrow.
    composed_text: Option<char>,
}

impl Keystroke {
    /// Return the input event of the keystroke, as both the daemon and a client send it.
    ///
    /// Keyboard layouts compose Backspace, Enter, Tab and Esc into their
    /// legacy control characters, as the US one does, so the virtual key goes
    /// before the composed text:
    /// <https://github.com/microsoft/Windows-driver-samples/blob/d49bb7d72f3651952b2b6e4ad28d14fe2247eb69/input/layout/kbdus/kbdus.c#L238>,
    /// <https://github.com/microsoft/Windows-driver-samples/blob/d49bb7d72f3651952b2b6e4ad28d14fe2247eb69/input/layout/kbdus/kbdus.c#L253-L255>.
    ///
    /// # Returns
    ///
    /// The code of the virtual key, else the composed text as a character
    /// key with that text, else [`KeyCode::Unknown`], with the modifiers of
    /// [`Keystroke::control_key_state_without_altgr`].
    pub(crate) fn input_event(&self) -> InputEvent {
        let (code, text) = match (virtual_key_to_keycode(self.virtual_key), self.composed_text) {
            (Some(code), _) => (code, None),
            (None, Some(character)) => (
                KeyCode::Char(character.to_string()),
                Some(character.to_string()),
            ),
            (None, None) => (KeyCode::Unknown, None),
        };
        return InputEvent::Key {
            code,
            modifiers: modifiers_from_state(self.control_key_state_without_altgr()),
            text,
        };
    }

    /// Return whether the keystroke is Ctrl+Break.
    ///
    /// # Returns
    ///
    /// `true` if its virtual key is `VK_CANCEL`.
    pub(crate) fn is_ctrl_break(&self) -> bool {
        return self.virtual_key == VK_CANCEL;
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
    fn control_key_state_without_altgr(&self) -> u32 {
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
/// [`keystroke_count`] times: [`ctrl_break_keystroke`] for Ctrl+Break, else
/// the encoding of its [`Keystroke::input_event`].
pub(crate) fn key_event_record_to_bytes(
    key: &KEY_EVENT_RECORD,
    decoder: &mut KeystrokeDecoder,
) -> Vec<u8> {
    let Some(keystroke) = decoder.decode(key) else {
        return Vec::new();
    };
    if keystroke.is_ctrl_break() {
        return ctrl_break_keystroke().repeat(keystroke_count(key));
    }
    return input_event_to_bytes(&keystroke.input_event()).repeat(keystroke_count(key));
}

/// Encode a key with its modifiers and optional composed text.
///
/// Navigation and function keys carry their modifiers in xterm's modifier
/// parameter; any other key sends Alt as an ESC prefix. As in Windows
/// Terminal, Ctrl turns Backspace's DEL into BS and Enter's CR into LF, and
/// Shift turns Tab into CBT:
/// <https://github.com/microsoft/terminal/blob/v1.25.3131.0/src/terminal/input/terminalInput.cpp#L911-L967>.
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
        KeyCode::Enter if modifiers.contains(Modifiers::CTRL) => vec![b'\n'],
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab if modifiers.contains(Modifiers::SHIFT) => format!("{CSI}Z").into_bytes(),
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Backspace if modifiers.contains(Modifiers::CTRL) => vec![0x08],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Escape => vec![0x1b],
        KeyCode::Delete => return tilde_final_key_bytes(3, modifiers),
        KeyCode::Insert => return tilde_final_key_bytes(2, modifiers),
        KeyCode::Up => return letter_final_key_bytes(CSI, 'A', modifiers),
        KeyCode::Down => return letter_final_key_bytes(CSI, 'B', modifiers),
        KeyCode::Right => return letter_final_key_bytes(CSI, 'C', modifiers),
        KeyCode::Left => return letter_final_key_bytes(CSI, 'D', modifiers),
        KeyCode::Home => return letter_final_key_bytes(CSI, 'H', modifiers),
        KeyCode::End => return letter_final_key_bytes(CSI, 'F', modifiers),
        KeyCode::PageUp => return tilde_final_key_bytes(5, modifiers),
        KeyCode::PageDown => return tilde_final_key_bytes(6, modifiers),
        KeyCode::F(number) => return function_key_bytes(*number, modifiers),
        KeyCode::Modifier(_) | KeyCode::Unknown => Vec::new(),
    };

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

/// Return the xterm sequence for function key `number` with its modifiers.
fn function_key_bytes(number: u8, modifiers: Modifiers) -> Vec<u8> {
    match number {
        1 => return letter_final_key_bytes(SS3, 'P', modifiers),
        2 => return letter_final_key_bytes(SS3, 'Q', modifiers),
        3 => return letter_final_key_bytes(SS3, 'R', modifiers),
        4 => return letter_final_key_bytes(SS3, 'S', modifiers),
        5 => return tilde_final_key_bytes(15, modifiers),
        6 => return tilde_final_key_bytes(17, modifiers),
        7 => return tilde_final_key_bytes(18, modifiers),
        8 => return tilde_final_key_bytes(19, modifiers),
        9 => return tilde_final_key_bytes(20, modifiers),
        10 => return tilde_final_key_bytes(21, modifiers),
        11 => return tilde_final_key_bytes(23, modifiers),
        12 => return tilde_final_key_bytes(24, modifiers),
        _ => return Vec::new(),
    }
}

/// Return `introducer final_byte`, or `CSI 1 ; m final_byte` with modifiers.
fn letter_final_key_bytes(introducer: &str, final_byte: char, modifiers: Modifiers) -> Vec<u8> {
    let sequence = match modifier_parameter(modifiers) {
        None => format!("{introducer}{final_byte}"),
        Some(parameter) => format!("{CSI}1;{parameter}{final_byte}"),
    };
    return sequence.into_bytes();
}

/// Return `CSI number ~`, or `CSI number ; m ~` with modifiers.
fn tilde_final_key_bytes(number: u8, modifiers: Modifiers) -> Vec<u8> {
    let sequence = match modifier_parameter(modifiers) {
        None => format!("{CSI}{number}~"),
        Some(parameter) => format!("{CSI}{number};{parameter}~"),
    };
    return sequence.into_bytes();
}

/// Return xterm's modifier parameter `m`, or `None` if no Shift, Alt or Ctrl is held:
/// <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h3-PC-Style-Function-Keys>.
fn modifier_parameter(modifiers: Modifiers) -> Option<u8> {
    let held = |modifier: Modifiers| return u8::from(modifiers.contains(modifier));
    let parameter =
        1 + held(Modifiers::SHIFT) + 2 * held(Modifiers::ALT) + 4 * held(Modifiers::CTRL);
    return (parameter > 1).then_some(parameter);
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
        VK_BACK => return Some(KeyCode::Backspace),
        VK_TAB => return Some(KeyCode::Tab),
        VK_RETURN => return Some(KeyCode::Enter),
        VK_ESCAPE => return Some(KeyCode::Escape),
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
