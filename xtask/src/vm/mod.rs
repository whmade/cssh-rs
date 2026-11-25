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

/// Lock file that each [`Lease`] holds shared, and that an xtask stopping
/// the VM takes exclusively.
const LOCK_FILE: &str = "vm.lock";

/// Lock file held exclusively while an xtask checks whether the VM runs
/// and launches it, so two xtasks starting at once launch it only once.
const LAUNCH_LOCK_FILE: &str = "launch.lock";

/// Marker present while the running VM was launched by a cross-test run,
/// which makes the last run using it stop it. The run writes it only once
/// the VM accepts SSH, so a VM that does not come up stays for inspection.
const AUTO_STOP_FILE: &str = "auto-stop";

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
/// one VM. A VM started here runs until `cargo xtask vm stop`, even one a
/// `cross-test --with-vm` run started earlier.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target whose tests the VM runs.
///
/// # Errors
/// Returns an error if the host cannot run the VM, a prerequisite is
/// missing, or the VM does not become reachable.
pub fn start<S: VmSystem>(system: &S, target: Target) -> Result<()> {
    require_vm_host(system, target)?;
    check_host_prerequisites(system)?;
    let _lease = acquire_lease(system)?;
    let dir = data_dir(system)?;
    let _launching = system.lock_exclusive(&dir.join(LAUNCH_LOCK_FILE))?;
    system.remove_file(&dir.join(AUTO_STOP_FILE))?;
    ensure_running(system, target, &dir)?;
    let triple = target.triple();
    log::info!("The {triple} test VM is ready; stop it with `cargo xtask vm stop {triple}`");
    Ok(())
}

/// Start the test VM for `target` for the cross-test run holding a lease
/// from [`acquire_lease`], unless it runs already.
///
/// A VM launched here that comes up stops in [`release_lease`] after the
/// last run using it; one that does not come up is left for inspection.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target whose tests the VM runs.
///
/// # Errors
/// Returns an error if the VM does not become reachable.
pub fn start_for_run<S: VmSystem>(system: &S, target: Target) -> Result<()> {
    let dir = data_dir(system)?;
    let _launching = system.lock_exclusive(&dir.join(LAUNCH_LOCK_FILE))?;
    if ensure_running(system, target, &dir)? {
        system.write_file(&dir.join(AUTO_STOP_FILE), "")?;
        log::info!(
            "The {} test VM is ready; it stops after the last cross-test using it",
            target.triple()
        );
    }
    Ok(())
}

/// Stop the test VM for `target`, keeping its disk for the next start.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target whose tests the VM runs.
/// * `force` - Stop the VM even while a cross-test run uses it.
///
/// # Errors
/// Returns an error if the host cannot run the VM, Docker is missing, a
/// cross-test run uses it and `force` is false, or Docker fails.
pub fn stop<S: VmSystem>(system: &S, target: Target, force: bool) -> Result<()> {
    require_vm_host(system, target)?;
    require_prerequisites(&[missing_docker(system)])?;
    let dir = data_dir(system)?;
    system.create_dir_all(&dir)?;
    let _lock = match system.try_lock_exclusive(&dir.join(LOCK_FILE))? {
        Some(lock) => Some(lock),
        None if force => {
            log::warn!("Stopping the test VM while a cross-test uses it; that run fails");
            None
        }
        None => bail!(
            "a cross-test or a starting `vm start` is using the {} test VM. Stop it \
             once they finish, or pass --force to stop it now",
            target.triple()
        ),
    };
    shut_down(system, target, &dir)
}

/// A claim on the test VM that a cross-test run holds while it uses the
/// VM, and `vm start` while it starts it: while any xtask holds one,
/// `vm stop` refuses and no cross-test run stops the VM.
#[must_use]
pub struct Lease<L> {
    _lock: L,
}

/// Claim the test VM, waiting while another xtask stops it.
///
/// # Arguments
/// * `system` - Injected I/O provider.
///
/// # Returns
/// The lease, to hand to [`release_lease`] when the run ends.
///
/// # Errors
/// Returns an error if the lock file cannot be created or locked.
pub fn acquire_lease<S: VmSystem>(system: &S) -> Result<Lease<S::Lock>> {
    let dir = data_dir(system)?;
    system.create_dir_all(&dir)?;
    Ok(Lease {
        _lock: system.lock_shared(&dir.join(LOCK_FILE))?,
    })
}

/// End a run's claim on the test VM, and stop the VM if a cross-test run
/// started it and no other run still holds a lease.
///
/// # Arguments
/// * `system` - Injected I/O provider.
/// * `target` - Target whose tests the VM runs.
/// * `lease` - The run's lease from [`acquire_lease`].
///
/// # Errors
/// Returns an error if stopping the VM fails.
pub fn release_lease<S: VmSystem>(system: &S, target: Target, lease: Lease<S::Lock>) -> Result<()> {
    drop(lease);
    let dir = data_dir(system)?;
    let auto_stop = dir.join(AUTO_STOP_FILE);
    if !system.path_exists(&auto_stop) {
        return Ok(());
    }
    let Some(_lock) = system.try_lock_exclusive(&dir.join(LOCK_FILE))? else {
        log::info!("Leaving the test VM to the cross-test runs still using it");
        return Ok(());
    };
    // Between our check and our lock, another run may have stopped the VM,
    // or `vm start` claimed it.
    if !system.path_exists(&auto_stop) {
        return Ok(());
    }
    shut_down(system, target, &dir)
}

/// Launch the test VM unless it runs, and wait until it accepts SSH. The
/// caller holds the launch lock; `dir` is the VM's data directory.
///
/// Returns `true` if this call launched the VM.
fn ensure_running<S: VmSystem>(system: &S, target: Target, dir: &Path) -> Result<bool> {
    let triple = target.triple();
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
    // A marker whose VM went away unstopped, as at a host reboot, would make
    // the last run stop this VM even if its launch fails.
    system.remove_file(&dir.join(AUTO_STOP_FILE))?;

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
        system.create_dir_all(dir)?;
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
    Ok(true)
}

fn shut_down<S: VmSystem>(system: &S, target: Target, dir: &Path) -> Result<()> {
    // `vm stop --force` takes no lock, so a run may launch the VM meanwhile.
    // It writes the marker once its VM is up, so removing the marker first
    // either stops that VM too or keeps its marker.
    system.remove_file(&dir.join(AUTO_STOP_FILE))?;
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
             `cargo xtask cross-test` to start it for the run"
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
