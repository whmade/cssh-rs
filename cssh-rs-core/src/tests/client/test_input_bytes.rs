//! Unit tests for the pure Key/Raw -> terminal-bytes conversion.

use cssh_rs_protocol::v1::input::InputEvent;
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use windows::Win32::System::Console::{KEY_EVENT_RECORD, KEY_EVENT_RECORD_0};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_C, VK_DELETE, VK_DOWN, VK_END, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3,
    VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR,
    VK_RIGHT, VK_UP,
};

use crate::client::input_bytes::{input_event_to_bytes, key_event_record_to_bytes};

/// Build a `Key` event with no modifiers and the given composed text.
fn key_text(code: KeyCode, text: &str) -> InputEvent {
    return InputEvent::Key {
        code,
        modifiers: Modifiers::NONE,
        text: Some(text.to_string()),
    };
}

/// Build a `Key` event with explicit modifiers and no composed text.
fn key_mods(code: KeyCode, modifiers: Modifiers) -> InputEvent {
    return InputEvent::Key {
        code,
        modifiers,
        text: None,
    };
}

#[test]
fn composed_text_is_written_verbatim() {
    assert_eq!(
        input_event_to_bytes(&key_text(KeyCode::Char("a".to_string()), "a")),
        b"a"
    );
    // Composed Unicode (euro sign via AltGr) survives as UTF-8.
    assert_eq!(
        input_event_to_bytes(&key_text(KeyCode::Char("\u{20ac}".to_string()), "\u{20ac}")),
        "\u{20ac}".as_bytes()
    );
}

#[test]
fn empty_composed_text_falls_through_to_keycode() {
    // A Key that carries an empty `text` must not short-circuit on it; the
    // keycode table still resolves the sequence.
    assert_eq!(
        input_event_to_bytes(&InputEvent::Key {
            code: KeyCode::Enter,
            modifiers: Modifiers::NONE,
            text: Some(String::new()),
        }),
        b"\r"
    );
}

#[test]
fn named_keys_map_to_sequences() {
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Enter, Modifiers::NONE)),
        b"\r"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Tab, Modifiers::NONE)),
        b"\t"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Backspace, Modifiers::NONE)),
        &[0x7f]
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Escape, Modifiers::NONE)),
        &[0x1b]
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Up, Modifiers::NONE)),
        b"\x1b[A"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Down, Modifiers::NONE)),
        b"\x1b[B"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Right, Modifiers::NONE)),
        b"\x1b[C"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Left, Modifiers::NONE)),
        b"\x1b[D"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Home, Modifiers::NONE)),
        b"\x1b[H"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::End, Modifiers::NONE)),
        b"\x1b[F"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Delete, Modifiers::NONE)),
        b"\x1b[3~"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Insert, Modifiers::NONE)),
        b"\x1b[2~"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::PageUp, Modifiers::NONE)),
        b"\x1b[5~"
    );
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::PageDown, Modifiers::NONE)),
        b"\x1b[6~"
    );
}

#[test]
fn function_keys_use_xterm_codes() {
    // The full F1..F12 table, so every arm of `function_key_bytes` is exercised.
    let expected: [(u8, &[u8]); 12] = [
        (1, b"\x1bOP"),
        (2, b"\x1bOQ"),
        (3, b"\x1bOR"),
        (4, b"\x1bOS"),
        (5, b"\x1b[15~"),
        (6, b"\x1b[17~"),
        (7, b"\x1b[18~"),
        (8, b"\x1b[19~"),
        (9, b"\x1b[20~"),
        (10, b"\x1b[21~"),
        (11, b"\x1b[23~"),
        (12, b"\x1b[24~"),
    ];
    for (n, bytes) in expected {
        assert_eq!(
            input_event_to_bytes(&key_mods(KeyCode::F(n), Modifiers::NONE)),
            bytes,
            "F{n}"
        );
    }
    // Out-of-range function keys produce no bytes.
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::F(20), Modifiers::NONE)),
        b""
    );
}

#[test]
fn unknown_keycode_produces_no_bytes() {
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Unknown, Modifiers::NONE)),
        b""
    );
}

#[test]
fn ctrl_letter_becomes_control_byte() {
    // Ctrl+C -> 0x03 (letter carried in Char, control byte derived).
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Char("c".to_string()), Modifiers::CTRL)),
        &[0x03]
    );
    // Case-insensitive.
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Char("C".to_string()), Modifiers::CTRL)),
        &[0x03]
    );
    // Ctrl with a non-control char falls back to the raw bytes.
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Char("1".to_string()), Modifiers::CTRL)),
        b"1"
    );
    // Ctrl with a multi-character grapheme is not a control key; the raw bytes
    // pass through unchanged.
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Char("ab".to_string()), Modifiers::CTRL)),
        b"ab"
    );
}

#[test]
fn alt_prefixes_escape() {
    assert_eq!(
        input_event_to_bytes(&key_mods(KeyCode::Char("x".to_string()), Modifiers::ALT)),
        b"\x1bx"
    );
    // Alt+Ctrl+C -> ESC then the control byte.
    assert_eq!(
        input_event_to_bytes(&key_mods(
            KeyCode::Char("c".to_string()),
            Modifiers::ALT | Modifiers::CTRL
        )),
        &[0x1b, 0x03]
    );
}

#[test]
fn raw_passthrough_and_empty_cases() {
    assert_eq!(
        input_event_to_bytes(&InputEvent::Raw {
            bytes: vec![1, 2, 3]
        }),
        &[1, 2, 3]
    );
    assert_eq!(input_event_to_bytes(&InputEvent::Unknown), b"");
    // A modifier pressed on its own produces nothing.
    assert_eq!(
        input_event_to_bytes(&key_mods(
            KeyCode::Modifier(cssh_rs_protocol::v1::keycode::ModifierKeyCode::LeftShift),
            Modifiers::NONE
        )),
        b""
    );
}

/// Build a key-down `KEY_EVENT_RECORD` with the given char and virtual key.
fn key_record(unicode: u16, vk: u16) -> KEY_EVENT_RECORD {
    return KEY_EVENT_RECORD {
        bKeyDown: true.into(),
        wRepeatCount: 1,
        wVirtualKeyCode: vk,
        wVirtualScanCode: 0,
        uChar: KEY_EVENT_RECORD_0 {
            UnicodeChar: unicode,
        },
        dwControlKeyState: 0,
    };
}

#[test]
fn local_char_key_uses_composed_unicode() {
    // A printable char is written verbatim regardless of virtual key.
    let record = key_record(u16::from(b'a'), VK_C.0);
    assert_eq!(key_event_record_to_bytes(&record), b"a");
    // A local Ctrl+C arrives with the composed control byte 0x03.
    let record = key_record(0x03, VK_C.0);
    assert_eq!(key_event_record_to_bytes(&record), &[0x03]);
}

#[test]
fn local_navigation_key_uses_virtual_key_table() {
    // Every no-character virtual key the table maps, so both the VK lookup and
    // the named-key arms of the shared encoder are exercised end to end.
    let expected: [(VIRTUAL_KEY, &[u8]); 22] = [
        (VK_UP, b"\x1b[A"),
        (VK_DOWN, b"\x1b[B"),
        (VK_RIGHT, b"\x1b[C"),
        (VK_LEFT, b"\x1b[D"),
        (VK_HOME, b"\x1b[H"),
        (VK_END, b"\x1b[F"),
        (VK_PRIOR, b"\x1b[5~"),
        (VK_NEXT, b"\x1b[6~"),
        (VK_INSERT, b"\x1b[2~"),
        (VK_DELETE, b"\x1b[3~"),
        (VK_F1, b"\x1bOP"),
        (VK_F2, b"\x1bOQ"),
        (VK_F3, b"\x1bOR"),
        (VK_F4, b"\x1bOS"),
        (VK_F5, b"\x1b[15~"),
        (VK_F6, b"\x1b[17~"),
        (VK_F7, b"\x1b[18~"),
        (VK_F8, b"\x1b[19~"),
        (VK_F9, b"\x1b[20~"),
        (VK_F10, b"\x1b[21~"),
        (VK_F11, b"\x1b[23~"),
        (VK_F12, b"\x1b[24~"),
    ];
    for (vk, bytes) in expected {
        let record = key_record(0, vk.0);
        assert_eq!(key_event_record_to_bytes(&record), bytes, "vk {:#x}", vk.0);
    }
    // An unmapped no-character key produces no bytes.
    let unmapped = key_record(0, 0);
    assert_eq!(key_event_record_to_bytes(&unmapped), b"");
}

#[test]
fn local_lone_surrogate_falls_through_to_virtual_key() {
    // A non-zero `UnicodeChar` that is not a scalar value (a lone UTF-16
    // surrogate) is not written verbatim; the virtual key still resolves.
    let record = key_record(0xD800, VK_UP.0);
    assert_eq!(key_event_record_to_bytes(&record), b"\x1b[A");
    // ...and with no mappable virtual key it produces nothing.
    let record = key_record(0xD800, 0);
    assert_eq!(key_event_record_to_bytes(&record), b"");
}
