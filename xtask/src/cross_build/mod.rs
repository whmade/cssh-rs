//! Cross-build dispatch for cssh-rs.
//!
//! Single contributor-facing entry point - `cargo xtask cross-build <target>` -
//! for producing a cssh-rs binary for any supported target from any
//! supported host. The xtask owns the toolchain selection so the
//! developer experience does not drift over time.
//!
//! [`run_cross_build`] orchestrates the workflow: host detection,
//! Rust target install, helper-tool install, and the final build
//! subprocess. Per-target details (strategy + I/O) live in the
//! [`strategy`] and [`system`] submodules.

pub mod strategy;
pub mod system;

use anyhow::Result;
use clap::ValueEnum;

pub use system::{CrossBuildSystem, RealSystem};

/// Supported cross-build target triples.
///
/// v1 supports one target, `x86_64-pc-windows-msvc` (the only target
/// cssh-rs currently ships). Adding a target later is additive: extend
/// this enum, extend [`Target::triple`] / [`Target::binary_filename`],
/// add a `strategy::<target>` submodule with a `build` entry point,
/// and add a dispatch arm in [`run_cross_build`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Target {
    /// `x86_64-pc-windows-msvc` (production Windows target).
    #[value(name = "x86_64-pc-windows-msvc")]
    X86_64PcWindowsMsvc,
}

impl Target {
    /// Return the Rust target triple this variant maps to.
    ///
    /// # Returns
    /// The canonical `--target` string passed to `cargo`.
    pub fn triple(self) -> &'static str {
        match self {
            Self::X86_64PcWindowsMsvc => "x86_64-pc-windows-msvc",
        }
    }

    /// Return the executable filename cargo emits for this target,
    /// including any platform-specific extension (e.g. `.exe`).
    pub fn binary_filename(self) -> &'static str {
        match self {
            Self::X86_64PcWindowsMsvc => "cssh-rs.exe",
        }
    }
}

/// Cross-build cssh-rs for the given target.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - The user-selected target.
/// * `release` - When true, build with `--release`; otherwise debug.
///
/// # Errors
/// Returns an error if the (host, target) pair is unsupported, a
/// toolchain install fails, or the build subprocess fails.
pub fn run_cross_build<S>(system: &S, target: Target, release: bool) -> Result<()>
where
    S: system::windows_msvc::WindowsMsvcSystem,
{
    let host = system.host_os();
    let triple = target.triple();
    let profile = if release { "release" } else { "debug" };
    system.print_info(&format!(
        "Cross-building {triple} from {host} host ({profile} profile)"
    ));

    match target {
        Target::X86_64PcWindowsMsvc => strategy::windows_msvc::build(system, release)?,
    }

    system.print_info(&format!(
        "Binary built: target/{triple}/{profile}/{}",
        target.binary_filename()
    ));
    Ok(())
}

/// Ensure the given Rust target triple is installed via rustup.
///
/// Shared by every per-target strategy; install is a single
/// `rustup target add` if the triple is not already listed.
pub(crate) fn ensure_rust_target_installed<S: CrossBuildSystem>(
    system: &S,
    triple: &str,
) -> Result<()> {
    let installed = system.list_installed_targets()?;
    if installed.lines().any(|line| line.trim() == triple) {
        system.print_info(&format!("Rust target {triple} already installed"));
    } else {
        system.print_info(&format!("Installing Rust target {triple}"));
        system.install_target(triple)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/test_cross_build.rs"]
mod tests;
