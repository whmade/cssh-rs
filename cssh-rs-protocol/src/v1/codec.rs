//! Length-prefixed CBOR framing for the v1 control channel.
//!
//! Each frame on the wire is:
//!
//! ```text
//! [u32 length, big-endian] [CBOR body of length bytes]
//! ```
//!
//! The length counts only the CBOR body, not the 4-byte prefix. The maximum
//! body size is a per-call parameter, not part of the wire format, so peers
//! enforce a negotiated limit (default
//! [`crate::v1::limits::DEFAULT_MAX_FRAME_LEN`]).
//!
//! Forward compatibility rides on the length prefix. `#[serde(other)]` absorbs
//! an unknown *unit* variant, but cannot fold an unknown *data-carrying*
//! message into a unit catch-all; for those, [`decode_frames`] drops the whole
//! (length-delimited) frame and counts the skip, so an unknown message never
//! crashes the stream. A length prefix over the enforced limit is the one
//! unrecoverable case: the boundary can no longer be trusted, so decoding fails
//! hard and the caller tears the connection down.

use std::io::{self, Write};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Number of bytes in a frame's big-endian `u32` length prefix.
const LENGTH_PREFIX_LEN: usize = 4;

/// Error produced while encoding or decoding length-prefixed CBOR frames.
#[derive(Debug, thiserror::Error)]
pub enum FramingError {
    /// The CBOR body could not be encoded.
    #[error("failed to encode CBOR frame body")]
    Encode(#[from] ciborium::ser::Error<std::io::Error>),
    /// A frame body exceeds the enforced limit - the declared length on decode
    /// (unrecoverable, so the caller tears the connection down), or the limit
    /// on encode.
    #[error("frame length {0} exceeds the maximum frame size")]
    FrameTooLarge(u32),
}

/// Result of decoding a buffer that may hold several frames.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedFrames<T> {
    /// Successfully decoded messages, in wire order.
    pub messages: Vec<T>,
    /// Count of complete but undecodable frames that were skipped - an
    /// unknown data-carrying message from a newer minor version, or a corrupt
    /// body. Skipping is safe because each frame is length-delimited.
    pub skipped: usize,
    /// Unconsumed trailing bytes (a partial final frame) to prepend to the
    /// next read; reuses the input buffer's allocation.
    pub remainder: Vec<u8>,
}

/// A `Write` sink that buffers at most `limit` bytes.
///
/// Lets [`encode_frame`] bound its allocation so a pathologically large
/// message fails fast instead of serializing far past the limit before the
/// size check.
struct LimitedWriter {
    buffer: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl Write for LimitedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.buffer.len() + data.len() > self.limit {
            self.overflowed = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "frame body exceeds the maximum frame size",
            ));
        }
        self.buffer.extend_from_slice(data);
        return Ok(data.len());
    }

    fn flush(&mut self) -> io::Result<()> {
        return Ok(());
    }
}

/// Encode a message into a single length-prefixed CBOR frame.
///
/// # Arguments
/// * `message` - The value to serialize into a frame.
/// * `max_frame_len` - Largest body, in bytes, to emit (the negotiated send
///   limit).
///
/// # Returns
/// The framed bytes: a big-endian `u32` body length followed by the CBOR
/// body.
///
/// # Errors
/// Returns [`FramingError::FrameTooLarge`] if the serialized body would exceed
/// `max_frame_len`, or [`FramingError::Encode`] for any other serialization
/// failure.
pub fn encode_frame<T: Serialize>(
    message: &T,
    max_frame_len: u32,
) -> Result<Vec<u8>, FramingError> {
    let mut writer = LimitedWriter {
        buffer: Vec::new(),
        limit: max_frame_len as usize,
        overflowed: false,
    };
    if let Err(error) = ciborium::ser::into_writer(message, &mut writer) {
        if writer.overflowed {
            return Err(FramingError::FrameTooLarge(max_frame_len));
        }
        return Err(FramingError::Encode(error));
    }
    let body = writer.buffer;
    let mut frame = Vec::with_capacity(LENGTH_PREFIX_LEN + body.len());
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&body);
    return Ok(frame);
}

/// Decode every complete frame in `buffer`, consuming it.
///
/// Frames whose body does not deserialize into `T` (an unknown message kind
/// or a corrupt body) are skipped and counted rather than aborting the batch,
/// since the length prefix delimits each body exactly.
///
/// # Arguments
/// * `buffer` - Bytes read from the control channel, possibly ending in a
///   partial frame. Its allocation is reused for the returned remainder.
/// * `max_frame_len` - Largest body, in bytes, to accept (this endpoint's
///   receive ceiling).
///
/// # Returns
/// A [`DecodedFrames`] holding the decoded messages, the count of skipped
/// frames, and any unconsumed trailing bytes.
///
/// # Errors
/// Returns [`FramingError::FrameTooLarge`] if a frame's declared length
/// exceeds `max_frame_len`, which leaves the stream unresynchronizable.
pub fn decode_frames<T: DeserializeOwned>(
    mut buffer: Vec<u8>,
    max_frame_len: u32,
) -> Result<DecodedFrames<T>, FramingError> {
    let mut messages = Vec::new();
    let mut skipped = 0;
    let mut pos = 0;
    while pos + LENGTH_PREFIX_LEN <= buffer.len() {
        let len = u32::from_be_bytes([
            buffer[pos],
            buffer[pos + 1],
            buffer[pos + 2],
            buffer[pos + 3],
        ]);
        if len > max_frame_len {
            return Err(FramingError::FrameTooLarge(len));
        }
        let body_start = pos + LENGTH_PREFIX_LEN;
        let body_end = body_start + len as usize;
        if body_end > buffer.len() {
            // The trailing frame is incomplete; hand it back as remainder.
            break;
        }
        match ciborium::de::from_reader(&buffer[body_start..body_end]) {
            Ok(message) => messages.push(message),
            Err(_) => skipped += 1,
        }
        pos = body_end;
    }
    buffer.drain(..pos);
    return Ok(DecodedFrames {
        messages,
        skipped,
        remainder: buffer,
    });
}

#[cfg(test)]
#[path = "../tests/v1/test_codec.rs"]
mod tests;
