//! Protocol version compatibility and capability negotiation.

use std::collections::BTreeSet;

use crate::v1::version::ProtocolVersion;

/// Error returned when two peers' protocol major versions differ.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("incompatible protocol major version: local {local}, remote {remote}")]
pub struct VersionMismatch {
    /// This endpoint's protocol version.
    pub local: ProtocolVersion,
    /// The peer's protocol version.
    pub remote: ProtocolVersion,
}

/// Accept the peer when the protocol major versions match.
///
/// # Arguments
/// * `local` - This endpoint's protocol version.
/// * `remote` - The peer's advertised protocol version.
///
/// # Returns
/// `Ok(())` when the major versions match; a differing minor is compatible.
///
/// # Errors
/// Returns [`VersionMismatch`] when the major versions differ.
pub fn check_version(
    local: ProtocolVersion,
    remote: ProtocolVersion,
) -> Result<(), VersionMismatch> {
    if local.major == remote.major {
        return Ok(());
    }
    return Err(VersionMismatch { local, remote });
}

/// Return the capabilities both peers advertise.
///
/// # Arguments
/// * `local` - Capability flags this endpoint advertises.
/// * `remote` - Capability flags the peer advertises.
///
/// # Returns
/// The intersection of the two flag sets, deduplicated and sorted ascending
/// so the result is deterministic regardless of input order.
pub fn negotiate_capabilities(local: &[String], remote: &[String]) -> Vec<String> {
    let remote_set: BTreeSet<&str> = remote.iter().map(|flag| return flag.as_str()).collect();
    let mut shared: Vec<String> = Vec::new();
    for capability in local {
        if remote_set.contains(capability.as_str()) {
            shared.push(capability.clone());
        }
    }
    shared.sort();
    shared.dedup();
    return shared;
}

/// Return the maximum frame size two peers agree on.
///
/// # Arguments
/// * `local` - This endpoint's advertised maximum receivable frame size.
/// * `remote` - The peer's advertised maximum receivable frame size.
///
/// # Returns
/// The smaller of the two - the send limit both sides honor.
pub fn negotiate_max_frame_len(local: u32, remote: u32) -> u32 {
    return local.min(remote);
}

#[cfg(test)]
#[path = "../tests/v1/test_capability.rs"]
mod tests;
