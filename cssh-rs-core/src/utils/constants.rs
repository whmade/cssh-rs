//! Shared constants.

/// Name of the Pipe used for interprocess communication between daemon and clients.
///
/// <https://learn.microsoft.com/en-us/windows/win32/ipc/pipe-names>
pub const PIPE_NAME: &str = r"\\.\pipe\cssh-rs-named-pipe-for-ipc";
#[cfg(windows)]
pub use cssh_rs_platform_windows::MAX_WINDOW_TITLE_LENGTH;
