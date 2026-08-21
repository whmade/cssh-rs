//! Version 1 of the cssh-rs daemon-client wire protocol.
//!
//! Messages are CBOR values carried over the control channel as
//! length-prefixed frames:
//!
//! ```text
//! [u32 length, big-endian] [CBOR body of `length` bytes]
//! ```
//!
//! The protocol is versioned `major.minor`, and this module holds every
//! `1.x` revision; a future breaking revision would be a sibling `v2` module:
//!
//! ```text
//! protocol version   module
//! 1.0, 1.1, ...  ->  v1   (this module)
//! 2.0, ...       ->  v2   (a future, breaking revision)
//! ```
//!
//! Peers reject a differing major version and tolerate a differing minor one.
//! Minor compatibility holds in both directions: a decoder skips a message it
//! does not know (an unknown enum variant falls back to an `Unknown`
//! catch-all, and a frame that still fails to decode is skipped by the
//! framing codec) and ignores unknown struct fields, while every field added
//! in a later minor version carries `#[serde(default)]` so a frame from an
//! older peer that omits it still deserializes.

pub mod codec;
pub mod handshake;
pub mod input;
pub mod keycode;
pub mod limits;
pub mod message;
pub mod version;
pub mod window;
