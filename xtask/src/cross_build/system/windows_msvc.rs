//! Windows-MSVC cross-build I/O: the pinned cargo-xwin version.

use anyhow::{Context, Result};

use super::CrossBuildSystem;

const CARGO_XWIN_VERSION_FILE: &str = ".config/cross-build/cargo-xwin.version";

/// I/O specific to building the `x86_64-pc-windows-msvc` target.
pub trait WindowsMsvcSystem: CrossBuildSystem {
    /// Return the pinned cargo-xwin version from
    /// `.config/cross-build/cargo-xwin.version`.
    ///
    /// # Returns
    /// The version, trimmed.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    fn read_cargo_xwin_version(&self) -> Result<String>;
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl WindowsMsvcSystem for super::RealSystem {
    fn read_cargo_xwin_version(&self) -> Result<String> {
        std::fs::read_to_string(CARGO_XWIN_VERSION_FILE)
            .with_context(|| format!("failed to read {CARGO_XWIN_VERSION_FILE}"))
            .map(|s| s.trim().to_owned())
    }
}
