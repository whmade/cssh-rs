//! Unit tests for the console-mode RAII guard, driven with a `MockWindowsApi`
//! so no real console handle is touched.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_INPUT_HANDLE,
};

use crate::client::console_mode::ConsoleModeGuard;
use crate::utils::windows::MockWindowsApi;

const STDIN_BITS: usize = 0x11;
const STDOUT_BITS: usize = 0x22;

#[test]
fn applies_raw_and_vt_then_restores_on_drop() {
    let stdin_original = ENABLE_LINE_INPUT.0 | ENABLE_ECHO_INPUT.0 | ENABLE_PROCESSED_INPUT.0;
    let stdout_original: u32 = 0;
    let stdout_vt = ENABLE_PROCESSED_OUTPUT.0 | ENABLE_VIRTUAL_TERMINAL_PROCESSING.0;

    let mut api = MockWindowsApi::new();
    api.expect_get_std_handle()
        .withf(|h| return *h == STD_INPUT_HANDLE)
        .times(1)
        .returning(|_| return Ok(HANDLE(STDIN_BITS as *mut _)));
    api.expect_get_stdout_handle()
        .times(1)
        .returning(|| return Ok(HANDLE(STDOUT_BITS as *mut _)));
    api.expect_get_console_mode()
        .times(2)
        .returning(move |handle| {
            if handle.0 as usize == STDIN_BITS {
                return Ok(CONSOLE_MODE(stdin_original));
            }
            return Ok(CONSOLE_MODE(stdout_original));
        });

    // Apply: stdin cleared to raw (0), stdout gains the VT flags.
    api.expect_set_console_mode()
        .withf(move |handle, mode| return handle.0 as usize == STDIN_BITS && mode.0 == 0)
        .times(1)
        .returning(|_, _| return Ok(()));
    api.expect_set_console_mode()
        .withf(move |handle, mode| return handle.0 as usize == STDOUT_BITS && mode.0 == stdout_vt)
        .times(1)
        .returning(|_, _| return Ok(()));
    // Restore on drop: originals written back.
    api.expect_set_console_mode()
        .withf(move |handle, mode| {
            return handle.0 as usize == STDIN_BITS && mode.0 == stdin_original;
        })
        .times(1)
        .returning(|_, _| return Ok(()));
    api.expect_set_console_mode()
        .withf(move |handle, mode| {
            return handle.0 as usize == STDOUT_BITS && mode.0 == stdout_original;
        })
        .times(1)
        .returning(|_, _| return Ok(()));

    {
        let _guard = ConsoleModeGuard::apply(&api);
    }
}

#[test]
fn handle_lookup_failure_leaves_console_untouched() {
    let mut api = MockWindowsApi::new();
    api.expect_get_std_handle()
        .times(1)
        .returning(|_| return Err(windows::core::Error::from_thread()));
    api.expect_get_stdout_handle()
        .times(1)
        .returning(|| return Err(windows::core::Error::from_thread()));
    // Neither stream was reconfigured, so drop must not call set_console_mode.
    api.expect_get_console_mode().never();
    api.expect_set_console_mode().never();

    {
        let _guard = ConsoleModeGuard::apply(&api);
    }
}

#[test]
fn mode_read_failure_leaves_console_untouched() {
    let mut api = MockWindowsApi::new();
    api.expect_get_std_handle()
        .times(1)
        .returning(|_| return Ok(HANDLE(STDIN_BITS as *mut _)));
    api.expect_get_stdout_handle()
        .times(1)
        .returning(|| return Ok(HANDLE(STDOUT_BITS as *mut _)));
    // Reading the current mode fails for both streams.
    api.expect_get_console_mode()
        .times(2)
        .returning(|_| return Err(windows::core::Error::from_thread()));
    api.expect_set_console_mode().never();

    {
        let _guard = ConsoleModeGuard::apply(&api);
    }
}

#[test]
fn mode_set_failure_leaves_console_untouched() {
    let mut api = MockWindowsApi::new();
    api.expect_get_std_handle()
        .times(1)
        .returning(|_| return Ok(HANDLE(STDIN_BITS as *mut _)));
    api.expect_get_stdout_handle()
        .times(1)
        .returning(|| return Ok(HANDLE(STDOUT_BITS as *mut _)));
    api.expect_get_console_mode()
        .times(2)
        .returning(|_| return Ok(CONSOLE_MODE(0)));
    // Applying raw/VT mode fails, so neither stream is recorded for restore and
    // drop performs no further set_console_mode calls.
    api.expect_set_console_mode()
        .times(2)
        .returning(|_, _| return Err(windows::core::Error::from_thread()));

    {
        let _guard = ConsoleModeGuard::apply(&api);
    }
}
