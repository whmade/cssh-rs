//! Protocol version and endpoint role.

use std::fmt;
use std::str::FromStr;

use serde_derive::{Deserialize, Serialize};

/// Protocol version as a `major.minor` pair.
///
/// Serialized on the wire as the textual `"major.minor"` form (e.g. `"1.0"`),
/// matching the handshake examples and the plugin manifest. A differing major
/// version between peers is fatal; a differing minor is tolerated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProtocolVersion {
    /// Major version; a mismatch between peers is a hard error.
    pub major: u16,
    /// Minor version; a mismatch between peers is tolerated.
    pub minor: u16,
}

impl ProtocolVersion {
    /// Return a version with the given major and minor components.
    ///
    /// # Arguments
    /// * `major` - Major version component.
    /// * `minor` - Minor version component.
    ///
    /// # Returns
    /// The corresponding [`ProtocolVersion`].
    pub const fn new(major: u16, minor: u16) -> Self {
        return Self { major, minor };
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        return write!(f, "{}.{}", self.major, self.minor);
    }
}

/// Error returned when a protocol version string cannot be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseProtocolVersionError(String);

impl fmt::Display for ParseProtocolVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        return write!(f, "invalid protocol version: {}", self.0);
    }
}

impl std::error::Error for ParseProtocolVersionError {}

impl FromStr for ProtocolVersion {
    type Err = ParseProtocolVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (major, minor) = match s.split_once('.') {
            Some(parts) => parts,
            None => return Err(ParseProtocolVersionError(s.to_string())),
        };
        let major = match major.parse::<u16>() {
            Ok(value) => value,
            Err(_) => return Err(ParseProtocolVersionError(s.to_string())),
        };
        let minor = match minor.parse::<u16>() {
            Ok(value) => value,
            Err(_) => return Err(ParseProtocolVersionError(s.to_string())),
        };
        return Ok(Self { major, minor });
    }
}

impl TryFrom<String> for ProtocolVersion {
    type Error = ParseProtocolVersionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        return value.parse();
    }
}

impl From<ProtocolVersion> for String {
    fn from(value: ProtocolVersion) -> Self {
        return value.to_string();
    }
}

/// Which endpoint an [`Hello`](crate::v1::handshake::Hello) came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The coordinating daemon.
    Daemon,
    /// A per-host client.
    Client,
    /// A role not defined in this protocol minor version.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
#[path = "../tests/v1/test_version.rs"]
mod tests;
