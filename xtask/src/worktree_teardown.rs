//! Paseo worktree teardown.
//!
//! Paseo runs `worktree.teardown` commands in the host shell -
//! PowerShell on Windows, bash on Linux/macOS - and `paseo.json` has
//! no per-platform branching. Encoding the cleanup in a Rust binary
//! lets the same `paseo.json` entry work on every platform: `git`
//! invocations and env-var reads go through identical Rust APIs
//! regardless of the host.
//!
//! The teardown does two things:
//!
//! 1. Detach `HEAD` in the worktree so the branch is not checked out
//!    when paseo removes the worktree.
//! 2. Force-delete the worktree's branch from the source checkout so
//!    a future `paseo worktree create` can reuse the name without a
//!    manual `git branch -D`.
//!
//! Both steps are best-effort: a non-zero git exit is logged but
//! does not abort teardown, mirroring the previous shell scripts
//! that used `; $global:LASTEXITCODE = 0` (PowerShell) and `|| true`
//! (bash). Aborting would leave the worktree half-removed and force
//! the contributor to clean up by hand.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// Env var paseo sets to the worktree being torn down.
const ENV_WORKTREE_PATH: &str = "PASEO_WORKTREE_PATH";

/// Env var paseo sets to the source checkout (the original repo
/// root, shared across worktrees).
const ENV_SOURCE_CHECKOUT_PATH: &str = "PASEO_SOURCE_CHECKOUT_PATH";

/// Env var paseo sets to the branch name backing the worktree.
const ENV_BRANCH_NAME: &str = "PASEO_BRANCH_NAME";

/// All side-effecting operations performed by this subcommand.
///
/// Implement with mocks in tests to achieve zero filesystem,
/// environment, or process side-effects.
pub trait WorktreeTeardownSystem {
    /// Look up an environment variable.
    ///
    /// # Arguments
    ///
    /// * `key` - Environment variable name.
    ///
    /// # Returns
    ///
    /// `Some(value)` when the variable is set and non-empty,
    /// `None` otherwise.
    fn env_var(&self, key: &str) -> Option<String>;

    /// Run `git -C <repo_path> <args...>` and return the exit code.
    ///
    /// # Arguments
    ///
    /// * `repo_path` - Repository the git command targets.
    /// * `args` - Arguments passed after `-C <repo_path>`. Owned to
    ///   keep the trait `mockall`-friendly (`&[&str]` introduces
    ///   non-`'static` lifetimes that the mock generator rejects).
    ///
    /// # Returns
    ///
    /// The process exit code, or `-1` when the process was killed by
    /// a signal.
    ///
    /// # Errors
    ///
    /// Returns an error if the `git` binary cannot be spawned (for
    /// example, when it is not on `PATH`).
    fn run_git(&self, repo_path: &Path, args: Vec<String>) -> Result<i32>;

    /// Emit an informational or warning message to the user.
    ///
    /// # Arguments
    ///
    /// * `msg` - Message to display.
    fn log(&self, msg: &str);
}

/// Production implementation of [`WorktreeTeardownSystem`].
pub struct RealSystem;

#[cfg_attr(coverage_nightly, coverage(off))]
impl WorktreeTeardownSystem for RealSystem {
    fn env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.is_empty())
    }

    fn run_git(&self, repo_path: &Path, args: Vec<String>) -> Result<i32> {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(&args)
            .status()
            .with_context(|| format!("failed to spawn `git` for {}", repo_path.display()))?;
        Ok(status.code().unwrap_or(-1))
    }

    fn log(&self, msg: &str) {
        println!("{msg}");
    }
}

/// Tear down a paseo worktree by detaching `HEAD` and deleting the
/// backing branch from the source checkout.
///
/// Both git steps are best-effort: a non-zero exit code is logged
/// but does not abort the function. A missing env var means paseo
/// did not invoke us (or invoked us with an incomplete environment);
/// we log the gap and skip the affected step rather than failing.
///
/// # Arguments
///
/// * `system` - Injected I/O provider.
///
/// # Returns
///
/// `Ok(())` once every applicable step has been attempted.
///
/// # Errors
///
/// Returns an error only if the `git` binary cannot be spawned at
/// all; per-step non-zero exits are absorbed.
pub fn worktree_teardown<S: WorktreeTeardownSystem>(system: &S) -> Result<()> {
    let worktree_path = system.env_var(ENV_WORKTREE_PATH);
    let source_path = system.env_var(ENV_SOURCE_CHECKOUT_PATH);
    let branch_name = system.env_var(ENV_BRANCH_NAME);

    if let Some(path) = worktree_path.as_deref() {
        let code = system.run_git(
            &PathBuf::from(path),
            vec!["checkout".to_owned(), "--detach".to_owned()],
        )?;
        if code != 0 {
            system.log(&format!(
                "WARN - paseo worktree teardown: `git checkout --detach` in {path} exited with code {code}; continuing."
            ));
        }
    } else {
        system.log(&format!(
            "INFO - paseo worktree teardown: {ENV_WORKTREE_PATH} not set; skipping HEAD detach."
        ));
    }

    match (source_path.as_deref(), branch_name.as_deref()) {
        (Some(source), Some(branch)) => {
            let code = system.run_git(
                &PathBuf::from(source),
                vec!["branch".to_owned(), "-D".to_owned(), branch.to_owned()],
            )?;
            if code != 0 {
                system.log(&format!(
                    "WARN - paseo worktree teardown: `git branch -D {branch}` in {source} exited with code {code}; continuing."
                ));
            }
        }
        _ => {
            system.log(&format!(
                "INFO - paseo worktree teardown: {ENV_SOURCE_CHECKOUT_PATH} or {ENV_BRANCH_NAME} not set; skipping branch delete."
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/test_worktree_teardown.rs"]
mod tests;
