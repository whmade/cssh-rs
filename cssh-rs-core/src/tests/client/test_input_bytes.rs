//! Unit tests for the keystroke encoding.

use cssh_rs_protocol::v1::input::InputEvent;
use cssh_rs_protocol::v1::keycode::{KeyCode, ModifierKeyCode, Modifiers};
use windows::Win32::System::Console::{
    KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED,
    SHIFT_PRESSED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_A, VK_BACK, VK_C, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F10, VK_F11,
    VK_F12, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LEFT,
    VK_MENU, VK_NEXT, VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD3, VK_NUMPAD6, VK_NUMPAD8,
    VK_PACKET, VK_PRIOR, VK_Q, VK_RETURN, VK_RIGHT, VK_TAB, VK_UP, VK_X,
};

use crate::client::input_bytes::{
    input_event_to_bytes, key_event_record_to_bytes, keystroke_count, KeystrokeDecoder,
};
use crate::utils::windows::CTRL_BREAK_KEY_DOWN;

pub(crate) const GRINNING_FACE: &str = "\u{1f600}";
pub(crate) const GRINNING_FACE_HIGH_SURROGATE: u16 = 0xd83d;
pub(crate) const GRINNING_FACE_LOW_SURROGATE: u16 = 0xde00;

pub(crate) const CTRL_BREAK_WIN32_INPUT_MODE: &[u8] = b"\x1b[3;70;3;1;264;1_\x1b[3;70;3;0;264;1_";

pub(crate) fn key_event_record(
    unicode_char: u16,
    virtual_key: VIRTUAL_KEY,
    repeat_count: u16,
) -> KEY_EVENT_RECORD {
    return KEY_EVENT_RECORD {
        bKeyDown: true.into(),
        wRepeatCount: repeat_count,
        wVirtualKeyCode: virtual_key.0,
        wVirtualScanCode: 0,
        uChar: KEY_EVENT_RECORD_0 {
            UnicodeChar: unicode_char,
        },
        dwControlKeyState: 0,
    };
}

pub(crate) fn released(record: KEY_EVENT_RECORD) -> KEY_EVENT_RECORD {
    return KEY_EVENT_RECORD {
        bKeyDown: false.into(),
        ..record
    };
}

/// Return the Alt code that types `digits` on the numeric keypad and releases Alt with `unit`.
fn alt_code(digits: &[VIRTUAL_KEY], unit: u16) -> Vec<KEY_EVENT_RECORD> {
    let alt = KEY_EVENT_RECORD {
        dwControlKeyState: LEFT_ALT_PRESSED,
        ..key_event_record(0, VK_MENU, 1)
    };
    let mut records = vec![alt];
    for &digit in digits {
        let digit = KEY_EVENT_RECORD {
            dwControlKeyState: LEFT_ALT_PRESSED,
            ..key_event_record(0, digit, 1)
        };
        records.extend([digit, released(digit)]);
    }
    records.push(released(KEY_EVENT_RECORD {
        uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
        dwControlKeyState: 0,
        ..alt
    }));
    return records;
}

/// Return the Alt code conhost pastes a keyless `unit` as, typing `?` and releasing Alt with `unit`:
/// <https://github.com/microsoft/terminal/blob/v1.25.3131.0/src/interactivity/base/EventSynthesis.cpp#L125-L156>.
pub(crate) fn pasted_alt_code(unit: u16) -> Vec<KEY_EVENT_RECORD> {
    return alt_code(&[VK_NUMPAD6, VK_NUMPAD3], unit);
}

fn key_event_records_to_bytes(records: &[KEY_EVENT_RECORD]) -> Vec<u8> {
    let mut decoder = KeystrokeDecoder::default();
    return records
        .iter()
        .flat_map(|record| return key_event_record_to_bytes(record, &mut decoder))
        .collect();
}

fn key(code: KeyCode, modifiers: Modifiers, text: Option<&str>) -> InputEvent {
    return InputEvent::Key {
        code,
        modifiers,
        text: text.map(str::to_string),
    };
}

fn character(text: &str) -> KeyCode {
    return KeyCode::Char(text.to_string());
}

#[test]
fn test_input_event_to_bytes() {
    let cases: [(InputEvent, &[u8]); 17] = [
        (key(character("a"), Modifiers::NONE, Some("a")), b"a"),
        (
            key(character("\u{20ac}"), Modifiers::NONE, Some("\u{20ac}")),
            "\u{20ac}".as_bytes(),
        ),
        (key(KeyCode::Enter, Modifiers::NONE, Some("")), b"\r"),
        (key(KeyCode::Tab, Modifiers::NONE, None), b"\t"),
        (key(KeyCode::Backspace, Modifiers::NONE, None), b"\x7f"),
        (key(KeyCode::Escape, Modifiers::NONE, None), b"\x1b"),
        (key(KeyCode::F(20), Modifiers::NONE, None), b""),
        (key(KeyCode::Unknown, Modifiers::NONE, None), b""),
        (
            key(
                KeyCode::Modifier(ModifierKeyCode::LeftShift),
                Modifiers::NONE,
                None,
            ),
            b"",
        ),
        (key(character("c"), Modifiers::CTRL, None), b"\x03"),
        (key(character("C"), Modifiers::CTRL, Some("C")), b"\x03"),
        (key(character("1"), Modifiers::CTRL, None), b"1"),
        (key(character("ab"), Modifiers::CTRL, None), b"ab"),
        (key(character("x"), Modifiers::ALT, None), b"\x1bx"),
        (
            key(character("c"), Modifiers::ALT | Modifiers::CTRL, None),
            b"\x1b\x03",
        ),
        (InputEvent::Raw { bytes: vec![1, 2] }, b"\x01\x02"),
        (InputEvent::Unknown, b""),
    ];
    for (event, expected) in cases {
        assert_eq!(input_event_to_bytes(&event), expected, "{event:?}");
    }
}

#[test]
fn test_input_event_to_bytes_sends_modified_navigation_and_function_keys_as_xterm_does() {
    let cases: [(KeyCode, Modifiers, &[u8]); 31] = [
        (KeyCode::Left, Modifiers::CTRL, b"\x1b[1;5D"),
        (KeyCode::Right, Modifiers::CTRL, b"\x1b[1;5C"),
        (KeyCode::Up, Modifiers::SHIFT, b"\x1b[1;2A"),
        (
            KeyCode::Down,
            Modifiers::CTRL | Modifiers::SHIFT,
            b"\x1b[1;6B",
        ),
        (KeyCode::Home, Modifiers::SHIFT, b"\x1b[1;2H"),
        (KeyCode::End, Modifiers::CTRL, b"\x1b[1;5F"),
        (KeyCode::Insert, Modifiers::SHIFT, b"\x1b[2;2~"),
        (KeyCode::Delete, Modifiers::CTRL, b"\x1b[3;5~"),
        (KeyCode::PageUp, Modifiers::CTRL, b"\x1b[5;5~"),
        (KeyCode::PageDown, Modifiers::SHIFT, b"\x1b[6;2~"),
        (KeyCode::F(1), Modifiers::SHIFT, b"\x1b[1;2P"),
        (KeyCode::F(2), Modifiers::CTRL, b"\x1b[1;5Q"),
        (KeyCode::F(3), Modifiers::SHIFT, b"\x1b[1;2R"),
        (
            KeyCode::F(4),
            Modifiers::CTRL | Modifiers::SHIFT,
            b"\x1b[1;6S",
        ),
        (KeyCode::F(5), Modifiers::SHIFT, b"\x1b[15;2~"),
        (KeyCode::F(6), Modifiers::CTRL, b"\x1b[17;5~"),
        (KeyCode::F(7), Modifiers::SHIFT, b"\x1b[18;2~"),
        (KeyCode::F(8), Modifiers::CTRL, b"\x1b[19;5~"),
        (KeyCode::F(9), Modifiers::SHIFT, b"\x1b[20;2~"),
        (KeyCode::F(10), Modifiers::CTRL, b"\x1b[21;5~"),
        (KeyCode::F(11), Modifiers::SHIFT, b"\x1b[23;2~"),
        (
            KeyCode::F(12),
            Modifiers::CTRL | Modifiers::SHIFT,
            b"\x1b[24;6~",
        ),
        (KeyCode::Left, Modifiers::ALT, b"\x1b[1;3D"),
        (KeyCode::Up, Modifiers::ALT | Modifiers::SHIFT, b"\x1b[1;4A"),
        (
            KeyCode::Home,
            Modifiers::ALT | Modifiers::CTRL,
            b"\x1b[1;7H",
        ),
        (KeyCode::Delete, Modifiers::ALT, b"\x1b[3;3~"),
        (KeyCode::F(1), Modifiers::ALT, b"\x1b[1;3P"),
        (
            KeyCode::F(5),
            Modifiers::ALT | Modifiers::CTRL | Modifiers::SHIFT,
            b"\x1b[15;8~",
        ),
        (KeyCode::Left, Modifiers::META, b"\x1b[D"),
        (KeyCode::F(1), Modifiers::META, b"\x1bOP"),
        (KeyCode::F(5), Modifiers::META, b"\x1b[15~"),
    ];
    for (code, modifiers, expected) in cases {
        let event = key(code, modifiers, None);
        assert_eq!(input_event_to_bytes(&event), expected, "{event:?}");
    }
}

#[test]
fn test_input_event_to_bytes_sends_modified_editing_keys_as_windows_terminal_does() {
    let cases: [(KeyCode, Modifiers, &[u8]); 14] = [
        (KeyCode::Backspace, Modifiers::CTRL, b"\x08"),
        (
            KeyCode::Backspace,
            Modifiers::CTRL | Modifiers::SHIFT,
            b"\x08",
        ),
        (KeyCode::Backspace, Modifiers::SHIFT, b"\x7f"),
        (KeyCode::Backspace, Modifiers::ALT, b"\x1b\x7f"),
        (
            KeyCode::Backspace,
            Modifiers::ALT | Modifiers::CTRL,
            b"\x1b\x08",
        ),
        (KeyCode::Tab, Modifiers::SHIFT, b"\x1b[Z"),
        (KeyCode::Tab, Modifiers::CTRL | Modifiers::SHIFT, b"\x1b[Z"),
        (KeyCode::Tab, Modifiers::CTRL, b"\t"),
        (
            KeyCode::Tab,
            Modifiers::ALT | Modifiers::SHIFT,
            b"\x1b\x1b[Z",
        ),
        (KeyCode::Enter, Modifiers::CTRL, b"\n"),
        (KeyCode::Enter, Modifiers::CTRL | Modifiers::SHIFT, b"\n"),
        (KeyCode::Enter, Modifiers::SHIFT, b"\r"),
        (KeyCode::Enter, Modifiers::ALT, b"\x1b\r"),
        (KeyCode::Escape, Modifiers::ALT, b"\x1b\x1b"),
    ];
    for (code, modifiers, expected) in cases {
        let event = key(code, modifiers, None);
        assert_eq!(input_event_to_bytes(&event), expected, "{event:?}");
    }
}

#[test]
fn test_key_event_record_to_bytes() {
    let cases: [(u16, VIRTUAL_KEY, &[u8]); 30] = [
        (u16::from(b'a'), VK_C, b"a"),
        (0x03, VK_C, b"\x03"),
        (0x08, VK_BACK, b"\x7f"),
        (0x09, VK_TAB, b"\t"),
        (0x0d, VK_RETURN, b"\r"),
        (0x1b, VK_ESCAPE, b"\x1b"),
        (0xDC00, VK_UP, b"\x1b[A"),
        (0, VIRTUAL_KEY(0), b""),
        (0, VK_UP, b"\x1b[A"),
        (0, VK_DOWN, b"\x1b[B"),
        (0, VK_RIGHT, b"\x1b[C"),
        (0, VK_LEFT, b"\x1b[D"),
        (0, VK_HOME, b"\x1b[H"),
        (0, VK_END, b"\x1b[F"),
        (0, VK_PRIOR, b"\x1b[5~"),
        (0, VK_NEXT, b"\x1b[6~"),
        (0, VK_INSERT, b"\x1b[2~"),
        (0, VK_DELETE, b"\x1b[3~"),
        (0, VK_F1, b"\x1bOP"),
        (0, VK_F2, b"\x1bOQ"),
        (0, VK_F3, b"\x1bOR"),
        (0, VK_F4, b"\x1bOS"),
        (0, VK_F5, b"\x1b[15~"),
        (0, VK_F6, b"\x1b[17~"),
        (0, VK_F7, b"\x1b[18~"),
        (0, VK_F8, b"\x1b[19~"),
        (0, VK_F9, b"\x1b[20~"),
        (0, VK_F10, b"\x1b[21~"),
        (0, VK_F11, b"\x1b[23~"),
        (0, VK_F12, b"\x1b[24~"),
    ];
    for (unicode_char, virtual_key, expected) in cases {
        let record = key_event_record(unicode_char, virtual_key, 1);
        assert_eq!(
            key_event_records_to_bytes(&[record]),
            expected,
            "{unicode_char:#x} {:#x}",
            virtual_key.0
        );
    }
}

#[test]
fn test_key_event_record_to_bytes_sends_the_modifiers_of_the_record() {
    let cases: [(u16, VIRTUAL_KEY, u32, &[u8]); 12] = [
        (0, VK_LEFT, LEFT_CTRL_PRESSED, b"\x1b[1;5D"),
        (0, VK_F5, SHIFT_PRESSED, b"\x1b[15;2~"),
        (0, VK_LEFT, LEFT_ALT_PRESSED, b"\x1b[1;3D"),
        (
            0,
            VK_F5,
            LEFT_CTRL_PRESSED | LEFT_ALT_PRESSED,
            b"\x1b[15;7~",
        ),
        (u16::from(b'x'), VK_X, LEFT_ALT_PRESSED, b"\x1bx"),
        (u16::from(b'A'), VK_A, SHIFT_PRESSED, b"A"),
        (0x03, VK_C, LEFT_CTRL_PRESSED, b"\x03"),
        (
            u16::from(b'@'),
            VK_Q,
            LEFT_CTRL_PRESSED | RIGHT_ALT_PRESSED,
            b"@",
        ),
        (0x7f, VK_BACK, LEFT_CTRL_PRESSED, b"\x08"),
        (0x09, VK_TAB, SHIFT_PRESSED, b"\x1b[Z"),
        (0x0a, VK_RETURN, LEFT_CTRL_PRESSED, b"\n"),
        (0, VK_TAB, LEFT_CTRL_PRESSED, b"\t"),
    ];
    for (unicode_char, virtual_key, control_key_state, expected) in cases {
        let record = KEY_EVENT_RECORD {
            dwControlKeyState: control_key_state,
            ..key_event_record(unicode_char, virtual_key, 1)
        };
        assert_eq!(
            key_event_records_to_bytes(&[record]),
            expected,
            "{unicode_char:#x} {:#x} {control_key_state:#x}",
            virtual_key.0
        );
    }
}

#[test]
fn test_key_event_record_to_bytes_encodes_every_keystroke() {
    let cases: [(u16, VIRTUAL_KEY, u16, &[u8]); 3] = [
        (u16::from(b'a'), VK_A, 3, b"aaa"),
        (u16::from(b'a'), VK_A, 0, b"a"),
        (0, VK_UP, 2, b"\x1b[A\x1b[A"),
    ];
    for (unicode_char, virtual_key, repeat_count, expected) in cases {
        let record = key_event_record(unicode_char, virtual_key, repeat_count);
        assert_eq!(
            key_event_records_to_bytes(&[record]),
            expected,
            "{unicode_char:#x} {:#x} x{repeat_count}",
            virtual_key.0
        );
    }
}

#[test]
fn test_key_event_record_to_bytes_pairs_surrogates_split_across_key_downs() {
    let high = key_event_record(GRINNING_FACE_HIGH_SURROGATE, VK_PACKET, 1);
    let low = key_event_record(GRINNING_FACE_LOW_SURROGATE, VK_PACKET, 1);

    let bytes = key_event_records_to_bytes(&[high, released(high), low, released(low)]);

    assert_eq!(bytes, GRINNING_FACE.as_bytes());
}

#[test]
fn test_key_event_record_to_bytes_drops_a_high_surrogate_the_next_character_does_not_pair() {
    let records = [
        key_event_record(GRINNING_FACE_HIGH_SURROGATE, VK_PACKET, 1),
        key_event_record(u16::from(b'a'), VK_A, 1),
    ];

    let bytes = key_event_records_to_bytes(&records);

    assert_eq!(bytes, b"a");
}

#[test]
fn test_key_event_record_to_bytes_pairs_only_the_last_of_two_high_surrogates() {
    let records = [
        key_event_record(0xd83c, VK_PACKET, 1),
        key_event_record(GRINNING_FACE_HIGH_SURROGATE, VK_PACKET, 1),
        key_event_record(GRINNING_FACE_LOW_SURROGATE, VK_PACKET, 1),
    ];

    let bytes = key_event_records_to_bytes(&records);

    assert_eq!(bytes, GRINNING_FACE.as_bytes());
}

#[test]
fn test_key_event_record_to_bytes_pairs_surrogates_split_across_alt_codes() {
    let records = [
        pasted_alt_code(GRINNING_FACE_HIGH_SURROGATE),
        pasted_alt_code(GRINNING_FACE_LOW_SURROGATE),
    ]
    .concat();

    let bytes = key_event_records_to_bytes(&records);

    assert_eq!(bytes, GRINNING_FACE.as_bytes());
}

#[test]
fn test_key_event_record_to_bytes_encodes_the_character_of_an_alt_code() {
    let pasted = pasted_alt_code(0x20ac);
    let typed_by_hand = alt_code(&[VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD8], 0x20ac);

    for records in [pasted, typed_by_hand] {
        assert_eq!(key_event_records_to_bytes(&records), "\u{20ac}".as_bytes());
    }
}

#[test]
fn test_key_event_record_to_bytes_encodes_ctrl_break_as_its_keystroke_not_ctrl_c() {
    assert_eq!(
        key_event_records_to_bytes(&[CTRL_BREAK_KEY_DOWN, released(CTRL_BREAK_KEY_DOWN)]),
        CTRL_BREAK_WIN32_INPUT_MODE
    );
}

#[test]
fn test_keystroke_count() {
    let cases: [(u16, usize); 4] = [(0, 1), (1, 1), (2, 2), (u16::MAX, 65535)];
    for (repeat_count, expected) in cases {
        let record = key_event_record(u16::from(b'a'), VK_A, repeat_count);
        assert_eq!(keystroke_count(&record), expected, "{repeat_count}");
    }
}
