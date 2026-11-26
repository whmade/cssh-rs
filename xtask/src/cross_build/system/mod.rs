//! I/O abstraction for cross-build subcommands.
//!
//! [`CrossBuildSystem`] is the trait every cross-build target reuses
//! (host detection, target install, cargo helpers). Per-target traits
//! such as [`windows_msvc::WindowsMsvcSystem`] extend it with the I/O
//! only that target needs, so a future macOS or Linux target can add
//! its own trait + impl without growing this one.

pub mod windows_msvc;

use anyhow::{bail, Context, Result};

/// Generic operations shared by every cross-build target.
pub trait CrossBuildSystem {
    /// Return the host OS identifier (matches [`std::env::consts::OS`]).
    fn host_os(&self) -> &'static str;

    /// Return the host CPU architecture.
    ///
    /// # Returns
    /// [`std::env::consts::ARCH`], such as `x86_64` or `aarch64`.
    fn host_arch(&self) -> &'static str;

    /// Return whether the host's Rust target uses the MSVC environment.
    ///
    /// # Returns
    /// `true` for a `*-msvc` host.
    fn host_is_msvc(&self) -> bool;

    /// Run `rustup target list --installed` and return its stdout.
    ///
    /// # Errors
    /// Returns an error if the process cannot be started or exits non-zero.
    fn list_installed_targets(&self) -> Result<String>;

    /// Run `rustup target add <triple>`.
    ///
    /// # Arguments
    /// * `triple` - Target triple to install.
    ///
    /// # Errors
    /// Returns an error if the install fails.
    fn install_target(&self, triple: &str) -> Result<()>;

    /// Return true if `name` resolves to an executable in `PATH`.
    ///
    /// # Arguments
    /// * `name` - Executable name without an extension.
    fn is_executable_in_path(&self, name: &str) -> bool;

    /// Run `cargo --list` and return its stdout.
    ///
    /// # Errors
    /// Returns an error if the process cannot be started or exits non-zero.
    fn list_cargo_subcommands(&self) -> Result<String>;

    /// Install a cargo subcommand crate via
    /// `cargo install --locked --version <version> <crate>`.
    ///
    /// # Arguments
    /// * `crate_name` - Crates.io package name.
    /// * `version` - Exact crates.io version to install.
    ///
    /// # Errors
    /// Returns an error if the install fails.
    fn install_cargo_subcommand(&self, crate_name: &str, version: &str) -> Result<()>;

    /// Run `cargo <args>` with inherited stdio.
    ///
    /// # Arguments
    /// * `args` - Arguments after `cargo`, starting with the subcommand.
    /// * `envs` - Environment variables set on the cargo process only.
    ///
    /// # Returns
    /// `Ok(())` once cargo exits zero.
    ///
    /// # Errors
    /// Returns an error if cargo cannot run or exits non-zero.
    fn run_cargo(&self, args: &[String], envs: &[(String, String)]) -> Result<()>;
}

/// Production implementation of every cross-build trait.
pub struct RealSystem;

#[cfg_attr(coverage_nightly, coverage(off))]
impl CrossBuildSystem for RealSystem {
    fn host_os(&self) -> &'static str {
        std::env::consts::OS
    }

    fn host_arch(&self) -> &'static str {
        std::env::consts::ARCH
    }

    fn host_is_msvc(&self) -> bool {
        cfg!(target_env = "msvc")
    }

    fn list_installed_targets(&self) -> Result<String> {
        let output = std::process::Command::new("rustup")
            .args(["target", "list", "--installed"])
            .output()
            .context("failed to run `rustup target list --installed`")?;
        if !output.status.success() {
            bail!(
                "`rustup target list --installed` failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim(),
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn install_target(&self, triple: &str) -> Result<()> {
        let output = std::process::Command::new("rustup")
            .args(["target", "add", triple])
            .output()
            .context("failed to run `rustup target add`")?;
        if !output.status.success() {
            bail!(
                "`rustup target add {triple}` failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim(),
            );
        }
        Ok(())
    }

    fn is_executable_in_path(&self, name: &str) -> bool {
        // `--version` is a safer probe than `--help` - it never opens an
        // interactive prompt and exits quickly for the tools we check.
        std::process::Command::new(name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
    }

    fn list_cargo_subcommands(&self) -> Result<String> {
        let output = std::process::Command::new("cargo")
            .arg("--list")
            .output()
            .context("failed to run `cargo --list`")?;
        if !output.status.success() {
            bail!(
                "`cargo --list` failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim(),
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn install_cargo_subcommand(&self, crate_name: &str, version: &str) -> Result<()> {
        let output = std::process::Command::new("cargo")
            .args(["install", "--locked", "--version", version, crate_name])
            .output()
            .context("failed to run `cargo install`")?;
        if !output.status.success() {
            bail!(
                "`cargo install --locked --version {version} {crate_name}` failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim(),
            );
        }
        Ok(())
    }

    fn run_cargo(&self, args: &[String], envs: &[(String, String)]) -> Result<()> {
        let status = std::process::Command::new("cargo")
            .args(args)
            .envs(envs.iter().map(|(key, value)| (key, value)))
            .status()
            .context("failed to run `cargo`")?;
        if !status.success() {
            bail!("`cargo {}` failed with status {status}", args.join(" "));
        }
        Ok(())
    }
}
