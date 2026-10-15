//! Unit tests for the console-mode guard.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, ENABLE_WINDOW_INPUT,
};

use crate::client::console_mode::ConsoleModeGuard;
use crate::utils::windows::MockWindowsApi;

const STDIN: usize = 0x11;
const STDOUT: usize = 0x22;

/// Expect exactly one `set_console_mode(handle, mode)` call.
fn expect_set_console_mode(api: &mut MockWindowsApi, handle: usize, mode: CONSOLE_MODE) {
    api.expect_set_console_mode()
        .withf(move |actual_handle, actual_mode| {
            return actual_handle.0 as usize == handle && *actual_mode == mode;
        })
        .times(1)
        .returning(|_, _| return Ok(()));
}

#[test]
fn test_applies_raw_input_and_vt_output_then_restores() {
    let stdin_original = ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT;
    let stdout_original = CONSOLE_MODE(0);
    let mut api = MockWindowsApi::new();
    api.expect_get_std_handle()
        .returning(|_| return Ok(HANDLE(STDIN as *mut _)));
    api.expect_get_stdout_handle()
        .returning(|| return Ok(HANDLE(STDOUT as *mut _)));
    api.expect_get_console_mode().returning(move |handle| {
        if handle.0 as usize == STDIN {
            return Ok(stdin_original);
        }
        return Ok(stdout_original);
    });
    expect_set_console_mode(&mut api, STDIN, ENABLE_WINDOW_INPUT);
    expect_set_console_mode(
        &mut api,
        STDOUT,
        ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    );
    expect_set_console_mode(&mut api, STDIN, stdin_original);
    expect_set_console_mode(&mut api, STDOUT, stdout_original);

    drop(ConsoleModeGuard::apply(&api));
}

#[test]
fn test_restores_only_successfully_changed_modes() {
    let mut api = MockWindowsApi::new();
    api.expect_get_std_handle()
        .returning(|_| return Err(windows::core::Error::from_thread()));
    api.expect_get_stdout_handle()
        .returning(|| return Ok(HANDLE(STDOUT as *mut _)));
    api.expect_get_console_mode()
        .returning(|_| return Ok(CONSOLE_MODE(0)));
    api.expect_set_console_mode()
        .times(1)
        .returning(|_, _| return Err(windows::core::Error::from_thread()));

    drop(ConsoleModeGuard::apply(&api));
}
