//! Windows-MSVC build orchestration.
//!
//! On Windows hosts, dispatches to native `cargo build`. On Linux and
//! macOS hosts, delegates to [`cargo-xwin`], which fetches the MSVC
//! CRT and Windows SDK on first use under the Microsoft Software
//! License Terms; this xtask sets `XWIN_ACCEPT_LICENSE=1` on the
//! cargo-xwin subprocess only after printing a notice.
//!
//! [`cargo-xwin`]: https://github.com/rust-cross/cargo-xwin

use anyhow::{bail, Result};

use super::super::ensure_rust_target_installed;
use super::super::system::windows_msvc::WindowsMsvcSystem;
use super::super::system::CrossBuildSystem;

pub const TRIPLE: &str = "x86_64-pc-windows-msvc";

/// Build cssh-rs for `x86_64-pc-windows-msvc`.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `release` - When true, build with `--release`; otherwise debug.
///
/// # Errors
/// Returns an error if the host is not supported, a prerequisite
/// install fails, or the build subprocess fails.
pub fn build<S: WindowsMsvcSystem>(system: &S, release: bool) -> Result<()> {
    let host = system.host_os();
    // Decide up front whether this (host, target) pair is supported
    // so we do not run a network-touching `rustup target add` against
    // a host we cannot finish the build on anyway.
    let path = match host {
        "windows" => BuildPath::Native,
        "linux" | "macos" => BuildPath::CargoXwin,
        _ => bail!("cross-build {TRIPLE} from {host} host is not supported by this xtask yet"),
    };
    ensure_rust_target_installed(system, TRIPLE)?;
    match path {
        BuildPath::Native => build_native(system, release),
        BuildPath::CargoXwin => build_via_xwin(system, host, release),
    }
}

enum BuildPath {
    Native,
    CargoXwin,
}

fn build_native<S: WindowsMsvcSystem>(system: &S, release: bool) -> Result<()> {
    log::info!(
        "Running `cargo build {}--target {TRIPLE}`",
        if release { "--release " } else { "" }
    );
    system.run_cargo_build(TRIPLE, release)
}

fn build_via_xwin<S: WindowsMsvcSystem>(system: &S, host: &str, release: bool) -> Result<()> {
    ensure_llvm_tooling_available(system, host)?;
    ensure_cargo_xwin_installed(system)?;
    log::info!(
        "Invoking cargo-xwin with XWIN_ACCEPT_LICENSE=1 to accept the \
         Microsoft Software License Terms for the MSVC CRT and Windows SDK. \
         cargo-xwin downloads them into your local cache on first use; \
         nothing is checked into this repository."
    );
    log::info!(
        "Running `cargo xwin build {}--target {TRIPLE}`",
        if release { "--release " } else { "" }
    );
    system.run_cargo_xwin_build(TRIPLE, release)
}

/// Verify that the LLVM tools cargo-xwin and embed-resource need are
/// available; do not auto-install (the package manager needs sudo /
/// admin, which the xtask must not silently assume).
fn ensure_llvm_tooling_available<S: CrossBuildSystem>(system: &S, host: &str) -> Result<()> {
    // `llvm-rc` is the load-bearing one: `embed-resource` invokes it
    // to compile the Windows .rc file. `clang` and `lld-link` are
    // exercised by cargo-xwin's compile/link stages.
    let missing: Vec<&str> = ["llvm-rc", "clang", "lld-link"]
        .into_iter()
        .filter(|tool| !system.is_executable_in_path(tool))
        .collect();
    if missing.is_empty() {
        log::info!("LLVM tooling (llvm-rc, clang, lld-link) found in PATH");
        return Ok(());
    }
    let hint = match host {
        "linux" => concat!(
            "install via your distro's package manager, for example ",
            "`sudo apt install clang llvm lld` on Debian/Ubuntu or ",
            "`sudo dnf install clang llvm lld` on Fedora",
        ),
        "macos" => concat!(
            "install via Homebrew: `brew install llvm` and ensure its ",
            "`bin/` directory is on PATH",
        ),
        _ => concat!(
            "install LLVM 14+ via your OS package manager and ensure ",
            "llvm-rc, clang, and lld-link are on PATH",
        ),
    };
    bail!(
        "cargo-xwin needs LLVM tooling that is not in PATH: {}. {hint}.",
        missing.join(", ")
    );
}

fn ensure_cargo_xwin_installed<S: WindowsMsvcSystem>(system: &S) -> Result<()> {
    let listed = system.list_cargo_subcommands()?;
    // `cargo --list` formats each external command as
    // `    <name>                 <description>` on its own line.
    let has_xwin = listed
        .lines()
        .any(|line| line.split_whitespace().next() == Some("xwin"));
    if has_xwin {
        log::info!("cargo-xwin already installed");
    } else {
        let version = system.read_cargo_xwin_version()?;
        log::info!("Installing cargo-xwin {version}");
        system.install_cargo_subcommand("cargo-xwin", &version)?;
    }
    Ok(())
}
