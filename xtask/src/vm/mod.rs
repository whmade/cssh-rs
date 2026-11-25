//! Start and stop the [dockurr/windows](https://github.com/dockur/windows) VM that xtasks reach over SSH.

pub mod system;

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::cross_build::Target;
pub use system::{SshEndpoint, VmSystem};

/// Name of both the VM container and the Docker volume holding its disk.
const VM_NAME: &str = "cssh-rs-windows-vm";

static IMAGE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "dockurr/windows:{}",
        include_str!("../../../.config/vm/dockurr-windows.version").trim()
    )
});

/// dockurr's `VERSION` value for Windows 11 Pro, which runs unactivated
/// without expiring, unlike the evaluation editions.
const WINDOWS_VERSION: &str = "11";

/// Directory in the VM for the files xtasks copy in, which `install.bat`
/// excludes from Defender scans.
///
/// `install.bat` runs only at install, so an installed VM keeps excluding
/// the old value of this until it is reinstalled.
pub const VM_ROOT: &str = r"C:\cssh-rs";

const VM_ROOT_PLACEHOLDER: &str = "@VM_ROOT@";

const INSTALL_BAT: &str = include_str!("install.bat");

const KEY_FILE: &str = "id_ed25519";

const KVM_DEVICE: &str = "/dev/kvm";

const SSH_PORT: u16 = 22;

/// Port of dockurr's web viewer, which shows the VM's screen.
const VIEWER_PORT: u16 = 8006;

const POLL_INTERVAL: Duration = Duration::from_secs(10);

const REPORT_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Upper bound for a start that installs Windows, which took 19 minutes
/// over a fast connection.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

/// Upper bound for booting an installed VM, which takes about 15 seconds;
/// the rest is headroom for Windows applying updates at boot.
const BOOT_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, PartialEq, Eq)]
enum ContainerState {
    Missing,
    Stopped,
    Running,
}

/// Start the test VM for `target` and wait until it accepts SSH.
///
/// On first use dockurr installs Windows unattended and runs `install.bat`,
/// which authorizes an SSH key this function generates. The VM's disk lives
/// in a Docker volume, so `cargo clean` keeps it and every worktree shares
/// one VM.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target whose tests the VM runs.
///
/// # Returns
/// `true` if this call launched the VM, `false` if it was already running.
///
/// # Errors
/// Returns an error if the host cannot run the VM, a prerequisite is
/// missing, or the VM does not become reachable.
pub fn start<S: VmSystem>(system: &S, target: Target) -> Result<bool> {
    require_vm_host(system, target)?;
    check_host_prerequisites(system)?;
    let triple = target.triple();
    let dir = data_dir(system)?;
    match container_state(system)? {
        ContainerState::Running => {
            log::info!("The {triple} test VM is already running");
            wait_until_ready(system, target, &connect(system, target)?)?;
            return Ok(false);
        }
        ContainerState::Stopped => {
            log::info!("Removing the stopped {VM_NAME} container");
            remove_container(system)?;
        }
        ContainerState::Missing => {}
    }

    let key = dir.join(KEY_FILE);
    let installed = system.volume_exists(VM_NAME);
    if !system.path_exists(&key) {
        if installed {
            bail!(
                "the test VM's disk (Docker volume {VM_NAME}) exists, but the SSH key \
                 it authorized, {}, is gone. Remove the disk with \
                 `docker volume rm {VM_NAME}` so the next start reinstalls Windows",
                key.display()
            );
        }
        system.create_dir_all(&dir)?;
        system.generate_ssh_key(&key)?;
    }
    let oem = dir.join("oem");
    system.create_dir_all(&oem)?;
    system.write_file(
        &oem.join("install.bat"),
        &INSTALL_BAT.replace(VM_ROOT_PLACEHOLDER, VM_ROOT),
    )?;
    let public_key = system.read_file(&dir.join(format!("{KEY_FILE}.pub")))?;
    system.write_file(&oem.join("authorized_keys"), &public_key)?;

    log::info!("Launching the {triple} test VM from {}", *IMAGE);
    system.run_docker(&run_args(&oem))?;
    let viewer = viewer_url(system)?;
    let timeout = if installed {
        log::info!("Booting Windows; watch the screen at {viewer}");
        BOOT_TIMEOUT
    } else {
        log::info!(
            "First start: dockurr downloads Windows {WINDOWS_VERSION} and installs it \
             into Docker volume {VM_NAME}, which takes 20 to 60 minutes. Watch the \
             screen at {viewer}"
        );
        INSTALL_TIMEOUT
    };
    wait_for_ssh(system, target, &connect(system, target)?, timeout)?;
    log::info!("The {triple} test VM is ready; stop it with `cargo xtask vm stop {triple}`");
    Ok(true)
}

/// Stop the test VM for `target`, keeping its disk for the next start.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target whose tests the VM runs.
///
/// # Errors
/// Returns an error if the host cannot run the VM, Docker is missing, or
/// Docker fails.
pub fn stop<S: VmSystem>(system: &S, target: Target) -> Result<()> {
    require_vm_host(system, target)?;
    require_prerequisites(&[missing_docker(system)])?;
    let triple = target.triple();
    match container_state(system)? {
        ContainerState::Missing => log::info!("The {triple} test VM is not running"),
        ContainerState::Running => {
            log::info!("Shutting down the {triple} test VM; Windows takes up to two minutes");
            system.run_docker(&docker_args(&["stop", VM_NAME]))?;
            remove_container(system)?;
        }
        ContainerState::Stopped => remove_container(system)?,
    }
    Ok(())
}

/// Return the SSH endpoint of the running test VM for `target`.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target whose tests the VM runs.
///
/// # Returns
/// The 127.0.0.1 port and private key to reach the VM over SSH.
///
/// # Errors
/// Returns an error, saying how to start the VM, if it is not running.
pub fn connect<S: VmSystem>(system: &S, target: Target) -> Result<SshEndpoint> {
    if container_state(system)? != ContainerState::Running {
        let triple = target.triple();
        bail!(
            "the {triple} test VM is not running. Start it with \
             `cargo xtask vm start {triple}`, or pass `--with-vm` to \
             `cargo xtask cross-test` to start it for one run"
        );
    }
    Ok(SshEndpoint {
        port: published_port(system, SSH_PORT)?,
        key: data_dir(system)?.join(KEY_FILE),
    })
}

/// Block until the installed VM behind `endpoint` accepts an SSH login.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target whose tests the VM runs.
/// * `endpoint` - SSH endpoint of the VM.
///
/// # Errors
/// Returns an error if the container stops or the VM does not boot within
/// the timeout.
pub fn wait_until_ready<S: VmSystem>(
    system: &S,
    target: Target,
    endpoint: &SshEndpoint,
) -> Result<()> {
    wait_for_ssh(system, target, endpoint, BOOT_TIMEOUT)
}

fn wait_for_ssh<S: VmSystem>(
    system: &S,
    target: Target,
    endpoint: &SshEndpoint,
    timeout: Duration,
) -> Result<()> {
    let started = system.now();
    let mut next_report = Duration::ZERO;
    while system.now() - started < timeout {
        if system.ssh_ready(endpoint) {
            return Ok(());
        }
        if container_state(system)? != ContainerState::Running {
            bail!(
                "the {VM_NAME} container stopped before the VM accepted SSH; \
                 see `docker logs {VM_NAME}`"
            );
        }
        let elapsed = system.now() - started;
        if elapsed >= next_report {
            if next_report.is_zero() {
                log::info!("Waiting for SSH on 127.0.0.1:{}", endpoint.port);
            } else {
                log::info!("Still waiting for SSH ({} min)", elapsed.as_secs() / 60);
            }
            next_report += REPORT_INTERVAL;
        }
        system.sleep(POLL_INTERVAL);
    }
    let triple = target.triple();
    bail!(
        "the test VM did not accept SSH within {} minutes. Check its screen at {}: \
         while Windows still installs or updates, run `cargo xtask vm start {triple}` \
         again to keep waiting. If it shows the desktop, the OpenSSH server is not \
         set up, and an `install.bat` window names the step that failed; reinstall \
         Windows with `cargo xtask vm stop {triple}` and `docker volume rm {VM_NAME}`",
        timeout.as_secs() / 60,
        viewer_url(system)?
    );
}

/// Fail, naming every missing piece, unless the host has what the test VM
/// needs: Docker with a reachable daemon, KVM and the OpenSSH client.
///
/// Nothing is installed here: each fix needs root, which the xtask must not
/// silently assume.
///
/// # Arguments
/// * `system` - Injected I/O provider.
///
/// # Errors
/// Returns an error listing each missing prerequisite with an install hint.
pub fn check_host_prerequisites<S: VmSystem>(system: &S) -> Result<()> {
    require_prerequisites(&[
        missing_docker(system),
        missing_kvm(system),
        missing_ssh_client(system),
    ])
}

fn missing_docker<S: VmSystem>(system: &S) -> Option<String> {
    if !system.is_executable_in_path("docker") {
        return Some(
            "Docker: `docker` is not in PATH. Install Docker Engine: \
             https://docs.docker.com/engine/install/"
                .to_owned(),
        );
    }
    (!system.docker_daemon_reachable()).then(|| {
        "Docker daemon: `docker info` cannot reach it. Start it \
         (`sudo systemctl start docker`) and make sure you may use it \
         (`sudo usermod -aG docker $USER`, then log in again to join the \
         `docker` group)"
            .to_owned()
    })
}

fn missing_kvm<S: VmSystem>(system: &S) -> Option<String> {
    (!system.path_exists(Path::new(KVM_DEVICE))).then(|| {
        format!(
            "KVM: {KVM_DEVICE} does not exist. Enable virtualization (VT-x or \
             AMD-V) in the firmware and load the kvm_intel or kvm_amd module"
        )
    })
}

fn missing_ssh_client<S: VmSystem>(system: &S) -> Option<String> {
    let tools: Vec<&str> = ["ssh", "scp", "ssh-keygen"]
        .into_iter()
        .filter(|tool| !system.is_executable_in_path(tool))
        .collect();
    (!tools.is_empty()).then(|| {
        format!(
            "OpenSSH client: {} not in PATH. Install it, for example \
             `sudo apt install openssh-client` on Debian/Ubuntu or \
             `sudo dnf install openssh-clients` on Fedora",
            tools.join(", ")
        )
    })
}

/// Fail, listing them, if any probe found a prerequisite missing.
fn require_prerequisites(missing: &[Option<String>]) -> Result<()> {
    let missing: Vec<&str> = missing.iter().flatten().map(String::as_str).collect();
    if missing.is_empty() {
        return Ok(());
    }
    bail!(
        "the test VM needs host prerequisites that are missing:\n  - {}",
        missing.join("\n  - ")
    );
}

/// Fail unless the host can run the test VM: dockurr needs Linux with KVM.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target whose tests the VM runs.
///
/// # Errors
/// Returns an error, saying why, on any host but Linux.
pub fn require_vm_host<S: VmSystem>(system: &S, target: Target) -> Result<()> {
    let host = system.host_os();
    let triple = target.triple();
    match host {
        "linux" => Ok(()),
        "windows" => bail!(
            "a Windows host needs no test VM for {triple}: cross-test runs the tests natively"
        ),
        _ => bail!(
            "the {triple} test VM cannot run on a {host} host: dockurr/windows needs Linux with KVM"
        ),
    }
}

fn container_state<S: VmSystem>(system: &S) -> Result<ContainerState> {
    let listing = system.list_containers()?;
    let state = listing.lines().find_map(|line| {
        let (name, state) = line.split_once('\t')?;
        (name == VM_NAME).then_some(state)
    });
    Ok(match state {
        None => ContainerState::Missing,
        Some("running") => ContainerState::Running,
        Some(_) => ContainerState::Stopped,
    })
}

/// Return the 127.0.0.1 host port Docker forwards to the container's TCP `port`.
fn published_port<S: VmSystem>(system: &S, port: u16) -> Result<u16> {
    let port = format!("{port}/tcp");
    let output = system.docker_port(VM_NAME, &port)?;
    output
        .lines()
        .find_map(|line| line.trim().rsplit_once(':')?.1.parse().ok())
        .with_context(|| format!("`docker port {VM_NAME} {port}` printed no host port: {output:?}"))
}

/// Return the directory holding the VM's SSH key and OEM folder.
///
/// It lives outside the worktree because one VM serves every worktree, and
/// outside the Docker volume so the host can read the key.
fn data_dir<S: VmSystem>(system: &S) -> Result<PathBuf> {
    let base = match system.env_var("XDG_DATA_HOME").map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => {
            let home = system
                .env_var("HOME")
                .context("HOME is not set, so the test VM's SSH key has no home")?;
            PathBuf::from(home).join(".local").join("share")
        }
    };
    Ok(base.join("cssh-rs").join("windows-vm"))
}

fn viewer_url<S: VmSystem>(system: &S) -> Result<String> {
    Ok(format!(
        "http://127.0.0.1:{}",
        published_port(system, VIEWER_PORT)?
    ))
}

fn remove_container<S: VmSystem>(system: &S) -> Result<()> {
    system.run_docker(&docker_args(&["rm", VM_NAME]))
}

fn docker_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

/// Return the `docker run` arguments that launch the VM container.
///
/// Ports publish on 127.0.0.1 only, each to a free host port, so the VM is
/// unreachable from the network and never collides with a running service.
fn run_args(oem: &Path) -> Vec<String> {
    let mut args = docker_args(&[
        "run",
        "--detach",
        "--name",
        VM_NAME,
        "--device",
        KVM_DEVICE,
        "--device",
        "/dev/net/tun",
        "--cap-add",
        "NET_ADMIN",
        "--publish",
        &format!("127.0.0.1::{SSH_PORT}"),
        "--publish",
        &format!("127.0.0.1::{VIEWER_PORT}"),
        "--env",
        &format!("VERSION={WINDOWS_VERSION}"),
        "--env",
        &format!("USERNAME={}", system::VM_USER),
        // dockurr's defaults, pinned because CONTRIBUTING.md documents them.
        "--env",
        "RAM_SIZE=4G",
        "--env",
        "CPU_CORES=2",
        "--env",
        "DISK_SIZE=64G",
        // dockurr falls back to user-mode networking when it cannot set up
        // NAT, and then forwards only the ports in USER_PORTS.
        // https://github.com/qemus/qemu#how-do-i-expose-network-ports
        "--env",
        &format!("USER_PORTS={SSH_PORT}"),
        "--volume",
        &format!("{VM_NAME}:/storage"),
        "--volume",
        &format!("{}:/oem:ro", oem.display()),
        // dockurr asks Windows to shut down on SIGTERM; Docker's default
        // 10 s would kill it mid-shutdown. 120 s is the README's value.
        // https://github.com/dockur/windows#usage-
        "--stop-timeout",
        "120",
    ]);
    args.push(IMAGE.clone());
    args
}

#[cfg(test)]
#[path = "../tests/test_vm.rs"]
mod tests;
