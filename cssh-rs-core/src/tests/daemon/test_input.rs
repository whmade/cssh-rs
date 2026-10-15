//! Unit tests for the daemon keystroke conversion.

use cssh_rs_protocol::v1::input::{InputEvent, SignalKind};
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use cssh_rs_protocol::v1::message::DaemonToClient;
use windows::Win32::System::Console::{
    KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED, SHIFT_PRESSED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_A, VK_C, VK_CANCEL, VK_UP};

use crate::daemon::input::key_event_to_message;

fn key(unicode_char: u16, virtual_key: VIRTUAL_KEY, control_key_state: u32) -> KEY_EVENT_RECORD {
    return KEY_EVENT_RECORD {
        bKeyDown: true.into(),
        wRepeatCount: 1,
        wVirtualKeyCode: virtual_key.0,
        wVirtualScanCode: 0,
        uChar: KEY_EVENT_RECORD_0 {
            UnicodeChar: unicode_char,
        },
        dwControlKeyState: control_key_state,
    };
}

fn input(code: KeyCode, modifiers: Modifiers, text: Option<&str>) -> DaemonToClient {
    return DaemonToClient::Input {
        event: InputEvent::Key {
            code,
            modifiers,
            text: text.map(str::to_string),
        },
    };
}

#[test]
fn test_key_event_to_message() {
    let cases = [
        (
            key(u16::from(b'a'), VK_A, 0),
            input(KeyCode::Char("a".to_string()), Modifiers::NONE, Some("a")),
        ),
        (
            key(u16::from(b'A'), VK_A, SHIFT_PRESSED),
            input(KeyCode::Char("A".to_string()), Modifiers::SHIFT, Some("A")),
        ),
        (
            key(0x03, VK_C, LEFT_CTRL_PRESSED),
            input(
                KeyCode::Char("\u{3}".to_string()),
                Modifiers::CTRL,
                Some("\u{3}"),
            ),
        ),
        (key(0, VK_UP, 0), input(KeyCode::Up, Modifiers::NONE, None)),
        (
            key(0, VIRTUAL_KEY(0), LEFT_ALT_PRESSED),
            input(KeyCode::Unknown, Modifiers::ALT, None),
        ),
        (
            key(0, VK_CANCEL, LEFT_CTRL_PRESSED),
            DaemonToClient::Signal(SignalKind::Break),
        ),
    ];
    for (key, expected) in cases {
        assert_eq!(key_event_to_message(&key), expected);
    }
}
