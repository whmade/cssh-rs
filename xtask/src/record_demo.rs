//! Record the cssh-rs demo GIF via the `demo/` package.
//!
//! The Rust side is a thin shell: it builds the binary and installs the
//! packages, then the `demo/suites/record_demo.robot` suite drives real
//! console windows and records the desktop. Windows only.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// All side-effecting operations required by this module.
///
/// Implement with mocks in tests to achieve zero process and filesystem
/// side-effects.
pub trait DemoSystem {
    /// Return `true` when running on Windows, the demo's only supported host.
    fn is_windows(&self) -> bool;

    /// Return the absolute path to the workspace root (parent of `xtask/`).
    ///
    /// # Errors
    /// Returns an error if the workspace root cannot be resolved.
    fn workspace_root(&self) -> Result<PathBuf>;

    /// Build the debug cssh-rs binary and return its path.
    ///
    /// # Errors
    /// Returns an error if the build fails.
    fn build_binary(&self) -> Result<PathBuf>;

    /// Install a Python package directory in editable mode.
    ///
    /// # Arguments
    /// * `package_dir` - Directory holding the package's `pyproject.toml`.
    ///
    /// # Errors
    /// Returns an error if the install fails.
    fn pip_install(&self, package_dir: &Path) -> Result<()>;

    /// Run the demo recorder robot suite to produce the GIF.
    ///
    /// # Arguments
    /// * `binary` - Path to the cssh-rs executable to drive.
    /// * `output_dir` - Directory for the intermediate MP4.
    /// * `gif` - Destination path for the exported GIF.
    ///
    /// # Errors
    /// Returns an error if the recorder exits with a non-zero status.
    fn run_demo(&self, binary: &Path, output_dir: &Path, gif: &Path) -> Result<()>;

    /// Return `true` when `path` exists.
    ///
    /// # Arguments
    /// * `path` - Filesystem path to probe.
    fn path_exists(&self, path: &Path) -> bool;
}

/// Production implementation of [`DemoSystem`].
pub struct RealSystem;

#[cfg_attr(coverage_nightly, coverage(off))]
impl DemoSystem for RealSystem {
    fn is_windows(&self) -> bool {
        cfg!(windows)
    }

    fn workspace_root(&self) -> Result<PathBuf> {
        // CARGO_MANIFEST_DIR points at xtask/, whose parent is the workspace root.
        Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("failed to resolve workspace root from CARGO_MANIFEST_DIR")?
            .to_path_buf())
    }

    fn build_binary(&self) -> Result<PathBuf> {
        let root = self.workspace_root()?;
        let status = std::process::Command::new("cargo")
            .args(["build", "-p", "cssh-rs"])
            .status()
            .context("failed to spawn `cargo build -p cssh-rs`")?;
        if !status.success() {
            bail!("`cargo build -p cssh-rs` failed with status {status}");
        }
        Ok(root.join("target").join("debug").join("cssh-rs.exe"))
    }

    fn pip_install(&self, package_dir: &Path) -> Result<()> {
        let status = std::process::Command::new("python")
            .args(["-m", "pip", "install", "-e"])
            .arg(package_dir)
            .status()
            .context("failed to spawn `python -m pip install`; is Python on PATH?")?;
        if !status.success() {
            bail!(
                "`pip install -e {}` failed with status {status}",
                package_dir.display()
            );
        }
        Ok(())
    }

    fn run_demo(&self, binary: &Path, output_dir: &Path, gif: &Path) -> Result<()> {
        let suite = self
            .workspace_root()?
            .join("demo")
            .join("suites")
            .join("record_demo.robot");
        // `python -m robot` uses the interpreter that installed the packages; a
        // bare `robot` needs the Scripts dir on PATH, which is not guaranteed.
        // `--variable NAME:VALUE` splits on the first colon, so `C:\...` survives.
        let status = std::process::Command::new("python")
            .args(["-m", "robot"])
            .arg("--variable")
            .arg(format!("BINARY:{}", binary.display()))
            .arg("--variable")
            .arg(format!("OUTPUT_DIR:{}", output_dir.display()))
            .arg("--variable")
            .arg(format!("GIF:{}", gif.display()))
            .arg("--outputdir")
            .arg(output_dir)
            .arg(&suite)
            .status()
            .context("failed to spawn `python -m robot`; is Python on PATH and the demo package installed?")?;
        if !status.success() {
            bail!("demo recorder (robot) exited with status {status}");
        }
        Ok(())
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

/// Record the demo GIF into `target/demo/cssh-rs.gif`.
///
/// The GIF lands under `target/` so Cargo's `.gitignore` covers it;
/// publishing it is a separate step.
///
/// # Arguments
/// * `system` - Injected I/O provider.
///
/// # Errors
/// Returns an error on a non-Windows host, or if any build, install or record
/// step fails, or if no GIF is produced.
pub fn record_demo<S: DemoSystem>(system: &S) -> Result<()> {
    if !system.is_windows() {
        bail!("`record-demo` runs on Windows only: it drives real console windows and records the desktop");
    }
    let root = system.workspace_root()?;

    log::info!("Building cssh-rs (debug) for the demo");
    let binary = system.build_binary()?;

    for package in ["automation", "demo"] {
        let dir = root.join(package);
        log::info!("Installing {} (editable)", dir.display());
        system.pip_install(&dir)?;
    }

    let output_dir = root.join("target").join("demo");
    let gif = output_dir.join("cssh-rs.gif");
    log::info!("Recording demo -> {}", gif.display());
    system.run_demo(&binary, &output_dir, &gif)?;

    if !system.path_exists(&gif) {
        bail!(
            "demo recorder finished but no GIF was written at {}",
            gif.display()
        );
    }
    log::info!("Wrote demo GIF -> {}", gif.display());
    Ok(())
}

#[cfg(test)]
#[path = "tests/test_record_demo.rs"]
mod tests;
