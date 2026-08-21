//! Unit tests for [`crate::v1::keycode`].

use super::*;

fn round_trip_keycode(code: &KeyCode) {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(code, &mut buf).expect("encode");
    let decoded: KeyCode = ciborium::de::from_reader(buf.as_slice()).expect("decode");
    assert_eq!(&decoded, code);
}

#[test]
fn test_keycode_round_trips_every_variant() {
    let variants = [
        KeyCode::Char("a".to_string()),
        KeyCode::Enter,
        KeyCode::Escape,
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::Delete,
        KeyCode::Insert,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::F(5),
        KeyCode::Modifier(ModifierKeyCode::RightShift),
        KeyCode::Unknown,
    ];
    for code in &variants {
        round_trip_keycode(code);
    }
}

#[test]
fn test_modifier_key_code_differentiates_left_and_right() {
    let mut left = Vec::new();
    ciborium::ser::into_writer(&ModifierKeyCode::LeftControl, &mut left).expect("encode");
    let mut right = Vec::new();
    ciborium::ser::into_writer(&ModifierKeyCode::RightControl, &mut right).expect("encode");
    assert_ne!(left, right);

    let decoded: ModifierKeyCode = ciborium::de::from_reader(left.as_slice()).expect("decode");
    assert_eq!(decoded, ModifierKeyCode::LeftControl);
}

#[test]
fn test_modifiers_encode_as_bare_integer() {
    let combined = Modifiers::CTRL | Modifiers::SHIFT;
    assert_eq!(combined.0, 0x03);

    let mut buf = Vec::new();
    ciborium::ser::into_writer(&combined, &mut buf).expect("encode");
    // A CBOR unsigned integer 3 is a single byte 0x03.
    assert_eq!(buf, vec![0x03]);
}

#[test]
fn test_modifiers_contains() {
    let combined = Modifiers::CTRL | Modifiers::ALT;
    assert!(combined.contains(Modifiers::CTRL));
    assert!(combined.contains(Modifiers::ALT));
    assert!(!combined.contains(Modifiers::SHIFT));
    assert!(combined.contains(Modifiers::NONE));
}
