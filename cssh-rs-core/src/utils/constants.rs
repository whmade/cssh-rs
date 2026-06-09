//! Shared constants.

/// Name of the Pipe used for interprocess comunication between daemon and clients.
///
/// `concat!` accepts string literals only, so the binary-name component cannot
/// be substituted from [`cssh_rs_meta::PACKAGE_NAME`] at compile time. The
/// `PIPE_NAME.contains(PACKAGE_NAME)` assertion in
/// `tests/utils/test_constants.rs` guards against the two strings drifting
/// apart.
///
/// <https://learn.microsoft.com/en-us/windows/win32/ipc/pipe-names>
pub const PIPE_NAME: &str = r"\\.\pipe\cssh-rs-named-pipe-for-ipc";
#[cfg(windows)]
pub use cssh_rs_platform_windows::MAX_WINDOW_TITLE_LENGTH;
