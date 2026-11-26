//! Cross-build, cross-lint and cross-doc dispatch for cssh-rs.
//!
//! Contributor-facing entry points - `cargo xtask cross-build <target>`,
//! `cross-lint <target>` and `cross-doc <target>` - for building, linting and
//! documenting cssh-rs and the other workspace packages for any supported
//! target from any supported host. The xtask owns the toolchain selection so
//! the developer experience does not drift over time.
//!
//! [`run_cross_build`], [`run_cross_lint`] and [`run_cross_doc`] orchestrate
//! the workflows. [`prepare_toolchain`] has the per-target [`strategy`] pick
//! the [`Toolchain`] that reaches the target from this host and install what
//! it needs; [`run_cargo_for`] turns a cargo subcommand into that toolchain's
//! command line. All I/O goes through the [`system`] traits.

pub mod strategy;
pub mod system;

use std::sync::LazyLock;

use anyhow::Result;
use clap::ValueEnum;
use cssh_rs_meta::PACKAGE_NAME;

use system::windows_msvc::WindowsMsvcSystem;
pub use system::{CrossBuildSystem, RealSystem};

static X86_64_PC_WINDOWS_MSVC_BINARY: LazyLock<String> =
    LazyLock::new(|| format!("{PACKAGE_NAME}.exe"));

/// Supported cross-build target triples.
///
/// v1 supports one target, `x86_64-pc-windows-msvc` (the only target
/// cssh-rs currently ships). Adding a target later is additive: extend
/// this enum, extend [`Target::triple`] / [`Target::binary_filename`],
/// add a `strategy::<target>` submodule with a `prepare` entry point,
/// and add a dispatch arm in [`prepare_toolchain`].
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
            Self::X86_64PcWindowsMsvc => X86_64_PC_WINDOWS_MSVC_BINARY.as_str(),
        }
    }
}

/// How cargo reaches a target from the current host.
///
/// Only [`prepare_toolchain`] returns one, after installing what it needs,
/// so no cargo run can skip that setup or run for another target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toolchain {
    target: Target,
    kind: ToolchainKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolchainKind {
    /// The host is the target, so plain `cargo` builds it.
    Host,
    /// Host cargo builds the target with `--target`.
    HostWithTarget,
    /// cargo-xwin builds the target from a Linux or macOS host.
    CargoXwin,
}

impl Toolchain {
    /// Return where cargo puts the target's binary for `profile`.
    fn binary_path(self, profile: &str) -> String {
        let dir = match self.kind {
            ToolchainKind::Host => format!("target/{profile}"),
            ToolchainKind::HostWithTarget | ToolchainKind::CargoXwin => {
                format!("target/{}/{profile}", self.target.triple())
            }
        };
        format!("{dir}/{}", self.target.binary_filename())
    }
}

/// Install what the host needs to build for `target`.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - The user-selected target.
///
/// # Returns
/// How cargo reaches `target` from this host.
///
/// # Errors
/// Returns an error if the (host, target) pair is unsupported or a
/// toolchain install fails.
pub fn prepare_toolchain<S: WindowsMsvcSystem>(system: &S, target: Target) -> Result<Toolchain> {
    let kind = match target {
        Target::X86_64PcWindowsMsvc => strategy::windows_msvc::prepare(system)?,
    };
    Ok(Toolchain { target, kind })
}

/// Run `cargo <subcommand> <args>` for a target through its `toolchain`.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `toolchain` - How cargo reaches the target, from [`prepare_toolchain`].
/// * `subcommand` - Cargo subcommand, such as `build`.
/// * `args` - Arguments after the subcommand.
/// * `envs` - Extra environment variables for cargo.
///
/// # Returns
/// `Ok(())` once cargo succeeds.
///
/// # Errors
/// Returns an error if cargo fails.
pub fn run_cargo_for<S: CrossBuildSystem>(
    system: &S,
    toolchain: Toolchain,
    subcommand: &str,
    args: &[String],
    envs: &[(String, String)],
) -> Result<()> {
    let mut cargo_args = Vec::new();
    let mut cargo_envs = envs.to_vec();
    if toolchain.kind == ToolchainKind::CargoXwin {
        cargo_args.push("xwin".to_owned());
        // cargo-xwin fetches the MSVC CRT and Windows SDK only once their
        // license is accepted: https://github.com/Jake-Shadle/xwin#common
        log::info!(
            "Invoking cargo-xwin with XWIN_ACCEPT_LICENSE=1 to accept the \
             Microsoft Software License Terms for the MSVC CRT and Windows SDK. \
             cargo-xwin downloads them into your local cache on first use; \
             nothing is checked into this repository."
        );
        cargo_envs.push(("XWIN_ACCEPT_LICENSE".to_owned(), "1".to_owned()));
    }
    cargo_args.push(subcommand.to_owned());
    if toolchain.kind != ToolchainKind::Host {
        cargo_args.extend(["--target".to_owned(), toolchain.target.triple().to_owned()]);
    }
    cargo_args.extend_from_slice(args);
    log::info!("Running `cargo {}`", cargo_args.join(" "));
    system.run_cargo(&cargo_args, &cargo_envs)
}

/// Cross-build cssh-rs, or the packages that `extra_args` select, for the
/// given target.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - The user-selected target.
/// * `release` - When true, build with `--release`; otherwise debug.
/// * `extra_args` - Further arguments for `cargo build`. If they select
///   packages, they replace the default `--package cssh-rs`.
///
/// # Returns
/// `Ok(())` once cargo succeeds.
///
/// # Errors
/// Returns an error if the (host, target) pair is unsupported, a
/// toolchain install fails, or the build subprocess fails.
pub fn run_cross_build<S: WindowsMsvcSystem>(
    system: &S,
    target: Target,
    release: bool,
    extra_args: &[String],
) -> Result<()> {
    let triple = target.triple();
    let profile = if release { "release" } else { "debug" };
    log::info!("Cross-building {triple} from {} host", system.host_os());
    let toolchain = prepare_toolchain(system, target)?;
    let mut args = Vec::new();
    if !extra_args.iter().any(|arg| selects_packages(arg)) {
        args.extend(["--package".to_owned(), PACKAGE_NAME.to_owned()]);
    }
    if release {
        args.push("--release".to_owned());
    }
    args.extend_from_slice(extra_args);
    let workspace_build = extra_args
        .iter()
        .any(|arg| arg == "--workspace" || arg == "--all");
    if toolchain.kind == ToolchainKind::Host && workspace_build {
        // A native workspace build can relink this running xtask.exe, and
        // Windows cannot delete a mapped file:
        // https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-deletefilew
        args.extend(["--exclude".to_owned(), env!("CARGO_PKG_NAME").to_owned()]);
    }
    run_cargo_for(system, toolchain, "build", &args, &[])?;
    // Arguments after `--` can change what cargo builds and where it puts it.
    if extra_args.is_empty() {
        log::info!("Binary built: {}", toolchain.binary_path(profile));
    }
    Ok(())
}

/// Return whether the `cargo build` argument `arg` selects packages.
fn selects_packages(arg: &str) -> bool {
    matches!(arg, "--package" | "--workspace" | "--all")
        || arg.starts_with("--package=")
        || arg.starts_with("-p")
}

/// Lint the workspace for the given target, as the `lint` alias in
/// `.cargo/config.toml` does for the host.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - The user-selected target.
///
/// # Returns
/// `Ok(())` once cargo succeeds.
///
/// # Errors
/// Returns an error if the (host, target) pair is unsupported, a
/// toolchain install fails, or clippy reports a warning.
pub fn run_cross_lint<S: WindowsMsvcSystem>(system: &S, target: Target) -> Result<()> {
    let toolchain = prepare_toolchain(system, target)?;
    let args = [
        "--workspace",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ]
    .map(str::to_owned);
    run_cargo_for(system, toolchain, "clippy", &args, &[])
}

/// Document the workspace for the given target, as the `docs` alias in
/// `.cargo/config.toml` does for the host.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - The user-selected target.
///
/// # Returns
/// `Ok(())` once cargo succeeds.
///
/// # Errors
/// Returns an error if the (host, target) pair is unsupported, a
/// toolchain install fails, or rustdoc fails.
pub fn run_cross_doc<S: WindowsMsvcSystem>(system: &S, target: Target) -> Result<()> {
    let toolchain = prepare_toolchain(system, target)?;
    let args = ["--bins", "--lib", "--no-deps"].map(str::to_owned);
    run_cargo_for(system, toolchain, "doc", &args, &[])
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
        log::info!("Rust target {triple} already installed");
    } else {
        log::info!("Installing Rust target {triple}");
        system.install_target(triple)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/test_cross_build.rs"]
mod tests;
