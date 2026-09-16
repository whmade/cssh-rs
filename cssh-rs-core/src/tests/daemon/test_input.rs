//! Unit tests for the daemon's key-event -> v1 input conversion.

use cssh_rs_protocol::v1::input::{InputEvent, SignalKind};
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use windows::Win32::System::Console::{
    INPUT_RECORD_0, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED,
    RIGHT_ALT_PRESSED, SHIFT_PRESSED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_C, VK_CANCEL, VK_UP};

use crate::daemon::input::{input_record_to_event, signal_for_key};

/// Build a `KEY_EVENT_RECORD` with the given char, virtual key, and modifiers.
fn key(unicode: u16, vk: u16, control_key_state: u32) -> KEY_EVENT_RECORD {
    return KEY_EVENT_RECORD {
        bKeyDown: true.into(),
        wRepeatCount: 1,
        wVirtualKeyCode: vk,
        wVirtualScanCode: 0,
        uChar: KEY_EVENT_RECORD_0 {
            UnicodeChar: unicode,
        },
        dwControlKeyState: control_key_state,
    };
}

fn record(key: KEY_EVENT_RECORD) -> INPUT_RECORD_0 {
    return INPUT_RECORD_0 { KeyEvent: key };
}

#[test]
fn printable_char_becomes_key_char_with_text() {
    let event = input_record_to_event(&record(key(u16::from(b'a'), u16::from(b'A'), 0)));
    assert_eq!(
        event,
        InputEvent::Key {
            code: KeyCode::Char("a".to_string()),
            modifiers: Modifiers::NONE,
            text: Some("a".to_string()),
        }
    );
}

#[test]
fn shift_modifier_is_reported() {
    let event = input_record_to_event(&record(key(
        u16::from(b'A'),
        u16::from(b'A'),
        SHIFT_PRESSED,
    )));
    match event {
        InputEvent::Key {
            code,
            modifiers,
            text,
        } => {
            assert_eq!(code, KeyCode::Char("A".to_string()));
            assert!(modifiers.contains(Modifiers::SHIFT));
            assert_eq!(text, Some("A".to_string()));
        }
        _ => panic!("expected a Key event"),
    }
}

#[test]
fn navigation_key_without_char_maps_via_virtual_key() {
    let event = input_record_to_event(&record(key(0, VK_UP.0, 0)));
    assert_eq!(
        event,
        InputEvent::Key {
            code: KeyCode::Up,
            modifiers: Modifiers::NONE,
            text: None,
        }
    );
}

#[test]
fn unmapped_no_char_key_is_unknown() {
    let event = input_record_to_event(&record(key(0, 0, LEFT_ALT_PRESSED)));
    match event {
        InputEvent::Key {
            code, modifiers, ..
        } => {
            assert_eq!(code, KeyCode::Unknown);
            assert!(modifiers.contains(Modifiers::ALT));
        }
        _ => panic!("expected a Key event"),
    }
}

#[test]
fn only_ctrl_break_classifies_as_a_signal() {
    // Ctrl+Break has no byte encoding, so it rides out-of-band as a signal.
    assert_eq!(
        signal_for_key(&key(0, VK_CANCEL.0, LEFT_CTRL_PRESSED)),
        Some(SignalKind::Break)
    );
    // Ctrl+C carries the composed 0x03 byte and stays plain input, never a signal.
    assert_eq!(signal_for_key(&key(0x03, VK_C.0, LEFT_CTRL_PRESSED)), None);
    // AltGr+C reports a synthetic Ctrl bit but composes a character; it must not
    // be misread as an interrupt.
    assert_eq!(
        signal_for_key(&key(
            u16::from(b'c'),
            VK_C.0,
            LEFT_CTRL_PRESSED | RIGHT_ALT_PRESSED
        )),
        None
    );
    // A plain letter is not a signal.
    assert_eq!(
        signal_for_key(&key(u16::from(b'a'), u16::from(b'A'), 0)),
        None
    );
}

#[test]
fn ctrl_c_becomes_the_composed_0x03_input_event() {
    let event = input_record_to_event(&record(key(0x03, VK_C.0, LEFT_CTRL_PRESSED)));
    assert_eq!(
        event,
        InputEvent::Key {
            code: KeyCode::Char("\u{3}".to_string()),
            modifiers: Modifiers::CTRL,
            text: Some("\u{3}".to_string()),
        }
    );
}
