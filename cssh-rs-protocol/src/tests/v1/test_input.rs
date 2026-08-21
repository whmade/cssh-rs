//! Unit tests for [`crate::v1::input`].

use super::*;

fn encode<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf).expect("encode");
    return buf;
}

fn round_trip<T>(value: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let bytes = encode(value);
    let decoded: T = ciborium::de::from_reader(bytes.as_slice()).expect("decode");
    assert_eq!(&decoded, value);
}

#[test]
fn test_input_event_key_round_trip_with_and_without_text() {
    round_trip(&InputEvent::Key {
        code: KeyCode::Char("e".to_string()),
        modifiers: Modifiers::NONE,
        text: None,
    });
    round_trip(&InputEvent::Key {
        code: KeyCode::Char("e".to_string()),
        modifiers: Modifiers::CTRL,
        text: Some("e".to_string()),
    });
}

#[test]
fn test_input_event_raw_encodes_bytes_as_cbor_byte_string() {
    let event = InputEvent::Raw {
        bytes: vec![0x01, 0x02, 0x03],
    };
    round_trip(&event);

    let bytes = encode(&event);
    // Adjacent tagging: {"t":"raw","c":{"bytes":<byte string>}}. The 3-byte
    // payload must be a CBOR byte string (major type 2, 0x43), not an array
    // of integers.
    assert!(
        bytes
            .windows(4)
            .any(|w| return w == [0x43, 0x01, 0x02, 0x03]),
        "raw bytes were not encoded as a CBOR byte string: {bytes:?}"
    );
}

#[test]
fn test_client_run_state_round_trip() {
    for state in [
        ClientRunState::Active,
        ClientRunState::Disabled,
        ClientRunState::Unknown,
    ] {
        round_trip(&state);
    }
}

#[test]
fn test_signal_kind_round_trip() {
    for signal in [
        SignalKind::Break,
        SignalKind::Interrupt,
        SignalKind::Unknown,
    ] {
        round_trip(&signal);
    }
}
