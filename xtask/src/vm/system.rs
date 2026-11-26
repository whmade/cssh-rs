//! I/O abstraction for the test VM subcommands.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::cross_build::system::CrossBuildSystem;
use crate::cross_build::RealSystem;

/// Windows account dockurr creates and logs in automatically; SSH logs in
/// as this user.
pub const VM_USER: &str = "Docker";

/// SSH endpoint of a running test VM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshEndpoint {
    /// Host port on 127.0.0.1 that Docker forwards to the VM's port 22.
    pub port: u16,
    /// Private key the VM authorized when it was installed.
    pub key: PathBuf,
}

/// I/O the test VM subcommands need on top of [`CrossBuildSystem`].
pub trait VmSystem: CrossBuildSystem {
    /// Return true if `docker info` reaches the Docker daemon.
    ///
    /// # Returns
    /// `true` if `docker info` succeeds.
    fn docker_daemon_reachable(&self) -> bool;

    /// Return true if `path` exists.
    ///
    /// # Arguments
    /// * `path` - Filesystem path to probe.
    ///
    /// # Returns
    /// `true` if something exists at `path`.
    fn path_exists(&self, path: &Path) -> bool;

    /// Return an environment variable, or `None` when it is unset or empty.
    ///
    /// # Arguments
    /// * `key` - Variable name.
    ///
    /// # Returns
    /// The variable's value, or `None` if it is unset, empty or not UTF-8.
    fn env_var(&self, key: &str) -> Option<String>;

    /// Run `docker ps --all` and return one `<name>\t<state>` line per container.
    ///
    /// # Returns
    /// The command's stdout.
    ///
    /// # Errors
    /// Returns an error if the process cannot be started or exits non-zero.
    fn list_containers(&self) -> Result<String>;

    /// Return true if the Docker volume `name` exists.
    ///
    /// # Arguments
    /// * `name` - Volume name.
    ///
    /// # Returns
    /// `true` if `docker volume inspect` finds the volume.
    fn volume_exists(&self, name: &str) -> bool;

    /// Run `docker` with `args`, showing its stderr.
    ///
    /// # Arguments
    /// * `args` - Arguments after `docker`, starting with the subcommand.
    ///
    /// # Errors
    /// Returns an error if the process cannot be started or exits non-zero.
    fn run_docker(&self, args: &[String]) -> Result<()>;

    /// Run `docker port <container> <port>` and return its stdout.
    ///
    /// # Arguments
    /// * `container` - Container name.
    /// * `port` - Container port with protocol, for example `22/tcp`.
    ///
    /// # Returns
    /// The command's stdout, one `<host>:<port>` line per binding.
    ///
    /// # Errors
    /// Returns an error if the process cannot be started or exits non-zero.
    fn docker_port(&self, container: &str, port: &str) -> Result<String>;

    /// Create `path` and any missing parent directories.
    ///
    /// # Arguments
    /// * `path` - Directory to create.
    ///
    /// # Errors
    /// Returns an error if a directory cannot be created.
    fn create_dir_all(&self, path: &Path) -> Result<()>;

    /// Read a UTF-8 file.
    ///
    /// # Arguments
    /// * `path` - File to read.
    ///
    /// # Returns
    /// The file's contents.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    fn read_file(&self, path: &Path) -> Result<String>;

    /// Write `contents` to `path`, replacing the file if it exists.
    ///
    /// # Arguments
    /// * `path` - File to write.
    /// * `contents` - New file contents.
    ///
    /// # Errors
    /// Returns an error if the file cannot be written.
    fn write_file(&self, path: &Path, contents: &str) -> Result<()>;

    /// Generate a passphrase-less ed25519 key pair at `path` and `path.pub`.
    ///
    /// # Arguments
    /// * `path` - Private key path.
    ///
    /// # Errors
    /// Returns an error if `ssh-keygen` fails.
    fn generate_ssh_key(&self, path: &Path) -> Result<()>;

    /// Return true if the VM accepts an SSH login and runs a command.
    ///
    /// # Arguments
    /// * `endpoint` - SSH endpoint of the VM.
    ///
    /// # Returns
    /// `true` if `ssh` logs in and runs `exit 0` within its timeout.
    fn ssh_ready(&self, endpoint: &SshEndpoint) -> bool;

    /// Block the current thread for `duration`.
    ///
    /// # Arguments
    /// * `duration` - Time to sleep.
    fn sleep(&self, duration: Duration);

    /// Return the current time.
    ///
    /// # Returns
    /// A monotonic timestamp.
    fn now(&self) -> Instant;
}

/// Return the options every `ssh` and `scp` call to the VM shares.
///
/// The VM is a throwaway bound to 127.0.0.1 whose host key changes on every
/// reinstall, so host key checking is off and nothing lands in
/// `known_hosts`. `IdentitiesOnly` stops an agent from offering other keys
/// first and hitting the server's authentication limit.
fn ssh_options(endpoint: &SshEndpoint) -> Vec<String> {
    let mut options = vec!["-i".to_owned(), endpoint.key.to_string_lossy().into_owned()];
    for option in [
        "IdentitiesOnly=yes",
        "BatchMode=yes",
        "StrictHostKeyChecking=no",
        "UserKnownHostsFile=/dev/null",
        "LogLevel=ERROR",
    ] {
        options.extend(["-o".to_owned(), option.to_owned()]);
    }
    options
}

/// Return true if `docker` with `args` exits successfully, discarding its
/// output.
#[cfg_attr(coverage_nightly, coverage(off))]
fn docker_succeeds(args: &[&str]) -> bool {
    Command::new("docker")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Run `docker` with `args` and return its stdout, failing with its stderr
/// if it exits non-zero.
#[cfg_attr(coverage_nightly, coverage(off))]
fn docker_stdout(args: &[&str]) -> Result<String> {
    let subcommand = args.first().copied().unwrap_or_default();
    let output = Command::new("docker")
        .args(args)
        .output()
        .with_context(|| format!("failed to run `docker {subcommand}`"))?;
    if !output.status.success() {
        bail!(
            "`docker {subcommand}` failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl VmSystem for RealSystem {
    fn docker_daemon_reachable(&self) -> bool {
        docker_succeeds(&["info", "--format", "{{.ServerVersion}}"])
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|value| !value.is_empty())
    }

    fn list_containers(&self) -> Result<String> {
        docker_stdout(&["ps", "--all", "--format", "{{.Names}}\t{{.State}}"])
    }

    fn volume_exists(&self, name: &str) -> bool {
        docker_succeeds(&["volume", "inspect", name])
    }

    fn run_docker(&self, args: &[String]) -> Result<()> {
        // stdout only echoes container IDs and names; pull progress and
        // errors go to stderr.
        let status = Command::new("docker")
            .args(args)
            .stdout(Stdio::null())
            .status()
            .context("failed to run `docker`")?;
        if !status.success() {
            bail!("`docker {}` failed with status {status}", args.join(" "));
        }
        Ok(())
    }

    fn docker_port(&self, container: &str, port: &str) -> Result<String> {
        docker_stdout(&["port", container, port])
    }

    fn create_dir_all(&self, path: &Path) -> Result<()> {
        std::fs::create_dir_all(path)
            .with_context(|| format!("failed to create {}", path.display()))
    }

    fn read_file(&self, path: &Path) -> Result<String> {
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<()> {
        std::fs::write(path, contents)
            .with_context(|| format!("failed to write {}", path.display()))
    }

    fn generate_ssh_key(&self, path: &Path) -> Result<()> {
        let status = Command::new("ssh-keygen")
            .args([
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-C",
                "cssh-rs test VM",
                "-f",
            ])
            .arg(path)
            .status()
            .context("failed to run `ssh-keygen`")?;
        if !status.success() {
            bail!("`ssh-keygen` failed with status {status}");
        }
        Ok(())
    }

    fn ssh_ready(&self, endpoint: &SshEndpoint) -> bool {
        Command::new("ssh")
            .args(ssh_options(endpoint))
            .args(["-o", "ConnectTimeout=5", "-p", &endpoint.port.to_string()])
            .arg(format!("{VM_USER}@127.0.0.1"))
            .arg("exit 0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}
