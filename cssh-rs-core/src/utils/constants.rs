//! Shared constants.

/// Name of the package.
pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");
/// Name of the Pipe used for interprocess comunication between daemon and clients.
///
/// <https://learn.microsoft.com/en-us/windows/win32/ipc/pipe-names>
pub const PIPE_NAME: &str = concat!(r"\\.\pipe\", env!("CARGO_PKG_NAME"), "-named-pipe-for-ipc");
#[cfg(windows)]
pub use cssh_rs_platform_windows::MAX_WINDOW_TITLE_LENGTH;
