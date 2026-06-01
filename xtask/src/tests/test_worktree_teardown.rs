//! Tests for the worktree_teardown module.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mockall::mock;

use crate::worktree_teardown::{worktree_teardown, WorktreeTeardownSystem};

mock! {
    WorktreeTeardownSystemMock {}
    impl WorktreeTeardownSystem for WorktreeTeardownSystemMock {
        fn env_var(&self, key: &str) -> Option<String>;
        fn run_git(&self, repo_path: &Path, args: Vec<String>) -> anyhow::Result<i32>;
        fn log(&self, msg: &str);
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

    mock.expect_log().never();

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
}

#[test]
fn test_nonzero_exit_logs_and_continues() {
    // Arrange
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

    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let logs_clone = logs.clone();
    mock.expect_log().returning(move |msg| {
        logs_clone.lock().unwrap().push(msg.to_owned());
    });

    // Act
    let result = worktree_teardown(&mock);

    // Assert
    assert!(result.is_ok());
    assert_eq!(*invocations.lock().unwrap(), 2);
    let logs = logs.lock().unwrap();
    assert_eq!(logs.len(), 2);
    assert!(logs[0].contains("checkout --detach"));
    assert!(logs[0].contains("code 1"));
    assert!(logs[1].contains("branch -D feature/x"));
    assert!(logs[1].contains("code 1"));
}

#[test]
fn test_missing_worktree_path_skips_detach() {
    // Arrange
    let mut mock = MockWorktreeTeardownSystemMock::new();
    expect_env(&mut mock, None, Some("/tmp/source"), Some("feature/x"));

    let calls = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
    let calls_clone = calls.clone();
    mock.expect_run_git().returning(move |path, _| {
        calls_clone.lock().unwrap().push(path.to_path_buf());
        Ok(0)
    });

    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let logs_clone = logs.clone();
    mock.expect_log().returning(move |msg| {
        logs_clone.lock().unwrap().push(msg.to_owned());
    });

    // Act
    let result = worktree_teardown(&mock);

    // Assert
    assert!(result.is_ok());
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], PathBuf::from("/tmp/source"));
    let logs = logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains("PASEO_WORKTREE_PATH not set"));
}

#[test]
fn test_missing_branch_or_source_skips_branch_delete() {
    // Arrange
    let mut mock = MockWorktreeTeardownSystemMock::new();
    expect_env(&mut mock, Some("/tmp/worktree"), Some("/tmp/source"), None);

    let calls = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let calls_clone = calls.clone();
    mock.expect_run_git().returning(move |_, args| {
        calls_clone.lock().unwrap().push(args);
        Ok(0)
    });

    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let logs_clone = logs.clone();
    mock.expect_log().returning(move |msg| {
        logs_clone.lock().unwrap().push(msg.to_owned());
    });

    // Act
    let result = worktree_teardown(&mock);

    // Assert
    assert!(result.is_ok());
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], vec!["checkout".to_owned(), "--detach".to_owned()]);
    let logs = logs.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains("PASEO_SOURCE_CHECKOUT_PATH or PASEO_BRANCH_NAME not set"));
}

#[test]
fn test_run_git_spawn_failure_propagates() {
    // Arrange
    let mut mock = MockWorktreeTeardownSystemMock::new();
    expect_env(
        &mut mock,
        Some("/tmp/worktree"),
        Some("/tmp/source"),
        Some("feature/x"),
    );
    mock.expect_run_git()
        .returning(|_, _| Err(anyhow::anyhow!("git binary missing")));

    // Act
    let result = worktree_teardown(&mock);

    // Assert
    assert!(result.is_err());
    assert!(format!("{:#}", result.unwrap_err()).contains("git binary missing"));
}
