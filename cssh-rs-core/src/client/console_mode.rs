//! RAII guard for the console modes the ConPTY client needs.

use std::ffi::c_void;

use log::warn;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, ENABLE_WINDOW_INPUT,
    STD_INPUT_HANDLE,
};

use crate::utils::windows::WindowsApi;

/// Restores the console modes it changed when dropped.
pub(crate) struct ConsoleModeGuard<'a> {
    api: &'a dyn WindowsApi,
    /// Handles and their original modes; handles are kept as `isize` because
    /// `HANDLE` is not `Send`.
    original_modes: Vec<(isize, CONSOLE_MODE)>,
}

impl<'a> ConsoleModeGuard<'a> {
    /// Switch stdin to raw input and stdout to VT processing.
    ///
    /// Raw input leaves echo to the SSH child and delivers Ctrl+C as `0x03`;
    /// window input reports console resizes.
    ///
    /// # Arguments
    ///
    /// * `api` - The Windows API implementation to use.
    ///
    /// # Returns
    ///
    /// A guard restoring the original modes.
    pub(crate) fn apply(api: &'a dyn WindowsApi) -> Self {
        let stdin = api.get_std_handle(STD_INPUT_HANDLE).and_then(|handle| {
            return change_mode(api, handle, |mode| {
                return mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT)
                    | ENABLE_WINDOW_INPUT;
            });
        });
        let stdout = api.get_stdout_handle().and_then(|handle| {
            return change_mode(api, handle, |mode| {
                return mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
            });
        });
        let original_modes = [stdin, stdout]
            .into_iter()
            .filter_map(|result| {
                return result
                    .inspect_err(|err| warn!("Failed to change console mode: {}", err))
                    .ok();
            })
            .collect();
        return Self {
            api,
            original_modes,
        };
    }
}

impl Drop for ConsoleModeGuard<'_> {
    fn drop(&mut self) {
        for (handle, mode) in &self.original_modes {
            let _ = self
                .api
                .set_console_mode(HANDLE(*handle as *mut c_void), *mode);
        }
    }
}

/// Apply `change` to the console mode of `handle`.
///
/// # Returns
///
/// The handle and its original mode.
fn change_mode(
    api: &dyn WindowsApi,
    handle: HANDLE,
    change: impl FnOnce(CONSOLE_MODE) -> CONSOLE_MODE,
) -> windows::core::Result<(isize, CONSOLE_MODE)> {
    let original_mode = api.get_console_mode(handle)?;
    api.set_console_mode(handle, change(original_mode))?;
    return Ok((handle.0 as isize, original_mode));
}

#[cfg(test)]
#[path = "../tests/client/test_console_mode.rs"]
mod tests;
