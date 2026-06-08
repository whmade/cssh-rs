//! Cross-build dispatch for cssh-rs.
//!
//! Single contributor-facing entry point for producing a release
//! binary for any supported target from any supported host:
//! `cargo xtask cross-build <target>`. The xtask owns the toolchain
//! selection so the developer experience does not drift over time.
//!
//! v1 supports one target, `x86_64-pc-windows-msvc` (the only target
//! cssh-rs currently ships). Adding a target later is a `Target`
//! variant plus a dispatch arm; no doc or interface change is needed.
//!
//! On non-Windows hosts targeting MSVC the build is delegated to
//! [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin), which
//! fetches the MSVC CRT and Windows SDK on first use under the
//! Microsoft Software License Terms. The xtask sets
//! `XWIN_ACCEPT_LICENSE=1` for that subprocess only and prints a
//! notice before invoking it; no Microsoft binaries are checked into
//! the repo.
//!
//! [`run_cross_build`] orchestrates the workflow: host detection,
//! Rust target install, helper-tool install, and the final build
//! subprocess.

use anyhow::{bail, Context, Result};
use clap::ValueEnum;

/// Supported cross-build target triples.
///
/// Each variant maps to one `--target` triple via [`Target::triple`].
/// Adding a target is additive: extend the enum, extend
/// [`Target::triple`], and extend [`strategy`] to pick a build path
/// for the new (host, target) combinations.
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

/// Build strategy chosen for a given (host, target) pair.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Strategy {
    /// Native build via `cargo build --release --target <triple>`.
    NativeCargoBuild,
    /// MSVC cross-build via `cargo xwin build --release --target <triple>`.
    CargoXwin,
}

/// Pick a build strategy for the given host OS and target.
///
/// # Arguments
/// * `host_os` - Host OS identifier as reported by [`std::env::consts::OS`]
///   (`"linux"`, `"macos"`, `"windows"`, ...).
/// * `target` - The user-selected target.
///
/// # Errors
/// Returns an error if the (host, target) pair is not yet supported.
fn strategy(host_os: &str, target: Target) -> Result<Strategy> {
    match (host_os, target) {
        ("windows", Target::X86_64PcWindowsMsvc) => Ok(Strategy::NativeCargoBuild),
        ("linux" | "macos", Target::X86_64PcWindowsMsvc) => Ok(Strategy::CargoXwin),
        (host, _) => bail!(
            "cross-build {} from {host} host is not supported by this xtask yet",
            target.triple()
        ),
    }
}

/// All side-effecting operations required by the cross-build xtask.
///
/// Implement with mocks in tests to achieve zero process, filesystem,
/// and network side-effects.
pub trait CrossBuildSystem {
    /// Return the host OS identifier (matches [`std::env::consts::OS`]).
    fn host_os(&self) -> &'static str;

    /// Run `rustup target list --installed` and return its stdout.
    ///
    /// # Errors
    /// Returns an error if the process cannot be started.
    fn list_installed_targets(&self) -> Result<String>;

    /// Run `rustup target add <triple>`.
    ///
    /// # Arguments
    /// * `triple` - Target triple to install.
    ///
    /// # Errors
    /// Returns an error if the install fails.
    fn install_target(&self, triple: &str) -> Result<()>;

    /// Run `cargo --list` and return its stdout.
    ///
    /// Used to detect whether a cargo subcommand (`cargo xwin`, ...)
    /// is installed.
    ///
    /// # Errors
    /// Returns an error if the process cannot be started.
    fn list_cargo_subcommands(&self) -> Result<String>;

    /// Install a cargo subcommand crate via `cargo install --locked <crate>`.
    ///
    /// # Arguments
    /// * `crate_name` - Crates.io package name (e.g. `cargo-xwin`).
    ///
    /// # Errors
    /// Returns an error if the install fails.
    fn install_cargo_subcommand(&self, crate_name: &str) -> Result<()>;

    /// Run `cargo build --release --target <triple>`.
    ///
    /// # Arguments
    /// * `triple` - Target triple passed to `--target`.
    ///
    /// # Errors
    /// Returns an error if the build fails.
    fn run_cargo_build(&self, triple: &str) -> Result<()>;

    /// Run `cargo xwin build --release --target <triple>` with
    /// `XWIN_ACCEPT_LICENSE=1` set on the subprocess environment.
    ///
    /// # Arguments
    /// * `triple` - Target triple passed to `--target`.
    ///
    /// # Errors
    /// Returns an error if the build fails.
    fn run_cargo_xwin_build(&self, triple: &str) -> Result<()>;

    /// Return true if `name` resolves to an executable in `PATH`.
    ///
    /// Used to preflight-check that LLVM tooling is available before
    /// invoking cargo-xwin, so the user sees an actionable message
    /// instead of an `embed-resource` panic for a missing `llvm-rc`.
    ///
    /// # Arguments
    /// * `name` - Executable name without an extension.
    fn is_executable_in_path(&self, name: &str) -> bool;

    /// Print an informational message to stdout.
    fn print_info(&self, message: &str);
}

/// Production implementation of [`CrossBuildSystem`].
pub struct RealSystem;

#[cfg_attr(coverage_nightly, coverage(off))]
impl CrossBuildSystem for RealSystem {
    fn host_os(&self) -> &'static str {
        std::env::consts::OS
    }

    fn list_installed_targets(&self) -> Result<String> {
        let output = std::process::Command::new("rustup")
            .args(["target", "list", "--installed"])
            .output()
            .context("failed to run `rustup target list --installed`")?;
        if !output.status.success() {
            bail!(
                "`rustup target list --installed` failed with status {}",
                output.status
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn install_target(&self, triple: &str) -> Result<()> {
        let status = std::process::Command::new("rustup")
            .args(["target", "add", triple])
            .status()
            .context("failed to run `rustup target add`")?;
        if !status.success() {
            bail!("`rustup target add {triple}` failed with status {status}");
        }
        Ok(())
    }

    fn list_cargo_subcommands(&self) -> Result<String> {
        let output = std::process::Command::new("cargo")
            .arg("--list")
            .output()
            .context("failed to run `cargo --list`")?;
        if !output.status.success() {
            bail!("`cargo --list` failed with status {}", output.status);
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn install_cargo_subcommand(&self, crate_name: &str) -> Result<()> {
        let status = std::process::Command::new("cargo")
            .args(["install", "--locked", crate_name])
            .status()
            .context("failed to run `cargo install`")?;
        if !status.success() {
            bail!("`cargo install --locked {crate_name}` failed with status {status}");
        }
        Ok(())
    }

    fn run_cargo_build(&self, triple: &str) -> Result<()> {
        let status = std::process::Command::new("cargo")
            .args(["build", "--release", "--target", triple])
            .status()
            .context("failed to run `cargo build`")?;
        if !status.success() {
            bail!("`cargo build --release --target {triple}` failed with status {status}");
        }
        Ok(())
    }

    fn run_cargo_xwin_build(&self, triple: &str) -> Result<()> {
        let status = std::process::Command::new("cargo")
            .args(["xwin", "build", "--release", "--target", triple])
            // xwin needs explicit acceptance of the Microsoft Software
            // License Terms before it will fetch the MSVC CRT and
            // Windows SDK. Set only on this subprocess so we do not
            // pollute the user's environment.
            // https://github.com/rust-cross/cargo-xwin#license
            .env("XWIN_ACCEPT_LICENSE", "1")
            .status()
            .context("failed to run `cargo xwin build`")?;
        if !status.success() {
            bail!("`cargo xwin build --release --target {triple}` failed with status {status}");
        }
        Ok(())
    }

    fn is_executable_in_path(&self, name: &str) -> bool {
        // Spawn the binary with no args and just check whether the
        // OS could launch it. `--version` is a safer probe than
        // `--help` because it never opens an interactive prompt.
        std::process::Command::new(name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
    }

    fn print_info(&self, message: &str) {
        println!("INFO - {message}");
    }
}

/// Cross-build cssh-rs for the given target.
///
/// Detects the host, ensures the Rust target and any required helper
/// tools (`cargo-xwin`, ...) are installed, then dispatches to the
/// appropriate build subprocess.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - The user-selected target.
///
/// # Errors
/// Returns an error if any step fails (unsupported host/target pair,
/// toolchain install failure, build failure).
pub fn run_cross_build<S: CrossBuildSystem>(system: &S, target: Target) -> Result<()> {
    let host = system.host_os();
    let triple = target.triple();
    system.print_info(&format!("Cross-building {triple} from {host} host"));

    let strategy = strategy(host, target)?;

    ensure_rust_target_installed(system, triple)?;

    match strategy {
        Strategy::NativeCargoBuild => {
            system.print_info(&format!(
                "Running `cargo build --release --target {triple}`"
            ));
            system.run_cargo_build(triple)?;
        }
        Strategy::CargoXwin => {
            ensure_llvm_tooling_available(system, host)?;
            ensure_cargo_xwin_installed(system)?;
            system.print_info(
                "Invoking cargo-xwin with XWIN_ACCEPT_LICENSE=1 to accept the \
                 Microsoft Software License Terms for the MSVC CRT and Windows \
                 SDK. cargo-xwin downloads them into your local cache on first \
                 use; nothing is checked into this repository.",
            );
            system.print_info(&format!(
                "Running `cargo xwin build --release --target {triple}`"
            ));
            system.run_cargo_xwin_build(triple)?;
        }
    }
    system.print_info(&format!(
        "Binary built: target/{triple}/release/{}",
        target.binary_filename()
    ));
    Ok(())
}

fn ensure_rust_target_installed<S: CrossBuildSystem>(system: &S, triple: &str) -> Result<()> {
    let installed = system.list_installed_targets()?;
    if installed.lines().any(|line| line.trim() == triple) {
        system.print_info(&format!("Rust target {triple} already installed"));
    } else {
        system.print_info(&format!("Installing Rust target {triple}"));
        system.install_target(triple)?;
    }
    Ok(())
}

/// Verify that the LLVM tools cargo-xwin and embed-resource need are
/// available, and fail with an actionable install hint when they
/// are not. We do not auto-install: the underlying package manager
/// requires elevated privileges (sudo, admin), which the xtask must
/// not silently assume.
fn ensure_llvm_tooling_available<S: CrossBuildSystem>(system: &S, host: &str) -> Result<()> {
    // `llvm-rc` is the load-bearing one: `embed-resource` invokes it
    // to compile the Windows .rc file. `clang` and `lld-link` are
    // exercised by cargo-xwin's compile/link stages.
    let missing: Vec<&str> = ["llvm-rc", "clang", "lld-link"]
        .into_iter()
        .filter(|tool| !system.is_executable_in_path(tool))
        .collect();
    if missing.is_empty() {
        system.print_info("LLVM tooling (llvm-rc, clang, lld-link) found in PATH");
        return Ok(());
    }
    let hint = match host {
        "linux" => {
            "install via your distro's package manager, for example \
                    `sudo apt install clang llvm lld` on Debian/Ubuntu or \
                    `sudo dnf install clang llvm lld` on Fedora"
        }
        "macos" => {
            "install via Homebrew: `brew install llvm` and ensure \
                    its `bin/` directory is on PATH"
        }
        _ => {
            "install LLVM 14+ via your OS package manager and ensure \
              llvm-rc, clang, and lld-link are on PATH"
        }
    };
    bail!(
        "cargo-xwin needs LLVM tooling that is not in PATH: {}. {hint}.",
        missing.join(", ")
    );
}

fn ensure_cargo_xwin_installed<S: CrossBuildSystem>(system: &S) -> Result<()> {
    let listed = system.list_cargo_subcommands()?;
    // `cargo --list` formats each external command as
    // `    <name>                 <description>` on its own line.
    let has_xwin = listed
        .lines()
        .any(|line| line.split_whitespace().next() == Some("xwin"));
    if has_xwin {
        system.print_info("cargo-xwin already installed");
    } else {
        system.print_info("Installing cargo-xwin");
        system.install_cargo_subcommand("cargo-xwin")?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/test_cross_build.rs"]
mod tests;
