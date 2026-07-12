//! Tests for the record_demo module.

use std::path::{Path, PathBuf};

use mockall::mock;
use mockall::predicate::eq;
use mockall::Sequence;

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
    let output_dir = workspace_root().join("target").join("demo");
    let mut install_order = Sequence::new();
    let mut mock = MockDemoSystemMock::new();
    mock.expect_is_windows().returning(|| true);
    mock.expect_workspace_root()
        .returning(|| Ok(workspace_root()));
    mock.expect_build_binary()
        .times(1)
        .returning(|| Ok(binary_path()));
    // The demo package imports the automation library, so it must install second.
    mock.expect_pip_install()
        .times(1)
        .in_sequence(&mut install_order)
        .with(eq(workspace_root().join("automation")))
        .returning(|_| Ok(()));
    mock.expect_pip_install()
        .times(1)
        .in_sequence(&mut install_order)
        .with(eq(workspace_root().join("demo")))
        .returning(|_| Ok(()));
    mock.expect_run_demo()
        .times(1)
        .with(
            eq(binary_path()),
            eq(output_dir.clone()),
            eq(output_dir.join("cssh-rs.gif")),
        )
        .returning(|_, _, _| Ok(()));
    mock.expect_path_exists().times(1).returning(|_| true);

    record_demo(&mock).expect("record_demo should succeed");
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
