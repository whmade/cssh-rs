//! Unit tests for [`crate::v1::handshake`].

use super::*;

fn round_trip<T>(value: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf).expect("encode");
    let decoded: T = ciborium::de::from_reader(buf.as_slice()).expect("decode");
    assert_eq!(&decoded, value);
}

#[test]
fn test_hello_round_trip() {
    round_trip(&Hello {
        protocol_version: ProtocolVersion::new(1, 0),
        role: Role::Client,
        pid: 4321,
        capabilities: vec!["paste".to_string(), "activation_token".to_string()],
        max_frame_len: 2048,
    });
}

#[test]
fn test_hello_defaults_max_frame_len_when_absent() {
    // A peer that predates the field omits it; the decoder fills in the
    // default rather than failing.
    #[derive(serde_derive::Serialize)]
    struct OldHello {
        protocol_version: ProtocolVersion,
        role: Role,
        pid: u32,
        capabilities: Vec<String>,
    }
    let mut buf = Vec::new();
    ciborium::ser::into_writer(
        &OldHello {
            protocol_version: ProtocolVersion::new(1, 0),
            role: Role::Daemon,
            pid: 1,
            capabilities: vec![],
        },
        &mut buf,
    )
    .expect("encode");

    let hello: Hello = ciborium::de::from_reader(buf.as_slice()).expect("decode");
    assert_eq!(hello.max_frame_len, DEFAULT_MAX_FRAME_LEN);
}

#[test]
fn test_welcome_round_trip() {
    round_trip(&Welcome {
        client_id: 7,
        server_capabilities: vec!["paste".to_string()],
    });
}
