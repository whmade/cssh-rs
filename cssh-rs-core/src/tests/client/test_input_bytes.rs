//! Unit tests for the keystroke encoding.

use cssh_rs_protocol::v1::input::InputEvent;
use cssh_rs_protocol::v1::keycode::{KeyCode, ModifierKeyCode, Modifiers};
use windows::Win32::System::Console::{KEY_EVENT_RECORD, KEY_EVENT_RECORD_0};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_C, VK_DELETE, VK_DOWN, VK_END, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2, VK_F3,
    VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR,
    VK_RIGHT, VK_UP,
};

use crate::client::input_bytes::{input_event_to_bytes, key_event_record_to_bytes};

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
fn test_key_event_record_to_bytes() {
    let cases: [(u16, VIRTUAL_KEY, &[u8]); 26] = [
        (u16::from(b'a'), VK_C, b"a"),
        (0x03, VK_C, b"\x03"),
        (0xD800, VK_UP, b"\x1b[A"),
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
        let record = KEY_EVENT_RECORD {
            bKeyDown: true.into(),
            wRepeatCount: 1,
            wVirtualKeyCode: virtual_key.0,
            wVirtualScanCode: 0,
            uChar: KEY_EVENT_RECORD_0 {
                UnicodeChar: unicode_char,
            },
            dwControlKeyState: 0,
        };
        assert_eq!(
            key_event_record_to_bytes(&record),
            expected,
            "{unicode_char:#x} {:#x}",
            virtual_key.0
        );
    }
}
