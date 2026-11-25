//! I/O abstraction for the cross-test subcommand.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::cross_build::system::windows_msvc::WindowsMsvcSystem;
use crate::cross_build::RealSystem;
use crate::vm::VmSystem;

/// I/O `cross-test` needs on top of the cross-build and test VM traits.
pub trait CrossTestSystem: WindowsMsvcSystem + VmSystem {
    /// Return the path of the running xtask executable.
    ///
    /// # Returns
    /// The absolute path of the executable.
    ///
    /// # Errors
    /// Returns an error if the path cannot be determined.
    fn current_exe(&self) -> Result<PathBuf>;

    /// Return the ID of the current process.
    ///
    /// # Returns
    /// The operating system's process ID.
    fn process_id(&self) -> u32;

    /// Return the current working directory.
    ///
    /// # Returns
    /// The absolute path of the working directory.
    ///
    /// # Errors
    /// Returns an error if the directory cannot be determined.
    fn current_dir(&self) -> Result<PathBuf>;

    /// Return the absolute path to the workspace root (parent of `xtask/`).
    ///
    /// # Returns
    /// The workspace root this xtask was built from.
    ///
    /// # Errors
    /// Returns an error if the workspace root cannot be resolved.
    fn workspace_root(&self) -> Result<PathBuf>;

    /// Write a tar archive of every file under `root` that git tracks or
    /// would track (`git ls-files --cached --others --exclude-standard`).
    ///
    /// # Arguments
    /// * `root` - Workspace root; archive paths are relative to it.
    /// * `archive` - Path of the tar file to write.
    ///
    /// # Errors
    /// Returns an error if `git` or `tar` fails.
    fn pack_workspace(&self, root: &Path, archive: &Path) -> Result<()>;
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl CrossTestSystem for RealSystem {
    fn current_exe(&self) -> Result<PathBuf> {
        std::env::current_exe().context("failed to locate the xtask executable")
    }

    fn process_id(&self) -> u32 {
        std::process::id()
    }

    fn current_dir(&self) -> Result<PathBuf> {
        std::env::current_dir().context("failed to read the current directory")
    }

    fn workspace_root(&self) -> Result<PathBuf> {
        Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("failed to resolve workspace root from CARGO_MANIFEST_DIR")?
            .to_path_buf())
    }

    fn pack_workspace(&self, root: &Path, archive: &Path) -> Result<()> {
        let output = Command::new("git")
            .args([
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ])
            .current_dir(root)
            .output()
            .context("failed to run `git ls-files`")?;
        if !output.status.success() {
            bail!(
                "`git ls-files` failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim(),
            );
        }
        // Tracked files deleted from the working tree are still listed.
        let list: Vec<u8> = output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .filter(|path| root.join(String::from_utf8_lossy(path).as_ref()).is_file())
            .flat_map(|path| path.iter().copied().chain([0]))
            .collect();
        let mut tar = Command::new("tar")
            .arg("-cf")
            .arg(archive)
            .args(["--null", "-T", "-"])
            .current_dir(root)
            .stdin(Stdio::piped())
            .spawn()
            .context("failed to run `tar`")?;
        tar.stdin
            .take()
            .context("`tar` has no stdin")?
            .write_all(&list)
            .context("failed to pass the file list to `tar`")?;
        let status = tar.wait().context("failed to wait for `tar`")?;
        if !status.success() {
            bail!("`tar` failed with status {status}");
        }
        Ok(())
    }
}
