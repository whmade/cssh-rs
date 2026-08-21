//! Shared protocol size limits.

/// Default maximum CBOR frame body size, in bytes, a v1 endpoint advertises in
/// `Hello` and accepts until a smaller value is negotiated.
///
/// Peers take the minimum of the two advertised values, so the cap can change
/// between versions in either direction without a wire break. 512 bytes holds
/// any v1 control message or keystroke; larger input such as a paste is
/// chunked.
pub const DEFAULT_MAX_FRAME_LEN: u32 = 512;
