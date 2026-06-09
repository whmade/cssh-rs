//! Tests for the worktree_teardown module.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use log::Level;
use mockall::mock;

use crate::worktree_teardown::{worktree_teardown, WorktreeTeardownSystem};

mock! {
    WorktreeTeardownSystemMock {}
    impl WorktreeTeardownSystem for WorktreeTeardownSystemMock {
        fn env_var(&self, key: &str) -> Option<String>;
        fn run_git(&self, repo_path: &Path, args: Vec<String>) -> anyhow::Result<i32>;
    }
}

/// Wire `env_var` to return values from a static map, defaulting to
/// `None` for unknown keys.
fn expect_env(
    mock: &mut MockWorktreeTeardownSystemMock,
    worktree: Option<&str>,
    source: Option<&str>,
    branch: Option<&str>,
) {
    let worktree = worktree.map(str::to_owned);
    let source = source.map(str::to_owned);
    let branch = branch.map(str::to_owned);
    mock.expect_env_var().returning(move |key| match key {
        "PASEO_WORKTREE_PATH" => worktree.clone(),
        "PASEO_SOURCE_CHECKOUT_PATH" => source.clone(),
        "PASEO_BRANCH_NAME" => branch.clone(),
        _ => None,
    });
}

#[test]
fn test_full_env_runs_both_git_commands() {
    // Arrange
    testing_logger::setup();
    let mut mock = MockWorktreeTeardownSystemMock::new();
    expect_env(
        &mut mock,
        Some("/tmp/worktree"),
        Some("/tmp/source"),
        Some("feature/x"),
    );

    type GitCall = (PathBuf, Vec<String>);
    let calls: Arc<Mutex<Vec<GitCall>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_clone = calls.clone();
    mock.expect_run_git().returning(move |path, args| {
        calls_clone.lock().unwrap().push((path.to_path_buf(), args));
        Ok(0)
    });

    // Act
    let result = worktree_teardown(&mock);

    // Assert
    assert!(result.is_ok());
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, PathBuf::from("/tmp/worktree"));
    assert_eq!(
        calls[0].1,
        vec!["checkout".to_owned(), "--detach".to_owned()]
    );
    assert_eq!(calls[1].0, PathBuf::from("/tmp/source"));
    assert_eq!(
        calls[1].1,
        vec!["branch".to_owned(), "-D".to_owned(), "feature/x".to_owned()]
    );
    testing_logger::validate(|logs| {
        assert_eq!(logs.len(), 0, "no log messages expected on the happy path");
    });
}

#[test]
fn test_nonzero_exit_logs_and_continues() {
    // Arrange
    testing_logger::setup();
    let mut mock = MockWorktreeTeardownSystemMock::new();
    expect_env(
        &mut mock,
        Some("/tmp/worktree"),
        Some("/tmp/source"),
        Some("feature/x"),
    );

    let invocations = Arc::new(Mutex::new(0usize));
    let invocations_clone = invocations.clone();
    mock.expect_run_git().returning(move |_, _| {
        let mut n = invocations_clone.lock().unwrap();
        *n += 1;
        Ok(1)
    });

    // Act
    let result = worktree_teardown(&mock);

    // Assert
    assert!(result.is_ok());
    assert_eq!(*invocations.lock().unwrap(), 2);
    testing_logger::validate(|logs| {
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].level, Level::Warn);
        assert!(logs[0].body.contains("checkout --detach"));
        assert!(logs[0].body.contains("code 1"));
        assert_eq!(logs[1].level, Level::Warn);
        assert!(logs[1].body.contains("branch -D feature/x"));
        assert!(logs[1].body.contains("code 1"));
    });
}

#[test]
fn test_missing_worktree_path_skips_detach() {
    // Arrange
    testing_logger::setup();
    let mut mock = MockWorktreeTeardownSystemMock::new();
    expect_env(&mut mock, None, Some("/tmp/source"), Some("feature/x"));

    let calls = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
    let calls_clone = calls.clone();
    mock.expect_run_git().returning(move |path, _| {
        calls_clone.lock().unwrap().push(path.to_path_buf());
        Ok(0)
    });

    // Act
    let result = worktree_teardown(&mock);

    // Assert
    assert!(result.is_ok());
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], PathBuf::from("/tmp/source"));
    testing_logger::validate(|logs| {
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, Level::Info);
        assert!(logs[0].body.contains("PASEO_WORKTREE_PATH not set"));
    });
}

#[test]
fn test_missing_branch_or_source_skips_branch_delete() {
    // Arrange
    testing_logger::setup();
    let mut mock = MockWorktreeTeardownSystemMock::new();
    expect_env(&mut mock, Some("/tmp/worktree"), Some("/tmp/source"), None);

    let calls = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let calls_clone = calls.clone();
    mock.expect_run_git().returning(move |_, args| {
        calls_clone.lock().unwrap().push(args);
        Ok(0)
    });

    // Act
    let result = worktree_teardown(&mock);

    // Assert
    assert!(result.is_ok());
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], vec!["checkout".to_owned(), "--detach".to_owned()]);
    testing_logger::validate(|logs| {
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, Level::Info);
        assert!(logs[0]
            .body
            .contains("PASEO_SOURCE_CHECKOUT_PATH or PASEO_BRANCH_NAME not set"));
    });
}

#[test]
fn test_run_git_spawn_failure_logs_and_continues() {
    // Arrange
    testing_logger::setup();
    let mut mock = MockWorktreeTeardownSystemMock::new();
    expect_env(
        &mut mock,
        Some("/tmp/worktree"),
        Some("/tmp/source"),
        Some("feature/x"),
    );

    let invocations = Arc::new(Mutex::new(0usize));
    let invocations_clone = invocations.clone();
    mock.expect_run_git().returning(move |_, _| {
        *invocations_clone.lock().unwrap() += 1;
        Err(anyhow::anyhow!("git binary missing"))
    });

    // Act
    let result = worktree_teardown(&mock);

    // Assert
    assert!(result.is_ok());
    assert_eq!(*invocations.lock().unwrap(), 2);
    testing_logger::validate(|logs| {
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].level, Level::Warn);
        assert!(logs[0].body.contains("checkout --detach"));
        assert!(logs[0].body.contains("git binary missing"));
        assert_eq!(logs[1].level, Level::Warn);
        assert!(logs[1].body.contains("branch -D feature/x"));
        assert!(logs[1].body.contains("git binary missing"));
    });
}
