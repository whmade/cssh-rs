//! Windows-MSVC cross-build I/O.
//!
//! Splits off the cargo-xwin-specific subprocess calls so a future
//! macOS / Linux target trait can add its own helpers without growing
//! the generic [`super::CrossBuildSystem`] trait.

use anyhow::{bail, Context, Result};

use super::CrossBuildSystem;

const CARGO_XWIN_VERSION_FILE: &str = ".config/cross-build/cargo-xwin.version";

/// I/O specific to building the `x86_64-pc-windows-msvc` target.
pub trait WindowsMsvcSystem: CrossBuildSystem {
    /// Return the pinned cargo-xwin version from
    /// `.config/cross-build/cargo-xwin.version`.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    fn read_cargo_xwin_version(&self) -> Result<String>;

    /// Run `cargo xwin build [--release] --target <triple>` with
    /// `XWIN_ACCEPT_LICENSE=1` set on the subprocess environment.
    ///
    /// # Arguments
    /// * `triple` - Target triple passed to `--target`.
    /// * `release` - When true, append `--release`.
    ///
    /// # Errors
    /// Returns an error if the build fails.
    fn run_cargo_xwin_build(&self, triple: &str, release: bool) -> Result<()>;
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl WindowsMsvcSystem for super::RealSystem {
    fn read_cargo_xwin_version(&self) -> Result<String> {
        std::fs::read_to_string(CARGO_XWIN_VERSION_FILE)
            .with_context(|| format!("failed to read {CARGO_XWIN_VERSION_FILE}"))
            .map(|s| s.trim().to_owned())
    }

    fn run_cargo_xwin_build(&self, triple: &str, release: bool) -> Result<()> {
        let mut args = vec!["xwin", "build"];
        if release {
            args.push("--release");
        }
        args.extend(["--target", triple]);
        let status = std::process::Command::new("cargo")
            .args(&args)
            // xwin needs explicit acceptance of the Microsoft Software
            // License Terms before it will fetch the MSVC CRT and
            // Windows SDK. Scoped to this subprocess only.
            // https://github.com/rust-cross/cargo-xwin#license
            .env("XWIN_ACCEPT_LICENSE", "1")
            .status()
            .context("failed to run `cargo xwin build`")?;
        if !status.success() {
            bail!("`cargo {}` failed with status {status}", args.join(" "));
        }
        Ok(())
    }
}
