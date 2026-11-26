//! Tests for the test VM module.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::cross_build::Target;
use crate::mock_system::{FakeDocker, FakeFiles, MockSystem, DATA_DIR, KEY, PUBLIC_KEY};
use crate::vm::{start, stop};

const TRIPLE: &str = "x86_64-pc-windows-msvc";

/// An `XDG_DATA_HOME` that `Path::is_absolute` accepts on the host running
/// the tests, which on Windows takes a drive letter.
const XDG_DATA_HOME: &str = if cfg!(windows) { r"C:\data" } else { "/data" };

#[test]
fn test_vm_start_on_windows_host_errors_without_touching_docker() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "windows");
    mock.expect_is_executable_in_path().never();
    mock.expect_list_containers().never();
    mock.expect_run_docker().never();

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("windows"),
        "error did not name the host: {err}"
    );
    assert!(
        err.contains("natively"),
        "error did not say the tests run natively: {err}"
    );
}

#[test]
fn test_vm_start_on_macos_host_errors_without_touching_docker() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "macos");
    mock.expect_is_executable_in_path().never();
    mock.expect_list_containers().never();
    mock.expect_run_docker().never();

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(err.contains("macos"), "error did not name the host: {err}");
    assert!(err.contains(TRIPLE), "error did not name the target: {err}");
    assert!(
        err.contains("KVM"),
        "error did not say KVM is needed: {err}"
    );
}

/// Build a Linux-host mock whose prerequisite probes report each tool as
/// present unless it is listed in `missing_tools`.
fn prerequisite_mock(
    missing_tools: &'static [&'static str],
    daemon_reachable: bool,
    has_kvm: bool,
) -> MockSystem {
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_is_executable_in_path()
        .returning(move |name| !missing_tools.contains(&name));
    mock.expect_docker_daemon_reachable()
        .returning(move || daemon_reachable);
    mock.expect_path_exists()
        .withf(|path| path == Path::new("/dev/kvm"))
        .returning(move |_| has_kvm);
    mock
}

/// Assert that `start` fails before touching any container and return the
/// error message.
fn start_error(mut mock: MockSystem) -> String {
    mock.expect_list_containers().never();
    mock.expect_volume_exists().never();
    mock.expect_run_docker().never();
    start(&mock, Target::X86_64PcWindowsMsvc)
        .unwrap_err()
        .to_string()
}

#[test]
fn test_vm_start_names_missing_docker() {
    // Arrange
    let mock = prerequisite_mock(&["docker"], false, true);

    // Act
    let err = start_error(mock);

    // Assert
    assert!(err.contains("Docker"), "error did not name Docker: {err}");
    assert!(
        err.contains("docs.docker.com"),
        "error did not include an install hint: {err}"
    );
}

#[test]
fn test_vm_start_names_unreachable_docker_daemon() {
    // Arrange
    let mock = prerequisite_mock(&[], false, true);

    // Act
    let err = start_error(mock);

    // Assert
    assert!(
        err.contains("daemon"),
        "error did not name the Docker daemon: {err}"
    );
    assert!(
        err.contains("docker` group"),
        "error did not include the docker group hint: {err}"
    );
}

#[test]
fn test_vm_start_names_missing_kvm() {
    // Arrange
    let mock = prerequisite_mock(&[], true, false);

    // Act
    let err = start_error(mock);

    // Assert
    assert!(err.contains("KVM"), "error did not name KVM: {err}");
    assert!(
        err.contains("/dev/kvm"),
        "error did not name /dev/kvm: {err}"
    );
}

#[test]
fn test_vm_start_names_missing_ssh_tools() {
    // Arrange
    let mock = prerequisite_mock(&["scp", "ssh-keygen"], true, true);

    // Act
    let err = start_error(mock);

    // Assert
    assert!(err.contains("OpenSSH"), "error did not name OpenSSH: {err}");
    assert!(
        err.contains("scp, ssh-keygen"),
        "error did not list the tools: {err}"
    );
    assert!(
        !err.contains("ssh, "),
        "error named ssh, which is present: {err}"
    );
}

#[test]
fn test_vm_start_names_every_missing_prerequisite_at_once() {
    // Arrange
    let mock = prerequisite_mock(&["docker", "ssh"], false, false);

    // Act
    let err = start_error(mock);

    // Assert
    for name in ["Docker", "KVM", "OpenSSH"] {
        assert!(err.contains(name), "error did not name {name}: {err}");
    }
}

/// Build a Linux-host mock with every prerequisite present and Docker and
/// the files faked. Tests add how the VM answers SSH.
fn vm_mock(docker: &FakeDocker, files: &FakeFiles, volume_exists: bool) -> MockSystem {
    vm_mock_with_env(docker, files, volume_exists, |key| {
        (key == "HOME").then(|| "/home/dev".to_owned())
    })
}

/// Build a [`vm_mock`] whose environment variables `env` answers.
fn vm_mock_with_env(
    docker: &FakeDocker,
    files: &FakeFiles,
    volume_exists: bool,
    env: fn(&str) -> Option<String>,
) -> MockSystem {
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_is_executable_in_path().returning(|_| true);
    mock.expect_docker_daemon_reachable().returning(|| true);
    mock.expect_env_var().returning(env);
    mock.expect_volume_exists()
        .returning(move |_| volume_exists);
    mock.expect_sleep().returning(|_| ());
    docker.install(&mut mock);
    files.install(&mut mock);
    mock
}

fn accept_ssh(mock: &mut MockSystem) {
    mock.expect_ssh_ready().returning(|_| true);
    mock.expect_now().returning(Instant::now);
}

/// Make the VM refuse SSH forever while each clock reading advances a
/// minute.
fn refuse_ssh(mock: &mut MockSystem) {
    mock.expect_ssh_ready().returning(|_| false);
    let start_time = Instant::now();
    let minutes = AtomicU64::new(0);
    mock.expect_now().returning(move || {
        start_time + Duration::from_secs(60 * minutes.fetch_add(1, Ordering::SeqCst))
    });
}

#[test]
fn test_vm_start_installs_windows_on_first_use() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[]);
    let mut mock = vm_mock(&docker, &files, false);
    accept_ssh(&mut mock);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert!(files.get(KEY).is_some(), "no SSH key was generated");
    assert_eq!(
        files
            .get(&format!("{DATA_DIR}/oem/authorized_keys"))
            .as_deref(),
        Some("ssh-ed25519 KEY"),
        "the OEM folder does not authorize the generated key"
    );
    let install_bat = files.get(&format!("{DATA_DIR}/oem/install.bat")).unwrap();
    assert!(
        install_bat.contains("OpenSSH.Server"),
        "install.bat does not enable the OpenSSH server: {install_bat}"
    );
    assert!(
        install_bat.contains(r"-ExclusionPath 'C:\cssh-rs'"),
        "install.bat does not exclude C:\\cssh-rs from Defender scans: {install_bat}"
    );
    let calls = docker.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    let run = calls[0].join(" ");
    for part in [
        "run --detach --name cssh-rs-windows-vm",
        "--device /dev/kvm",
        "--publish 127.0.0.1::22",
        "--publish 127.0.0.1::8006",
        "--env VERSION=11",
        "--volume cssh-rs-windows-vm:/storage",
    ] {
        assert!(run.contains(part), "`docker {run}` lacks `{part}`");
    }
    let oem_mount = calls[0]
        .iter()
        .find_map(|arg| arg.strip_suffix(":/oem:ro"))
        .unwrap_or_else(|| panic!("`docker {run}` mounts no OEM folder"));
    assert_eq!(Path::new(oem_mount), Path::new(DATA_DIR).join("oem"));
    assert!(
        calls[0].last().unwrap().starts_with("dockurr/windows:"),
        "`docker {run}` does not run the dockurr image last"
    );
}

#[test]
fn test_vm_start_reuses_the_installed_disk() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[(KEY, "PRIVATE"), (PUBLIC_KEY, "ssh-ed25519 OLD")]);
    let mut mock = vm_mock(&docker, &files, true);
    accept_ssh(&mut mock);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(docker.subcommands(), ["run"]);
    assert_eq!(files.get(KEY).as_deref(), Some("PRIVATE"));
}

#[test]
fn test_vm_start_refuses_a_disk_whose_key_is_gone() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[]);
    let mut mock = vm_mock(&docker, &files, true);
    accept_ssh(&mut mock);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("docker volume rm cssh-rs-windows-vm"),
        "error did not say how to reinstall: {err}"
    );
    assert!(docker.calls().is_empty(), "{:?}", docker.calls());
}

#[test]
fn test_vm_start_leaves_a_running_vm_running() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let files = FakeFiles::new(&[(KEY, "PRIVATE")]);
    let mut mock = vm_mock(&docker, &files, true);
    accept_ssh(&mut mock);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    assert!(!result.unwrap(), "start claimed to launch a running VM");
    assert!(docker.calls().is_empty(), "{:?}", docker.calls());
}

#[test]
fn test_vm_start_replaces_a_stopped_container() {
    // Arrange
    let docker = FakeDocker::new(Some("exited"));
    let files = FakeFiles::new(&[(KEY, "PRIVATE"), (PUBLIC_KEY, "ssh-ed25519 KEY")]);
    let mut mock = vm_mock(&docker, &files, true);
    accept_ssh(&mut mock);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(docker.subcommands(), ["rm", "run"]);
}

#[test]
fn test_vm_start_fails_when_the_container_stops_before_ssh_comes_up() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[(KEY, "PRIVATE"), (PUBLIC_KEY, "ssh-ed25519 KEY")]);
    let mut mock = vm_mock(&docker, &files, true);
    let crashing = docker.clone();
    mock.expect_ssh_ready().returning(move |_| {
        crashing.set_state(Some("exited"));
        false
    });
    mock.expect_now().returning(Instant::now);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("docker logs cssh-rs-windows-vm"),
        "error did not point at the container logs: {err}"
    );
}

#[test]
fn test_vm_start_waits_two_hours_for_a_first_install() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[]);
    let mut mock = vm_mock(&docker, &files, false);
    refuse_ssh(&mut mock);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("did not accept SSH within 120 minutes"),
        "error did not report the install timeout: {err}"
    );
}

#[test]
fn test_vm_start_gives_up_on_an_installed_vm_after_fifteen_minutes() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[(KEY, "PRIVATE"), (PUBLIC_KEY, "ssh-ed25519 KEY")]);
    let mut mock = vm_mock(&docker, &files, true);
    refuse_ssh(&mut mock);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("did not accept SSH within 15 minutes"),
        "error did not report the boot timeout: {err}"
    );
    assert!(
        err.contains("http://127.0.0.1:8006"),
        "error did not point at the web viewer: {err}"
    );
    assert!(
        err.contains(&format!("`cargo xtask vm start {TRIPLE}` again")),
        "error did not say how to keep waiting: {err}"
    );
    assert!(
        err.contains("docker volume rm cssh-rs-windows-vm"),
        "error did not say how to reinstall: {err}"
    );
    assert_eq!(docker.subcommands(), ["run"], "the VM was not left running");
}

#[test]
fn test_vm_start_keeps_its_files_under_xdg_data_home() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[]);
    let mut mock = vm_mock_with_env(&docker, &files, false, |key| match key {
        "XDG_DATA_HOME" => Some(XDG_DATA_HOME.to_owned()),
        "HOME" => Some("/home/dev".to_owned()),
        _ => None,
    });
    accept_ssh(&mut mock);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert!(
        files
            .get(&format!("{XDG_DATA_HOME}/cssh-rs/windows-vm/id_ed25519"))
            .is_some(),
        "no SSH key was generated under $XDG_DATA_HOME"
    );
    assert!(files.get(KEY).is_none(), "the SSH key went under $HOME");
}

#[test]
fn test_vm_start_ignores_a_relative_xdg_data_home() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[]);
    let mut mock = vm_mock_with_env(&docker, &files, false, |key| match key {
        "XDG_DATA_HOME" => Some("data".to_owned()),
        "HOME" => Some("/home/dev".to_owned()),
        _ => None,
    });
    accept_ssh(&mut mock);

    // Act
    let result = start(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert!(
        files.get(KEY).is_some(),
        "no SSH key was generated under $HOME"
    );
}

#[test]
fn test_vm_stop_shuts_down_and_removes_a_running_vm() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let files = FakeFiles::new(&[]);
    let mock = vm_mock(&docker, &files, true);

    // Act
    let result = stop(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(docker.subcommands(), ["stop", "rm"]);
}

#[test]
fn test_vm_stop_does_nothing_without_a_vm() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[]);
    let mock = vm_mock(&docker, &files, true);

    // Act
    let result = stop(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert!(docker.calls().is_empty(), "{:?}", docker.calls());
}

#[test]
fn test_vm_stop_removes_an_exited_container() {
    // Arrange
    let docker = FakeDocker::new(Some("exited"));
    let files = FakeFiles::new(&[]);
    let mock = vm_mock(&docker, &files, true);

    // Act
    let result = stop(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(docker.subcommands(), ["rm"]);
}

#[test]
fn test_vm_stop_on_macos_host_errors_without_touching_docker() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "macos");
    mock.expect_is_executable_in_path().never();
    mock.expect_list_containers().never();
    mock.expect_run_docker().never();

    // Act
    let result = stop(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(err.contains("macos"), "error did not name the host: {err}");
}

#[test]
fn test_vm_stop_names_missing_docker() {
    // Arrange
    let mut mock = prerequisite_mock(&["docker"], false, true);
    mock.expect_list_containers().never();
    mock.expect_run_docker().never();

    // Act
    let result = stop(&mock, Target::X86_64PcWindowsMsvc);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("docs.docker.com"),
        "error did not include an install hint: {err}"
    );
}
