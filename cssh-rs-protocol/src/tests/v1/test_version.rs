//! Unit tests for [`crate::v1::version`].

use super::*;

fn encode<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf).expect("encode");
    return buf;
}

#[test]
fn test_protocol_version_encodes_as_text() {
    let bytes = encode(&ProtocolVersion::new(1, 0));
    // CBOR text string "1.0": 0x63 (text, len 3) then the ASCII bytes.
    assert_eq!(bytes, vec![0x63, b'1', b'.', b'0']);
}

#[test]
fn test_protocol_version_round_trip() {
    let version = ProtocolVersion::new(2, 17);
    let bytes = encode(&version);
    let decoded: ProtocolVersion = ciborium::de::from_reader(bytes.as_slice()).expect("decode");
    assert_eq!(decoded, version);
}

#[test]
fn test_protocol_version_display() {
    assert_eq!(ProtocolVersion::new(1, 0).to_string(), "1.0");
}

#[test]
fn test_protocol_version_parse_rejects_malformed() {
    assert!("1".parse::<ProtocolVersion>().is_err());
    assert!("1.x".parse::<ProtocolVersion>().is_err());
    assert!("".parse::<ProtocolVersion>().is_err());
}

#[test]
fn test_role_round_trip() {
    for role in [Role::Daemon, Role::Client, Role::Unknown] {
        let bytes = encode(&role);
        let decoded: Role = ciborium::de::from_reader(bytes.as_slice()).expect("decode");
        assert_eq!(decoded, role);
    }
}

#[test]
fn test_role_unknown_string_decodes_to_unknown() {
    let bytes = encode(&"teleporter".to_string());
    let decoded: Role = ciborium::de::from_reader(bytes.as_slice()).expect("decode");
    assert_eq!(decoded, Role::Unknown);
}
