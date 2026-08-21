//! Unit tests for [`crate::v1::codec`].

use super::*;
use crate::v1::input::InputEvent;
use crate::v1::limits::DEFAULT_MAX_FRAME_LEN;
use crate::v1::message::{ClientToDaemon, DaemonToClient};

/// Frozen wire bytes for `DaemonToClient::KeepAlive`: a big-endian body
/// length of 14 followed by the CBOR map `{"t":"keep_alive"}`.
const EXPECTED_KEEP_ALIVE_FRAME: [u8; 18] = [
    0x00, 0x00, 0x00, 0x0E, // body length 14, big-endian
    0xA1, // map(1)
    0x61, 0x74, // text(1) "t"
    0x6A, 0x6B, 0x65, 0x65, 0x70, 0x5F, 0x61, 0x6C, 0x69, 0x76, 0x65, // text(10) "keep_alive"
];

#[test]
fn test_encode_keep_alive_frame_is_stable() {
    let frame = encode_frame(&DaemonToClient::KeepAlive, DEFAULT_MAX_FRAME_LEN).expect("encode");
    assert_eq!(frame, EXPECTED_KEEP_ALIVE_FRAME);
}

#[test]
fn test_decode_mixed_sequence_round_trips() {
    let messages = vec![
        DaemonToClient::KeepAlive,
        DaemonToClient::Highlight { on: true },
        DaemonToClient::Terminate {
            reason: "bye".to_string(),
        },
    ];
    let mut buf = Vec::new();
    for message in &messages {
        buf.extend_from_slice(&encode_frame(message, DEFAULT_MAX_FRAME_LEN).expect("encode"));
    }

    let decoded = decode_frames::<DaemonToClient>(buf, DEFAULT_MAX_FRAME_LEN).expect("decode");
    assert!(decoded.remainder.is_empty());
    assert_eq!(decoded.skipped, 0);
    assert_eq!(decoded.messages, messages);
}

#[test]
fn test_partial_trailing_frame_returned_as_remainder() {
    let full = encode_frame(
        &DaemonToClient::Terminate {
            reason: "later".to_string(),
        },
        DEFAULT_MAX_FRAME_LEN,
    )
    .expect("encode");
    let split_at = full.len() - 3;

    let decoded = decode_frames::<DaemonToClient>(full[..split_at].to_vec(), DEFAULT_MAX_FRAME_LEN)
        .expect("decode");
    assert!(decoded.messages.is_empty());
    assert_eq!(decoded.remainder.as_slice(), &full[..split_at]);

    // Concatenating the remainder with the rest yields the original frame and
    // parses cleanly, mirroring a client's read loop across reads.
    let mut next = decoded.remainder;
    next.extend_from_slice(&full[split_at..]);
    let decoded = decode_frames::<DaemonToClient>(next, DEFAULT_MAX_FRAME_LEN).expect("decode");
    assert!(decoded.remainder.is_empty());
    assert_eq!(decoded.messages.len(), 1);
}

#[test]
fn test_frame_too_large_is_rejected() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(DEFAULT_MAX_FRAME_LEN + 1).to_be_bytes());
    let result = decode_frames::<DaemonToClient>(buf, DEFAULT_MAX_FRAME_LEN);
    assert!(matches!(result, Err(FramingError::FrameTooLarge(_))));
}

#[test]
fn test_encode_rejects_oversized_body() {
    let huge = DaemonToClient::Input {
        event: InputEvent::Raw {
            bytes: vec![0u8; DEFAULT_MAX_FRAME_LEN as usize],
        },
    };
    let result = encode_frame(&huge, DEFAULT_MAX_FRAME_LEN);
    assert!(matches!(result, Err(FramingError::FrameTooLarge(_))));
}

#[test]
fn test_encode_accepts_larger_body_under_a_higher_limit() {
    // The same payload that overflows the default limit fits when the caller
    // raises the negotiated limit, confirming the cap is per-call.
    let payload = DaemonToClient::Input {
        event: InputEvent::Raw {
            bytes: vec![0u8; DEFAULT_MAX_FRAME_LEN as usize],
        },
    };
    let frame = encode_frame(&payload, DEFAULT_MAX_FRAME_LEN * 4).expect("encode");
    let decoded =
        decode_frames::<DaemonToClient>(frame, DEFAULT_MAX_FRAME_LEN * 4).expect("decode");
    assert_eq!(decoded.messages, vec![payload]);
}

#[test]
fn test_unknown_unit_message_decodes_to_unknown_variant() {
    // An unknown tag with no content maps to the unit `Unknown` variant via
    // `#[serde(other)]`, so it is delivered as a real message.
    #[derive(serde_derive::Serialize)]
    struct BogusUnit {
        t: &'static str,
    }
    let frame = encode_frame(&BogusUnit { t: "teleport" }, DEFAULT_MAX_FRAME_LEN).expect("encode");

    let decoded = decode_frames::<DaemonToClient>(frame, DEFAULT_MAX_FRAME_LEN).expect("decode");
    assert!(decoded.remainder.is_empty());
    assert_eq!(decoded.skipped, 0);
    assert_eq!(decoded.messages, vec![DaemonToClient::Unknown]);
}

#[derive(serde_derive::Serialize)]
struct BogusData {
    t: &'static str,
    c: u8,
}

#[test]
fn test_unknown_data_carrying_message_is_skipped_daemon_catalog() {
    // An unknown data-carrying message cannot fold into the unit `Unknown`
    // variant, so the length-delimited frame is skipped and counted; a
    // following known frame still decodes. This is the forward-compat
    // guarantee: unknown messages never crash the stream.
    let mut buf = encode_frame(
        &BogusData {
            t: "teleport",
            c: 9,
        },
        DEFAULT_MAX_FRAME_LEN,
    )
    .expect("encode");
    buf.extend_from_slice(
        &encode_frame(&DaemonToClient::KeepAlive, DEFAULT_MAX_FRAME_LEN).expect("encode"),
    );

    let decoded = decode_frames::<DaemonToClient>(buf, DEFAULT_MAX_FRAME_LEN).expect("decode");
    assert!(decoded.remainder.is_empty());
    assert_eq!(decoded.skipped, 1);
    assert_eq!(decoded.messages, vec![DaemonToClient::KeepAlive]);
}

#[test]
fn test_unknown_data_carrying_message_is_skipped_client_catalog() {
    let mut buf = encode_frame(
        &BogusData {
            t: "teleport",
            c: 9,
        },
        DEFAULT_MAX_FRAME_LEN,
    )
    .expect("encode");
    buf.extend_from_slice(
        &encode_frame(&ClientToDaemon::KeepAlive, DEFAULT_MAX_FRAME_LEN).expect("encode"),
    );

    let decoded = decode_frames::<ClientToDaemon>(buf, DEFAULT_MAX_FRAME_LEN).expect("decode");
    assert!(decoded.remainder.is_empty());
    assert_eq!(decoded.skipped, 1);
    assert_eq!(decoded.messages, vec![ClientToDaemon::KeepAlive]);
}
