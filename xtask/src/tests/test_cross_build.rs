//! Tests for the cross-build module.

use crate::cross_build::{
    prepare_toolchain, run_cargo_for, run_cross_build, Target, Toolchain, ToolchainKind,
};
use crate::mock_system::{strings, MockSystem};

const TRIPLE: &str = "x86_64-pc-windows-msvc";

fn xwin_license() -> Vec<(String, String)> {
    vec![("XWIN_ACCEPT_LICENSE".to_owned(), "1".to_owned())]
}

/// Build a mock for `host`/`arch`, with a Windows host on MSVC, that expects
/// exactly one `cargo` run with `args` and `envs`. It answers no toolchain
/// probe, so a test that reaches one fails unless it adds the probes it allows.
fn mock_expecting_cargo(
    host: &'static str,
    arch: &'static str,
    args: Vec<String>,
    envs: Vec<(String, String)>,
) -> MockSystem {
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(move || host);
    mock.expect_host_arch().returning(move || arch);
    mock.expect_host_is_msvc()
        .returning(move || host == "windows");
    expect_cargo(&mut mock, args, envs);
    mock
}

fn expect_cargo(mock: &mut MockSystem, args: Vec<String>, envs: Vec<(String, String)>) {
    mock.expect_run_cargo()
        .withf(move |actual_args, actual_envs| actual_args == args && actual_envs == envs)
        .times(1)
        .returning(|_, _| Ok(()));
}

fn expect_rust_target_installed(mock: &mut MockSystem) {
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
}

fn expect_cargo_xwin_ready(mock: &mut MockSystem) {
    expect_rust_target_installed(mock);
    mock.expect_is_executable_in_path().returning(|_| true);
    mock.expect_list_cargo_subcommands().returning(|| {
        Ok("Installed Commands:\n    xwin                  Cross compile\n".to_owned())
    });
}

#[test]
fn test_cross_build_windows_x64_host_runs_plain_cargo_build() {
    // Arrange
    // The host is the target: any rustup, cargo-xwin or LLVM probe panics.
    let mock = mock_expecting_cargo(
        "windows",
        "x86_64",
        strings(&["build", "--package", "cssh-rs"]),
        vec![],
    );

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_cross_build_windows_arm64_host_builds_with_target_flag() {
    // Arrange
    let mut mock = mock_expecting_cargo(
        "windows",
        "aarch64",
        strings(&["build", "--target", TRIPLE, "--package", "cssh-rs"]),
        vec![],
    );
    expect_rust_target_installed(&mut mock);

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_cross_build_leaves_xtask_out_of_a_workspace_build_on_a_windows_x64_host() {
    // Arrange
    let mock = mock_expecting_cargo(
        "windows",
        "x86_64",
        strings(&[
            "build",
            "--release",
            "--workspace",
            "--all-targets",
            "--exclude",
            "xtask",
        ]),
        vec![],
    );

    // Act
    let result = run_cross_build(
        &mock,
        Target::X86_64PcWindowsMsvc,
        true,
        &strings(&["--workspace", "--all-targets"]),
    );

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_cross_build_package_arguments_replace_the_cssh_rs_default() {
    for extra in [
        vec!["--workspace"],
        vec!["--all"],
        vec!["-p", "cssh-rs-core"],
        vec!["-pcssh-rs-core"],
        vec!["--package", "cssh-rs-core"],
        vec!["--package=cssh-rs-core"],
    ] {
        // Arrange
        // A Windows arm64 host builds with `--target`, so xtask stays in a
        // workspace build.
        let mut mock = mock_expecting_cargo(
            "windows",
            "aarch64",
            strings(&[&["build", "--target", TRIPLE], extra.as_slice()].concat()),
            vec![],
        );
        expect_rust_target_installed(&mut mock);

        // Act
        let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false, &strings(&extra));

        // Assert
        assert!(result.is_ok(), "{extra:?}: {result:?}");
    }
}

#[test]
fn test_cross_build_keeps_the_cssh_rs_default_beside_other_arguments() {
    // Arrange
    let mut mock = mock_expecting_cargo(
        "linux",
        "x86_64",
        strings(&[
            "xwin",
            "build",
            "--target",
            TRIPLE,
            "--package",
            "cssh-rs",
            "--all-targets",
        ]),
        xwin_license(),
    );
    expect_cargo_xwin_ready(&mut mock);

    // Act
    let result = run_cross_build(
        &mock,
        Target::X86_64PcWindowsMsvc,
        false,
        &strings(&["--all-targets"]),
    );

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_cross_build_windows_gnu_host_builds_with_target_flag() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "windows");
    mock.expect_host_arch().returning(|| "x86_64");
    mock.expect_host_is_msvc().returning(|| false);
    expect_rust_target_installed(&mut mock);
    expect_cargo(
        &mut mock,
        strings(&[
            "build",
            "--target",
            TRIPLE,
            "--package",
            "cssh-rs",
            "--release",
        ]),
        vec![],
    );

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_cross_build_linux_and_macos_hosts_use_cargo_xwin() {
    for host in ["linux", "macos"] {
        // Arrange
        let mut mock = mock_expecting_cargo(
            host,
            "x86_64",
            strings(&[
                "xwin",
                "build",
                "--target",
                TRIPLE,
                "--package",
                "cssh-rs",
                "--release",
            ]),
            xwin_license(),
        );
        expect_cargo_xwin_ready(&mut mock);

        // Act
        let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, true, &[]);

        // Assert
        assert!(result.is_ok(), "{host}: {result:?}");
    }
}

#[test]
fn test_run_cargo_for_adds_the_xwin_license_to_the_callers_variables() {
    // Arrange
    let mut mock = mock_expecting_cargo(
        "linux",
        "x86_64",
        strings(&["xwin", "test", "--target", TRIPLE, "--", "--nocapture"]),
        vec![
            ("RUST_BACKTRACE".to_owned(), "1".to_owned()),
            ("XWIN_ACCEPT_LICENSE".to_owned(), "1".to_owned()),
        ],
    );
    expect_cargo_xwin_ready(&mut mock);
    let toolchain = prepare_toolchain(&mock, Target::X86_64PcWindowsMsvc).unwrap();

    // Act
    let result = run_cargo_for(
        &mock,
        toolchain,
        "test",
        &strings(&["--", "--nocapture"]),
        &[("RUST_BACKTRACE".to_owned(), "1".to_owned())],
    );

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_binary_path_has_the_triple_unless_the_host_is_the_target() {
    // Arrange
    let toolchain = |kind| Toolchain {
        target: Target::X86_64PcWindowsMsvc,
        kind,
    };

    // Act
    let host = toolchain(ToolchainKind::Host).binary_path("debug");
    let with_target = toolchain(ToolchainKind::HostWithTarget).binary_path("release");
    let cargo_xwin = toolchain(ToolchainKind::CargoXwin).binary_path("release");

    // Assert
    assert_eq!(host, "target/debug/cssh-rs.exe");
    assert_eq!(with_target, format!("target/{TRIPLE}/release/cssh-rs.exe"));
    assert_eq!(cargo_xwin, format!("target/{TRIPLE}/release/cssh-rs.exe"));
}

#[test]
fn test_cross_build_installs_missing_rust_target() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "windows");
    mock.expect_host_arch().returning(|| "aarch64");
    mock.expect_list_installed_targets()
        .returning(|| Ok("aarch64-pc-windows-msvc\n".to_owned()));
    mock.expect_install_target()
        .withf(|triple| triple == TRIPLE)
        .times(1)
        .returning(|_| Ok(()));
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_cross_build_installs_missing_cargo_xwin_at_pinned_version() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_host_arch().returning(|| "x86_64");
    expect_rust_target_installed(&mut mock);
    mock.expect_is_executable_in_path().returning(|_| true);
    mock.expect_list_cargo_subcommands()
        .returning(|| Ok("Installed Commands:\n    build\n    test\n".to_owned()));
    // The value read from the version file must reach `cargo install`
    // verbatim; the literal is irrelevant.
    const SENTINEL: &str = "0.0.0-test";
    mock.expect_read_cargo_xwin_version()
        .returning(|| Ok(SENTINEL.to_owned()));
    mock.expect_install_cargo_subcommand()
        .withf(|name, version| name == "cargo-xwin" && version == SENTINEL)
        .times(1)
        .returning(|_, _| Ok(()));
    mock.expect_run_cargo().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_cross_build_unsupported_host_errors_before_any_build_work() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "freebsd");
    mock.expect_host_arch().returning(|| "x86_64");
    mock.expect_list_installed_targets().never();
    mock.expect_install_target().never();
    mock.expect_list_cargo_subcommands().never();
    mock.expect_install_cargo_subcommand().never();
    mock.expect_run_cargo().never();

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(err.contains("freebsd"), "error did not mention host: {err}");
    assert!(err.contains(TRIPLE), "error did not mention target: {err}");
}

#[test]
fn test_cross_build_propagates_build_failure() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "windows");
    mock.expect_host_arch().returning(|| "x86_64");
    mock.expect_host_is_msvc().returning(|| true);
    mock.expect_run_cargo()
        .returning(|_, _| anyhow::bail!("link.exe equivalent crashed"));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("link.exe equivalent crashed"));
}

#[test]
fn test_cross_build_errors_when_llvm_tooling_missing() {
    // Arrange
    let mut mock = MockSystem::new();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_host_arch().returning(|| "x86_64");
    expect_rust_target_installed(&mut mock);
    mock.expect_is_executable_in_path()
        .returning(|name| name != "llvm-rc");
    mock.expect_list_cargo_subcommands().never();
    mock.expect_install_cargo_subcommand().never();
    mock.expect_run_cargo().never();

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false, &[]);

    // Assert
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("llvm-rc"),
        "error did not mention the missing tool: {err}"
    );
    assert!(
        err.contains("apt install") || err.contains("dnf install"),
        "error did not include a Linux install hint: {err}"
    );
}

#[test]
fn test_target_triple_matches_value_enum_name() {
    // The clap value name and the runtime triple must match so
    // `cargo xtask cross-build x86_64-pc-windows-msvc` and the
    // resulting `--target` flag agree.
    assert_eq!(Target::X86_64PcWindowsMsvc.triple(), TRIPLE);
}
