//! RAII guard that puts this window's console into the raw, VT-rendering mode
//! the ConPTY client needs, and restores the previous modes on drop.

use std::ffi::c_void;

use log::warn;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, ENABLE_WINDOW_INPUT,
    STD_INPUT_HANDLE,
};

use crate::utils::windows::WindowsApi;

/// Restores the console input/output modes captured at construction when
/// dropped.
///
/// The saved handles are kept as raw `isize` bits (not `HANDLE`, which wraps a
/// `!Send` pointer) so the guard stays `Send` across the client's `await`
/// points; they are reconstructed into a `HANDLE` only in `Drop`.
pub(crate) struct ConsoleModeGuard<'a> {
    /// The Windows API implementation used to restore modes.
    api: &'a dyn WindowsApi,
    /// Saved `(handle_bits, original_mode_bits)` for stdin, if reconfigured.
    stdin: Option<(isize, u32)>,
    /// Saved `(handle_bits, original_mode_bits)` for stdout, if reconfigured.
    stdout: Option<(isize, u32)>,
}

impl<'a> ConsoleModeGuard<'a> {
    /// Enter raw stdin and VT-processing stdout, capturing the prior modes.
    ///
    /// stdin drops `ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT |
    /// ENABLE_PROCESSED_INPUT` so local keystrokes are delivered raw (the SSH
    /// child echoes through the PTY, so local echo would double-print, and a
    /// locally typed Ctrl+C must arrive as a `0x03` byte, not a signal) and adds
    /// `ENABLE_WINDOW_INPUT` so `ReadConsoleInput` reports window-size changes
    /// that resize the PTY. stdout gains `ENABLE_PROCESSED_OUTPUT |
    /// ENABLE_VIRTUAL_TERMINAL_PROCESSING` so the child's escape sequences render.
    ///
    /// # Arguments
    ///
    /// * `api` - The Windows API implementation to use.
    ///
    /// # Returns
    ///
    /// A guard that restores the captured modes when dropped.
    pub(crate) fn apply(api: &'a dyn WindowsApi) -> Self {
        let stdin = configure_stdin(api);
        let stdout = configure_stdout(api);
        return Self { api, stdin, stdout };
    }
}

impl Drop for ConsoleModeGuard<'_> {
    fn drop(&mut self) {
        if let Some((bits, mode)) = self.stdin {
            let _ = self
                .api
                .set_console_mode(HANDLE(bits as *mut c_void), CONSOLE_MODE(mode));
        }
        if let Some((bits, mode)) = self.stdout {
            let _ = self
                .api
                .set_console_mode(HANDLE(bits as *mut c_void), CONSOLE_MODE(mode));
        }
    }
}

/// Switch stdin to raw mode, returning the handle and original mode to restore.
fn configure_stdin(api: &dyn WindowsApi) -> Option<(isize, u32)> {
    let handle = match api.get_std_handle(STD_INPUT_HANDLE) {
        Ok(handle) => handle,
        Err(err) => {
            warn!("Failed to get stdin handle; console stays cooked: {}", err);
            return None;
        }
    };
    let original = match api.get_console_mode(handle) {
        Ok(mode) => mode,
        Err(err) => {
            warn!("Failed to read stdin console mode: {}", err);
            return None;
        }
    };
    let raw = CONSOLE_MODE(
        (original.0 & !(ENABLE_LINE_INPUT.0 | ENABLE_ECHO_INPUT.0 | ENABLE_PROCESSED_INPUT.0))
            | ENABLE_WINDOW_INPUT.0,
    );
    if let Err(err) = api.set_console_mode(handle, raw) {
        warn!("Failed to set stdin to raw mode: {}", err);
        return None;
    }
    return Some((handle.0 as isize, original.0));
}

/// Enable VT processing on stdout, returning the handle and original mode.
fn configure_stdout(api: &dyn WindowsApi) -> Option<(isize, u32)> {
    let handle = match api.get_stdout_handle() {
        Ok(handle) => handle,
        Err(err) => {
            warn!("Failed to get stdout handle; VT output disabled: {}", err);
            return None;
        }
    };
    let original = match api.get_console_mode(handle) {
        Ok(mode) => mode,
        Err(err) => {
            warn!("Failed to read stdout console mode: {}", err);
            return None;
        }
    };
    let vt =
        CONSOLE_MODE(original.0 | ENABLE_PROCESSED_OUTPUT.0 | ENABLE_VIRTUAL_TERMINAL_PROCESSING.0);
    if let Err(err) = api.set_console_mode(handle, vt) {
        warn!("Failed to enable VT processing on stdout: {}", err);
        return None;
    }
    return Some((handle.0 as isize, original.0));
}

#[cfg(test)]
#[path = "../tests/client/test_console_mode.rs"]
mod tests;
