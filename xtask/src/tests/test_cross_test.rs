//! Tests for the cross-test module.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cross_build::Target;
use crate::cross_test::{run_cross_test, run_test_binary};
use crate::mock_system::{
    refuse_ssh, strings, vm_in_use, FakeDocker, FakeFiles, FakeSsh, MockSystem, AUTO_STOP, KEY,
    LAUNCH_LOCK, PUBLIC_KEY, VM_LOCK,
};

const TRIPLE: &str = "x86_64-pc-windows-msvc";
const ROOT: &str = "/work";
/// The VM directory that mirrors the worktree at [`ROOT`].
const MIRROR: &str = r"C:\cssh-rs\workspaces\1f92008dc8753ba3";
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

/// Build a Linux-host mock on top of [`runner_mock_at`] for a run in the
/// worktree at `root`, with every prerequisite present, the files faked,
/// and the worktree lock and a lease on the VM free. It answers no launch
/// lock, disk, SSH, toolchain or VM-in-use probe; tests add those.
fn linux_host(
    root: &'static str,
    docker: &FakeDocker,
    files: &FakeFiles,
    ssh: &FakeSsh,
) -> MockSystem {
    let mut mock = runner_mock_at(root, root, docker, ssh, &[], |_| 0);
    mock.expect_host_os().returning(|| "linux");
    mock.expect_host_arch().returning(|| "x86_64");
    mock.expect_is_executable_in_path().returning(|_| true);
    mock.expect_docker_daemon_reachable().returning(|| true);
    mock.expect_lock_shared()
        .withf(|path| path == Path::new(VM_LOCK))
        .returning(|_| Ok(()));
    let worktree_lock = worktree_lock(root);
    mock.expect_try_lock_exclusive()
        .withf(move |path| path == worktree_lock)
        .returning(|_| Ok(Some(())));
    mock.expect_current_exe()
        .returning(move || Ok(PathBuf::from(format!("{root}/target/debug/xtask"))));
    mock.expect_pack_workspace().returning(|_, _| Ok(()));
    files.install(&mut mock);
    mock
}

/// Build a [`linux_host`] mock in [`ROOT`] whose launch lock is free and
/// whose installed VM accepts SSH. `vm_in_use_at_end` says whether another
/// run holds the VM when this one ends.
fn linux_mock(
    docker: &FakeDocker,
    files: &FakeFiles,
    ssh: &FakeSsh,
    vm_in_use_at_end: bool,
) -> MockSystem {
    let mut mock = linux_host(ROOT, docker, files, ssh);
    mock.expect_volume_exists().returning(|_| true);
    free_launch_lock(&mut mock);
    accept_ssh(&mut mock);
    vm_in_use(&mut mock, vm_in_use_at_end);
    mock
}

fn worktree_lock(root: &str) -> PathBuf {
    Path::new(root).join("target/cross-test/worktree.lock")
}

fn free_launch_lock(mock: &mut MockSystem) {
    mock.expect_lock_exclusive()
        .withf(|path| path == Path::new(LAUNCH_LOCK))
        .returning(|_| Ok(()));
}

fn accept_ssh(mock: &mut MockSystem) {
    mock.expect_ssh_ready().returning(|_| true);
    mock.expect_now().returning(Instant::now);
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
    let mut mock = linux_mock(&docker, &installed_vm(), &ssh, false);
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
fn test_cross_test_mirrors_the_workspace_into_a_directory_of_the_worktree_in_the_vm() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &installed_vm(), &ssh, false);
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
            format!(
                r#"(if exist "{MIRROR}" rmdir /s /q "{MIRROR}") && mkdir "{MIRROR}" && (if not exist "C:\cssh-rs\cross-test" mkdir "C:\cssh-rs\cross-test")"#
            ),
            format!(
                r#"tar -xf "C:\cssh-rs\cross-test\workspace-7.tar" -C "{MIRROR}" && del "C:\cssh-rs\cross-test\workspace-7.tar""#
            ),
        ]
    );
}

#[test]
fn test_cross_test_mirrors_a_nested_worktree_outside_the_mirror_of_the_outer_one() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mut mock = linux_host(
        "/work/.claude/worktrees/feature",
        &docker,
        &installed_vm(),
        &ssh,
    );
    free_launch_lock(&mut mock);
    accept_ssh(&mut mock);
    vm_in_use(&mut mock, false);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    let nested = r"C:\cssh-rs\workspaces\2b79c7e0ed46dd10";
    let commands = ssh.commands();
    assert!(
        commands[0].starts_with(&format!(r#"(if exist "{nested}" rmdir /s /q "{nested}")"#)),
        "the nested worktree does not replace its own mirror beside {MIRROR}: {commands:?}"
    );
    assert!(
        commands.iter().all(|command| !command.contains(MIRROR)),
        "the nested worktree mirrors inside {MIRROR}, which runs in /work replace: {commands:?}"
    );
}

#[test]
fn test_cross_test_errors_before_building_when_the_vm_is_not_running() {
    for vm_state in [None, Some("exited")] {
        // Arrange
        let docker = FakeDocker::new(vm_state);
        let ssh = FakeSsh::default();
        let mut mock = linux_mock(&docker, &installed_vm(), &ssh, false);
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

/// Build a Linux-host mock for a run in the worktree at `root`, whose
/// worktree lock is free unless `worktree_busy`, and that must neither
/// touch the VM nor build.
fn worktree_mock(root: &'static str, worktree_busy: bool) -> MockSystem {
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_workspace_root()
        .returning(move || Ok(PathBuf::from(root)));
    mock.expect_create_dir_all().returning(|_| Ok(()));
    let worktree_lock = worktree_lock(root);
    mock.expect_try_lock_exclusive()
        .withf(move |path| path == worktree_lock)
        .returning(move |_| Ok((!worktree_busy).then_some(())));
    mock.expect_current_exe()
        .returning(move || Ok(PathBuf::from(format!("{root}/target/debug/xtask"))));
    forbid_vm(&mut mock);
    forbid_build(&mut mock);
    mock
}

#[test]
fn test_cross_test_names_missing_host_prerequisites_before_building() {
    for with_vm in [false, true] {
        // Arrange
        let mut mock = worktree_mock(ROOT, false);
        mock.expect_is_executable_in_path()
            .returning(|name| name != "docker");
        mock.expect_path_exists()
            .withf(|path| path == Path::new("/dev/kvm"))
            .returning(|_| false);

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
    let mock = worktree_mock("/home/dev/my work", false);

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
fn test_cross_test_refuses_a_second_run_in_the_same_worktree() {
    // Arrange
    let mut mock = worktree_mock(ROOT, true);
    mock.expect_lock_shared().never();

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("already running in this worktree"),
        "error did not name the conflict: {err}"
    );
    assert!(
        err.contains("/work"),
        "error did not name the worktree: {err}"
    );
}

#[test]
fn test_cross_test_with_vm_starts_the_vm_and_stops_it_after_the_run() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = installed_vm();
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &files, &ssh, false);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().times(1).returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(docker.subcommands(), ["run", "stop", "rm"]);
    assert_eq!(files.get(AUTO_STOP), None);
}

#[test]
fn test_cross_test_with_vm_stops_the_vm_when_the_tests_fail() {
    // Arrange
    let docker = FakeDocker::new(None);
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &installed_vm(), &ssh, false);
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
fn test_cross_test_with_vm_reuses_a_vm_another_run_launched_while_it_waited() {
    // Arrange
    let docker = FakeDocker::new(None);
    let ssh = FakeSsh::default();
    let mut mock = linux_host(ROOT, &docker, &installed_vm(), &ssh);
    let other_run = docker.clone();
    mock.expect_lock_exclusive()
        .withf(|path| path == Path::new(LAUNCH_LOCK))
        .returning(move |_| {
            other_run.set_state(Some("running"));
            Ok(())
        });
    accept_ssh(&mut mock);
    vm_in_use(&mut mock, true);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert!(
        docker.calls().is_empty(),
        "launched a second VM: {:?}",
        docker.calls()
    );
}

#[test]
fn test_cross_test_with_vm_leaves_a_first_install_running_when_it_times_out() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = FakeFiles::new(&[]);
    let ssh = FakeSsh::default();
    let mut mock = linux_host(ROOT, &docker, &files, &ssh);
    mock.expect_volume_exists().returning(|_| false);
    free_launch_lock(&mut mock);
    refuse_ssh(&mut mock);
    vm_in_use(&mut mock, false);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().never();

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("within 120 minutes"),
        "error did not report the install timeout: {err}"
    );
    assert!(
        err.contains(&format!("`cargo xtask vm start {TRIPLE}` again")),
        "error did not say how to keep waiting: {err}"
    );
    assert_eq!(docker.subcommands(), ["run"], "the install was stopped");
    assert_eq!(files.get(AUTO_STOP), None);
}

#[test]
fn test_cross_test_with_vm_leaves_a_vm_that_does_not_come_up_for_inspection() {
    // Arrange
    let docker = FakeDocker::new(Some("exited"));
    let files = FakeFiles::new(&[
        (KEY, "PRIVATE"),
        (PUBLIC_KEY, "ssh-ed25519 KEY"),
        (AUTO_STOP, ""),
    ]);
    let ssh = FakeSsh::default();
    let mut mock = linux_host(ROOT, &docker, &files, &ssh);
    mock.expect_volume_exists().returning(|_| true);
    free_launch_lock(&mut mock);
    let crashing = docker.clone();
    mock.expect_ssh_ready().returning(move |_| {
        crashing.set_state(Some("exited"));
        false
    });
    mock.expect_now().returning(Instant::now);
    vm_in_use(&mut mock, false);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().never();

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("docker logs cssh-rs-windows-vm"),
        "error did not point at the container logs: {err}"
    );
    assert_eq!(
        docker.subcommands(),
        ["rm", "run"],
        "the container whose logs the error names was removed"
    );
    assert_eq!(
        files.get(AUTO_STOP),
        None,
        "the marker of the VM that went away would stop the next one"
    );
}

#[test]
fn test_cross_test_with_vm_leaves_a_vm_started_by_vm_start_running() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &installed_vm(), &ssh, false);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert!(docker.calls().is_empty(), "{:?}", docker.calls());
}

#[test]
fn test_cross_test_with_vm_leaves_the_vm_to_another_run_still_using_it() {
    // Arrange
    let docker = FakeDocker::new(None);
    let files = installed_vm();
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &files, &ssh, true);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(docker.subcommands(), ["run"]);
    assert_eq!(
        files.get(AUTO_STOP).as_deref(),
        Some(""),
        "the run still using the VM would not stop it"
    );
}

#[test]
fn test_cross_test_last_run_leaves_a_vm_that_vm_start_claimed_while_it_waited() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let files = FakeFiles::new(&[(KEY, "PRIVATE"), (AUTO_STOP, "")]);
    let ssh = FakeSsh::default();
    let mut mock = linux_host(ROOT, &docker, &files, &ssh);
    free_launch_lock(&mut mock);
    accept_ssh(&mut mock);
    let vm_start = files.clone();
    mock.expect_try_lock_exclusive()
        .withf(|path| path == Path::new(VM_LOCK))
        .returning(move |_| {
            vm_start.remove(AUTO_STOP);
            Ok(Some(()))
        });
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert!(docker.calls().is_empty(), "{:?}", docker.calls());
}

#[test]
fn test_cross_test_last_run_stops_a_vm_another_run_started_with_vm() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let files = FakeFiles::new(&[(KEY, "PRIVATE"), (AUTO_STOP, "")]);
    let ssh = FakeSsh::default();
    let mut mock = linux_mock(&docker, &files, &ssh, false);
    toolchain_ready(&mut mock);
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_test(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(docker.subcommands(), ["stop", "rm"]);
    assert_eq!(files.get(AUTO_STOP), None);
}

/// Build a [`runner_mock_at`] mock that cargo runs in the `cssh-rs-core`
/// package of the worktree at [`ROOT`].
fn runner_mock(
    docker: &FakeDocker,
    ssh: &FakeSsh,
    host_env: &'static [(&'static str, &'static str)],
    respond: impl Fn(&str) -> i32 + Send + 'static,
) -> MockSystem {
    runner_mock_at(ROOT, "/work/cssh-rs-core", docker, ssh, host_env, respond)
}

/// Build a mock for the runner that cargo runs in `cwd` for the worktree
/// at `root`, whose host sets `HOME` and `host_env`, and whose VM commands
/// exit with the code `respond` picks.
fn runner_mock_at(
    root: &'static str,
    cwd: &'static str,
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
        .returning(move || Ok(PathBuf::from(root)));
    mock.expect_current_dir()
        .returning(move || Ok(PathBuf::from(cwd)));
    mock.expect_process_id().returning(|| 7);
    mock.expect_sleep().returning(|_| ());
    docker.install(&mut mock);
    ssh.install(&mut mock, respond);
    mock
}

const MKDIR: &str = r#"if not exist "C:\cssh-rs\cross-test\7" mkdir "C:\cssh-rs\cross-test\7""#;
const RMDIR: &str = r#"rmdir /s /q "C:\cssh-rs\cross-test\7" 2>nul"#;
const RUN: &str = r#"set "INSTA_WORKSPACE_ROOT=C:\cssh-rs\workspaces\1f92008dc8753ba3" && cd /d "C:\cssh-rs\workspaces\1f92008dc8753ba3\cssh-rs-core" && "C:\cssh-rs\cross-test\7\core-abc.exe""#;

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
        format!(
            r#"set "INSTA_WORKSPACE_ROOT={MIRROR}" && set "RUST_BACKTRACE=full" && set "RUST_TEST_THREADS=1" && cd /d "{MIRROR}\cssh-rs-core" && "C:\cssh-rs\cross-test\7\core-abc.exe""#
        )
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

#[test]
fn test_runner_fails_when_cargo_runs_the_binary_outside_the_workspace() {
    // Arrange
    let docker = FakeDocker::new(Some("running"));
    let ssh = FakeSsh::default();
    let mock = runner_mock_at(ROOT, "/elsewhere", &docker, &ssh, &[], |_| 0);

    // Act
    let result = run_test_binary(&mock, Target::X86_64PcWindowsMsvc, &strings(&[BINARY]));

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("/elsewhere") && err.contains("outside the workspace /work"),
        "error did not name the directories: {err}"
    );
    assert_eq!(ssh.commands(), [MKDIR, RMDIR]);
}
