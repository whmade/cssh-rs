//! Toolchain preparation for the `x86_64-pc-windows-msvc` target.

use anyhow::{bail, Result};

use super::super::system::windows_msvc::WindowsMsvcSystem;
use super::super::system::CrossBuildSystem;
use super::super::{ensure_rust_target_installed, ToolchainKind};

pub const TRIPLE: &str = "x86_64-pc-windows-msvc";

/// Install what the host needs to build `x86_64-pc-windows-msvc`.
///
/// # Arguments
/// * `system` - Injected I/O provider.
///
/// # Returns
/// How cargo reaches the target from this host.
///
/// # Errors
/// Returns an error if the host is not supported or a prerequisite
/// install fails.
pub fn prepare<S: WindowsMsvcSystem>(system: &S) -> Result<ToolchainKind> {
    let host = system.host_os();
    // Decide up front whether this (host, target) pair is supported
    // so we do not run a network-touching `rustup target add` against
    // a host we cannot finish the build on anyway.
    let kind = match (host, system.host_arch()) {
        ("windows", "x86_64") if system.host_is_msvc() => ToolchainKind::Host,
        ("windows", _) => ToolchainKind::HostWithTarget,
        ("linux" | "macos", _) => ToolchainKind::CargoXwin,
        _ => bail!("building {TRIPLE} on a {host} host is not supported by this xtask yet"),
    };
    if kind != ToolchainKind::Host {
        ensure_rust_target_installed(system, TRIPLE)?;
    }
    if kind == ToolchainKind::CargoXwin {
        ensure_llvm_tooling_available(system, host)?;
        ensure_cargo_xwin_installed(system)?;
    }
    Ok(kind)
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
