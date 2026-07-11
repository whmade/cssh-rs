//! Tests for the record_demo module.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mockall::mock;

use crate::record_demo::{record_demo, DemoSystem};

mock! {
    DemoSystemMock {}
    impl DemoSystem for DemoSystemMock {
        fn is_windows(&self) -> bool;
        fn workspace_root(&self) -> anyhow::Result<PathBuf>;
        fn build_binary(&self) -> anyhow::Result<PathBuf>;
        fn pip_install(&self, package_dir: &Path) -> anyhow::Result<()>;
        fn run_demo(&self, binary: &Path, output_dir: &Path, gif: &Path) -> anyhow::Result<()>;
        fn path_exists(&self, path: &Path) -> bool;
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from("ws-root")
}

fn binary_path() -> PathBuf {
    workspace_root()
        .join("target")
        .join("debug")
        .join("cssh-rs.exe")
}

/// Captured arguments from the single `run_demo` call.
#[derive(Clone, Default)]
struct DemoCall {
    binary: PathBuf,
    output_dir: PathBuf,
    gif: PathBuf,
}

#[test]
fn test_bails_on_non_windows() {
    let mut mock = MockDemoSystemMock::new();
    mock.expect_is_windows().times(1).returning(|| false);

    let err = record_demo(&mock).unwrap_err();

    assert!(
        err.to_string().contains("Windows only"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_happy_path_builds_installs_and_records() {
    // Arrange
    let mut mock = MockDemoSystemMock::new();
    mock.expect_is_windows().returning(|| true);
    mock.expect_workspace_root()
        .returning(|| Ok(workspace_root()));
    mock.expect_build_binary()
        .times(1)
        .returning(|| Ok(binary_path()));

    let installed = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
    let installed_slot = Arc::clone(&installed);
    mock.expect_pip_install().times(2).returning(move |dir| {
        installed_slot.lock().unwrap().push(dir.to_path_buf());
        Ok(())
    });

    let recorded = Arc::new(Mutex::new(DemoCall::default()));
    let recorded_slot = Arc::clone(&recorded);
    mock.expect_run_demo()
        .times(1)
        .returning(move |binary, output_dir, gif| {
            *recorded_slot.lock().unwrap() = DemoCall {
                binary: binary.to_path_buf(),
                output_dir: output_dir.to_path_buf(),
                gif: gif.to_path_buf(),
            };
            Ok(())
        });
    mock.expect_path_exists().times(1).returning(|_| true);

    // Act
    record_demo(&mock).expect("record_demo should succeed");

    // Assert
    let installed = installed.lock().unwrap().clone();
    assert_eq!(
        installed,
        vec![
            workspace_root().join("automation"),
            workspace_root().join("demo")
        ]
    );

    let call = recorded.lock().unwrap().clone();
    assert_eq!(call.binary, binary_path());
    assert_eq!(
        call.output_dir,
        workspace_root().join("target").join("demo")
    );
    assert_eq!(
        call.gif,
        workspace_root()
            .join("target")
            .join("demo")
            .join("cssh-rs.gif")
    );
}

#[test]
fn test_fails_when_gif_not_written() {
    let mut mock = MockDemoSystemMock::new();
    mock.expect_is_windows().returning(|| true);
    mock.expect_workspace_root()
        .returning(|| Ok(workspace_root()));
    mock.expect_build_binary().returning(|| Ok(binary_path()));
    mock.expect_pip_install().returning(|_| Ok(()));
    mock.expect_run_demo().returning(|_, _, _| Ok(()));
    mock.expect_path_exists().times(1).returning(|_| false);

    let err = record_demo(&mock).unwrap_err();

    assert!(
        err.to_string().contains("no GIF was written"),
        "unexpected error: {err}"
    );
}
