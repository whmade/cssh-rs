//! Tests for the cross-build module.

use mockall::mock;

use crate::cross_build::system::windows_msvc::WindowsMsvcSystem;
use crate::cross_build::system::CrossBuildSystem;
use crate::cross_build::{run_cross_build, Target};

mock! {
    CrossBuildSystemMock {}
    impl CrossBuildSystem for CrossBuildSystemMock {
        fn host_os(&self) -> &'static str;
        fn print_info(&self, message: &str);
        fn list_installed_targets(&self) -> anyhow::Result<String>;
        fn install_target(&self, triple: &str) -> anyhow::Result<()>;
        fn is_executable_in_path(&self, name: &str) -> bool;
        fn list_cargo_subcommands(&self) -> anyhow::Result<String>;
        fn install_cargo_subcommand(&self, crate_name: &str, version: &str) -> anyhow::Result<()>;
        fn run_cargo_build(&self, triple: &str, release: bool) -> anyhow::Result<()>;
    }
    impl WindowsMsvcSystem for CrossBuildSystemMock {
        fn read_cargo_xwin_version(&self) -> anyhow::Result<String>;
        fn run_cargo_xwin_build(&self, triple: &str, release: bool) -> anyhow::Result<()>;
    }
}

const TRIPLE: &str = "x86_64-pc-windows-msvc";
const PINNED_XWIN_VERSION: &str = "0.18.6";

/// Build a mock with `print_info` swallowing any call. Callers
/// configure the remaining expectations.
fn base_mock() -> MockCrossBuildSystemMock {
    let mut mock = MockCrossBuildSystemMock::new();
    mock.expect_print_info().returning(|_| ());
    mock
}

#[test]
fn test_cross_build_windows_host_uses_native_cargo_build() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "windows");
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    mock.expect_install_target().never();
    // cargo-xwin and its LLVM preflight must never be touched when
    // the host can build natively.
    mock.expect_list_cargo_subcommands().never();
    mock.expect_install_cargo_subcommand().never();
    mock.expect_read_cargo_xwin_version().never();
    mock.expect_is_executable_in_path().never();
    mock.expect_run_cargo_xwin_build().never();
    mock.expect_run_cargo_build()
        .withf(|t, release| t == TRIPLE && !*release)
        .times(1)
        .returning(|_, _| Ok(()));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false);

    // Assert
    assert!(result.is_ok());
}

#[test]
fn test_cross_build_release_flag_threads_through_native_path() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "windows");
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    mock.expect_run_cargo_build()
        .withf(|t, release| t == TRIPLE && *release)
        .times(1)
        .returning(|_, _| Ok(()));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, true);

    // Assert
    assert!(result.is_ok());
}

#[test]
fn test_cross_build_linux_host_uses_cargo_xwin() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    mock.expect_install_target().never();
    mock.expect_is_executable_in_path().returning(|_| true);
    mock.expect_list_cargo_subcommands().returning(|| {
        Ok("Installed Commands:\n    build\n    xwin                  Cross compile\n".to_owned())
    });
    mock.expect_install_cargo_subcommand().never();
    mock.expect_read_cargo_xwin_version().never();
    mock.expect_run_cargo_build().never();
    mock.expect_run_cargo_xwin_build()
        .withf(|t, release| t == TRIPLE && !*release)
        .times(1)
        .returning(|_, _| Ok(()));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false);

    // Assert
    assert!(result.is_ok());
}

#[test]
fn test_cross_build_release_flag_threads_through_xwin_path() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    mock.expect_is_executable_in_path().returning(|_| true);
    mock.expect_list_cargo_subcommands()
        .returning(|| Ok("Installed Commands:\n    xwin\n".to_owned()));
    mock.expect_run_cargo_xwin_build()
        .withf(|t, release| t == TRIPLE && *release)
        .times(1)
        .returning(|_, _| Ok(()));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, true);

    // Assert
    assert!(result.is_ok());
}

#[test]
fn test_cross_build_macos_host_uses_cargo_xwin() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "macos");
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    mock.expect_is_executable_in_path().returning(|_| true);
    mock.expect_list_cargo_subcommands()
        .returning(|| Ok("Installed Commands:\n    xwin\n".to_owned()));
    mock.expect_install_cargo_subcommand().never();
    mock.expect_run_cargo_xwin_build()
        .times(1)
        .returning(|_, _| Ok(()));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false);

    // Assert
    assert!(result.is_ok());
}

#[test]
fn test_cross_build_installs_missing_rust_target() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "windows");
    // Target not in the installed list.
    mock.expect_list_installed_targets()
        .returning(|| Ok("x86_64-unknown-linux-gnu\n".to_owned()));
    mock.expect_install_target()
        .withf(|t| t == TRIPLE)
        .times(1)
        .returning(|_| Ok(()));
    mock.expect_is_executable_in_path().never();
    mock.expect_run_cargo_build().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false);

    // Assert
    assert!(result.is_ok());
}

#[test]
fn test_cross_build_installs_missing_cargo_xwin_at_pinned_version() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    mock.expect_install_target().never();
    mock.expect_is_executable_in_path().returning(|_| true);
    // cargo --list does not include xwin.
    mock.expect_list_cargo_subcommands()
        .returning(|| Ok("Installed Commands:\n    build\n    test\n".to_owned()));
    mock.expect_read_cargo_xwin_version()
        .returning(|| Ok(PINNED_XWIN_VERSION.to_owned()));
    mock.expect_install_cargo_subcommand()
        .withf(|c, v| c == "cargo-xwin" && v == PINNED_XWIN_VERSION)
        .times(1)
        .returning(|_, _| Ok(()));
    mock.expect_run_cargo_xwin_build().returning(|_, _| Ok(()));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false);

    // Assert
    assert!(result.is_ok());
}

#[test]
fn test_cross_build_unsupported_host_errors_before_any_build_work() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "freebsd");
    // The rust target install is still attempted (the triple may be
    // valid on the host even when the build path is not), but no
    // build or helper-tool call should fire when the (host, target)
    // pair is unsupported.
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    mock.expect_install_target().never();
    mock.expect_list_cargo_subcommands().never();
    mock.expect_install_cargo_subcommand().never();
    mock.expect_run_cargo_build().never();
    mock.expect_run_cargo_xwin_build().never();

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false);

    // Assert
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("freebsd"), "error did not mention host: {err}");
    assert!(err.contains(TRIPLE), "error did not mention target: {err}");
}

#[test]
fn test_cross_build_propagates_build_failure() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    mock.expect_is_executable_in_path().returning(|_| true);
    mock.expect_list_cargo_subcommands()
        .returning(|| Ok("Installed Commands:\n    xwin\n".to_owned()));
    mock.expect_run_cargo_xwin_build()
        .returning(|_, _| anyhow::bail!("link.exe equivalent crashed"));

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false);

    // Assert
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("link.exe equivalent crashed"));
}

#[test]
fn test_cross_build_errors_when_llvm_tooling_missing() {
    // Arrange
    let mut mock = base_mock();
    mock.expect_host_os().returning(|| "linux");
    mock.expect_list_installed_targets()
        .returning(|| Ok(format!("{TRIPLE}\n")));
    // Pretend llvm-rc is missing; clang and lld-link are present.
    mock.expect_is_executable_in_path()
        .returning(|name| name != "llvm-rc");
    // cargo-xwin must not be touched once the preflight has failed.
    mock.expect_list_cargo_subcommands().never();
    mock.expect_install_cargo_subcommand().never();
    mock.expect_run_cargo_xwin_build().never();

    // Act
    let result = run_cross_build(&mock, Target::X86_64PcWindowsMsvc, false);

    // Assert
    assert!(result.is_err());
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
