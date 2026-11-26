//! Tests for the cross-test module.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cross_build::Target;
use crate::cross_test::{run_cross_test, run_test_binary};
use crate::mock_system::{strings, FakeDocker, FakeFiles, FakeSsh, MockSystem, KEY, PUBLIC_KEY};

const TRIPLE: &str = "x86_64-pc-windows-msvc";
const RUNNER: &str = "/work/target/debug/xtask cross-test-runner x86_64-pc-windows-msvc";
const BINARY: &str = "/work/target/x86_64-pc-windows-msvc/debug/deps/core-abc.exe";

fn cargo_args() -> Vec<String> {
    strings(&["-p", "cssh-rs-core", "input"])
}

fn forbid_vm(mock: &mut MockSystem) {
    mock.expect_docker_daemon_reachable().never();
    mock.expect_list_containers().never();
    mock.expect_run_docker().never();
    mock.expect_ssh_ready().never();
}

fn forbid_build(mock: &mut MockSystem) {
    mock.expect_list_installed_targets().never();
    mock.expect_install_target().never();
    mock.expect_install_cargo_subcommand().never();
    mock.expect_run_cargo().never();
}

#[test]
fn test_cross_test_windows_x64_host_runs_plain_cargo_test() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "windows");
    mock.expect_host_arch().returning(|| "x86_64");
    mock.expect_host_is_msvc().returning(|| true);
    forbid_vm(&mut mock);
    mock.expect_run_cargo()
        .withf(|args, envs| {
            args == strings(&["test", "-p", "cssh-rs-core", "input"]) && envs.is_empty()
        })
        .times(1)
        .returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, false, &cargo_args());

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_cross_test_windows_host_ignores_with_vm() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "windows");
    mock.expect_host_arch().returning(|| "x86_64");
    mock.expect_host_is_msvc().returning(|| true);
    forbid_vm(&mut mock);
    mock.expect_run_cargo().times(1).returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_cross_test_macos_host_errors_before_any_work() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "macos");
    forbid_vm(&mut mock);
    forbid_build(&mut mock);
    mock.expect_is_executable_in_path().never();

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(err.contains("macos"), "error did not name the host: {err}");
    assert!(err.contains(TRIPLE), "error did not name the target: {err}");
    assert!(
        err.contains("KVM"),
        "error did not say KVM is needed: {err}"
    );
}

/// Build a Linux-host mock on top of [`runner_mock`] with every
/// prerequisite present, the files faked, and a VM that accepts SSH. It
/// answers no toolchain probe; tests that get past the VM checks add
/// [`toolchain_ready`].
fn linux_mock(docker: &FakeDocker, files: &FakeFiles, ssh: &FakeSsh) -> MockSystem {
    let mut mock = runner_mock(docker, ssh, &[], |_| 0);
    mock.expect_host_os().returning(|| "linux");
    mock.expect_host_arch().returning(|| "x86_64");
    mock.expect_is_executable_in_path().returning(|_| true);
    mock.expect_docker_daemon_reachable().returning(|| true);
    mock.expect_volume_exists().returning(|_| true);
    mock.expect_ssh_ready().returning(|_| true);
    mock.expect_now().returning(Instant::now);
    mock.expect_current_exe()
        .returning(|| Ok(PathBuf::from("/work/target/debug/xtask")));
    mock.expect_pack_workspace().returning(|_, _| Ok(()));
    files.install(&mut mock);
    mock
}

fn toolchain_ready(mock: &mut MockSystem) {
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    mock.expect_list_cargo_subcommands()
        .returning(|| Ok("Installed Commands:\n    xwin\n".to_owned()));
}

fn installed_vm() -> FakeFiles {
    FakeFiles::new(&[(KEY, "PRIVATE"), (PUBLIC_KEY, "ssh-ed25519 KEY")])
}

#[test]
fn test_cross_test_linux_host_runs_cargo_xwin_test_through_the_vm_runner() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &installed_vm(), &ssh);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo()
        .withf(|args, envs| {
            args == strings(&[
                "xwin",
                "test",
                "--target",
                TRIPLE,
                "-p",
                "cssh-rs-core",
                "input",
            ]) && envs
                == [
                    (
                        "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER".to_owned(),
                        RUNNER.to_owned(),
                    ),
                    ("XWIN_ACCEPT_LICENSE".to_owned(), "1".to_owned()),
                ]
        })
        .times(1)
        .returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, false, &cargo_args());

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert!(docker.calls().is_empty(), "{:?}", docker.calls());
}

#[test]
fn test_cross_test_mirrors_the_workspace_into_the_vm_at_the_same_path() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &installed_vm(), &ssh);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        ssh.copies(),
        [(
            PathBuf::from("/work/target/cross-test/workspace.tar"),
            r"C:\cssh-rs\cross-test\workspace-7.tar".to_owned()
        )]
    );
    assert_eq!(
        ssh.commands(),
        [
            r#"(if exist "C:\work" rmdir /s /q "C:\work") && mkdir "C:\work" && (if not exist "C:\cssh-rs\cross-test" mkdir "C:\cssh-rs\cross-test")"#,
            r#"tar -xf "C:\cssh-rs\cross-test\workspace-7.tar" -C "C:\work" && del "C:\cssh-rs\cross-test\workspace-7.tar""#,
        ]
    );
}

#[test]
fn test_cross_test_errors_before_building_when_the_vm_is_not_running() {
    for vm_state in [None, Some("exited")] {
        // Arrange
        let docker = FakeDocker::new(vm_state);
        let ssh = FakeSsh::default();
        let mut mock = linux_mock(&docker, &installed_vm(), &ssh);
        forbid_build(&mut mock);

        // Act
        let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

        // Assert
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not running"),
            "{vm_state:?}: error did not say the VM is down: {err}"
        );
        assert!(
            err.contains("cargo xtask vm start x86_64-pc-windows-msvc"),
            "{vm_state:?}: error did not say how to start the VM: {err}"
        );
        assert!(
            err.contains("--with-vm"),
            "{vm_state:?}: error did not mention --with-vm: {err}"
        );
        assert!(docker.calls().is_empty(), "{:?}", docker.calls());
    }
}

#[test]
fn test_cross_test_names_missing_host_prerequisites_before_building() {
    for with_vm in [false, true] {
        // Arrange
        let mut mock = MockSystem::new();
        mock.expect_host_os().returning(|| "linux");
        mock.expect_current_exe()
            .returning(|| Ok(PathBuf::from("/work/target/debug/xtask")));
        mock.expect_is_executable_in_path()
            .returning(|name| name != "docker");
        mock.expect_path_exists()
            .withf(|path| path == Path::new("/dev/kvm"))
            .returning(|_| false);
        forbid_vm(&mut mock);
        forbid_build(&mut mock);

        // Act
        let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, with_vm, &[]);

        // Assert
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Docker"),
            "with_vm={with_vm}: error did not name Docker: {err}"
        );
        assert!(
            err.contains("KVM"),
            "with_vm={with_vm}: error did not name KVM: {err}"
        );
    }
}

#[test]
fn test_cross_test_errors_before_building_when_the_runner_path_holds_whitespace() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_current_exe()
        .returning(|| Ok(PathBuf::from("/home/dev/my work/target/debug/xtask")));
    forbid_vm(&mut mock);
    forbid_build(&mut mock);

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("my work"),
        "error did not name the offending path: {err}"
    );
    assert!(err.contains("whitespace"), "error did not say why: {err}");
}

#[test]
fn test_cross_test_with_vm_starts_the_vm_and_stops_it_after_the_run() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = installed_vm();
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &files, &ssh);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().times(1).returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(docker.subcommands(), ["run", "stop", "rm"]);
}

#[test]
fn test_cross_test_with_vm_stops_the_vm_when_the_tests_fail() {
    // Arrange
    let docker = FakeDocker::new(None);
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &installed_vm(), &ssh);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo()
        .returning(|_, _| anyhow::bail!("`cargo xwin test` failed with status 101"));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(err.contains("101"), "the test failure was lost: {err}");
    assert_eq!(docker.subcommands(), ["run", "stop", "rm"]);
}

#[test]
fn test_cross_test_with_vm_leaves_a_vm_started_by_vm_start_running() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &installed_vm(), &ssh);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert!(docker.calls().is_empty(), "{:?}", docker.calls());
}

/// Build a mock for the runner whose host sets `HOME` and `host_env`, and
/// whose VM commands exit with the code `respond` picks.
fn runner_mock(
    docker: &FakeDocker,
    ssh: &FakeSsh,
    host_env: &'static [(&'static str, &'static str)],
    respond: impl Fn(&str) -> i32 + Send + 'static,
) -> MockSystem {
    let mut mock = MockSystem::new();
    mock.expect_env_var().returning(move |key| {
        [("HOME", "/home/dev")]
            .iter()
            .chain(host_env)
            .find(|(name, _)| *name == key)
            .map(|(_, value)| (*value).to_owned())
    });
    mock.expect_workspace_root()
        .returning(|| Ok(PathBuf::from("/work")));
    mock.expect_current_dir()
        .returning(|| Ok(PathBuf::from("/work/cssh-rs-core")));
    mock.expect_process_id().returning(|| 7);
    mock.expect_sleep().returning(|_| ());
    docker.install(&mut mock);
    ssh.install(&mut mock, respond);
    mock
}

const MKDIR: &str = r#"if not exist "C:\cssh-rs\cross-test\7" mkdir "C:\cssh-rs\cross-test\7""#;
const RMDIR: &str = r#"rmdir /s /q "C:\cssh-rs\cross-test\7" 2>nul"#;
const RUN: &str = r#"set "INSTA_WORKSPACE_ROOT=C:\work" && cd /d "C:\work\cssh-rs-core" && "C:\cssh-rs\cross-test\7\core-abc.exe""#;

/// Windows' "The directory is not empty." error:
/// https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-
const ERROR_DIR_NOT_EMPTY: i32 = 145;

#[test]
fn test_runner_runs_the_binary_in_the_vm_and_returns_its_exit_code() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mock = runner_mock(&docker, &ssh, &[], |command| {
        if command.starts_with("set ") {
            101
        } else {
            0
        }
    });

    // Act
    let result = run_test_binary(
        &mock,
        Target::X86_64PcWindowsMsvc,
        &strings(&[BINARY, "--nocapture"]),
    );

    // Assert
    assert_eq!(result.unwrap(), 101);
    assert_eq!(
        ssh.copies(),
        [(
            PathBuf::from(BINARY),
            r"C:\cssh-rs\cross-test\7\core-abc.exe".to_owned()
        )]
    );
    assert_eq!(
        ssh.commands(),
        [
            MKDIR.to_owned(),
            format!("{RUN} --nocapture"),
            RMDIR.to_owned()
        ]
    );
}

#[test]
fn test_runner_quotes_arguments_for_cmd() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mock = runner_mock(&docker, &ssh, &[], |_| 0);

    // Act
    let result = run_test_binary(
        &mock,
        Target::X86_64PcWindowsMsvc,
        &strings(&[BINARY, "--skip", "a b", r"c d\", r"e\", "x&y", ""]),
    );

    // Assert
    assert_eq!(result.unwrap(), 0);
    assert_eq!(
        ssh.commands()[1],
        format!(r#"{RUN} --skip "a b" "c d\\" e\ "x&y" """#)
    );
}

#[test]
fn test_runner_rejects_arguments_cmd_cannot_pass_before_touching_the_vm() {
    for argument in [r#"say "hi""#, "100%"] {
        // Arrange
        let mut mock = MockSystem::new();
        mock.expect_list_containers().never();
        mock.expect_ssh().never();
        mock.expect_scp().never();

        // Act
        let result = run_test_binary(
            &mock,
            Target::X86_64PcWindowsMsvc,
            &strings(&[BINARY, argument]),
        );

        // Assert
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("cmd.exe"),
            "{argument}: error did not say why: {err}"
        );
    }
}

#[test]
fn test_runner_forwards_rust_test_variables_set_on_the_host() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mock = runner_mock(
        &docker,
        &ssh,
        &[
            ("RUST_TEST_THREADS", "1"),
            ("RUST_BACKTRACE", "full"),
            ("INSTA_UPDATE", "always"),
        ],
        |_| 0,
    );

    // Act
    let result = run_test_binary(&mock, Target::X86_64PcWindowsMsvc, &strings(&[BINARY]));

    // Assert
    assert_eq!(result.unwrap(), 0);
    assert_eq!(
        ssh.commands()[1],
        r#"set "INSTA_WORKSPACE_ROOT=C:\work" && set "RUST_BACKTRACE=full" && set "RUST_TEST_THREADS=1" && cd /d "C:\work\cssh-rs-core" && "C:\cssh-rs\cross-test\7\core-abc.exe""#
    );
}

#[test]
fn test_runner_rejects_a_forwarded_variable_cmd_cannot_pass_before_touching_the_vm() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_env_var()
        .returning(|key| (key == "RUST_TEST_THREADS").then(|| "%NUMBER_OF_PROCESSORS%".to_owned()));
    mock.expect_list_containers().never();
    mock.expect_ssh().never();
    mock.expect_scp().never();

    // Act
    let result = run_test_binary(&mock, Target::X86_64PcWindowsMsvc, &strings(&[BINARY]));

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("RUST_TEST_THREADS"),
        "error did not name the variable: {err}"
    );
    assert!(err.contains("cmd.exe"), "error did not say why: {err}");
}

#[test]
fn test_runner_retries_cleanup_while_windows_holds_the_binary() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let failed_removals = std::sync::atomic::AtomicU32::new(0);
    let mock = runner_mock(&docker, &ssh, &[], move |command| {
        let held = command.starts_with("rmdir")
            && failed_removals.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 2;
        if held {
            ERROR_DIR_NOT_EMPTY
        } else {
            0
        }
    });

    // Act
    let result = run_test_binary(&mock, Target::X86_64PcWindowsMsvc, &strings(&[BINARY]));

    // Assert
    assert_eq!(result.unwrap(), 0);
    let removals = ssh
        .commands()
        .into_iter()
        .filter(|command| command == RMDIR)
        .count();
    assert_eq!(removals, 3, "{:?}", ssh.commands());
}

#[test]
fn test_runner_cleans_up_when_the_copy_fails() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::failing_copies();
    let mock = runner_mock(&docker, &ssh, &[], |_| 0);

    // Act
    let result = run_test_binary(&mock, Target::X86_64PcWindowsMsvc, &strings(&[BINARY]));

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(err.contains("connection reset"), "{err}");
    assert_eq!(ssh.commands(), [MKDIR, RMDIR]);
}

#[test]
fn test_runner_fails_when_the_vm_is_not_running() {
    // Arrange
    let docker = FakeDocker::new(None);
    let ssh = FakeSsh::default();
    let mock = runner_mock(&docker, &ssh, &[], |_| 0);

    // Act
    let result = run_test_binary(&mock, Target::X86_64PcWindowsMsvc, &strings(&[BINARY]));

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("cargo xtask vm start"),
        "error did not say how to start the VM: {err}"
    );
    assert!(ssh.commands().is_empty(), "{:?}", ssh.commands());
}
