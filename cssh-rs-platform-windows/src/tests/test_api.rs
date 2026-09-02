//! Unit tests for the utils windows module using mockall for Windows API mocking.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]

use crate::api::{
    clear_screen, is_windows_10, read_console_input, read_keyboard_input, set_console_border_color,
    set_console_color, set_console_palette, snapshot_console_palette, tinted_palette,
    utf16_buffer_to_string, ConsolePaletteSnapshot, MockWindowsApi, KEY_EVENT,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::System::Console::{
    CONSOLE_CHARACTER_ATTRIBUTES, CONSOLE_SCREEN_BUFFER_INFO, CONSOLE_SCREEN_BUFFER_INFOEX, COORD,
    INPUT_RECORD, INPUT_RECORD_0, MOUSE_EVENT,
};

/// Tests Windows version detection.
mod version_detection_test {
    use super::*;

    /// Tests that Windows 8.1 is correctly classified as "Windows 10 or older".
    /// Validates version parsing for major versions less than 10.
    #[test]
    fn test_is_windows_10_with_windows_8_version() {
        let mut mock_api = MockWindowsApi::new();
        mock_api
            .expect_get_os_version()
            .times(1)
            .return_const("6.3.9600".to_string());

        let result = is_windows_10(&mock_api);
        assert!(
            result,
            "Should detect Windows 6.3.9600 as Windows 10 or older (major <= 10)"
        );
    }

    /// Tests that future Windows versions are correctly classified as newer than Windows 10.
    /// Validates detection of Windows 11+ versions with major > 10.
    #[test]
    fn test_is_windows_10_with_future_version() {
        let mut mock_api = MockWindowsApi::new();
        mock_api
            .expect_get_os_version()
            .times(1)
            .return_const("11.0.25000".to_string());

        let result = is_windows_10(&mock_api);
        assert!(
            !result,
            "Should detect Windows 11.0.25000 as newer than Windows 10"
        );
    }

    /// Tests Windows 10/11 boundary detection at build 22000.
    /// Validates that build 21999 is Windows 10 and 22000+ is Windows 11.
    #[test]
    fn test_is_windows_10_boundary_cases() {
        let test_cases = vec![
            ("10.0.21999", true),
            ("10.0.22000", false),
            ("10.0.19045", true),
            ("10.0.17763", true),
        ];

        for (version, expected) in test_cases {
            let mut mock_api = MockWindowsApi::new();
            mock_api
                .expect_get_os_version()
                .times(1)
                .return_const(version.to_string());

            let result = is_windows_10(&mock_api);
            assert_eq!(
                result, expected,
                "Version {version} should return {expected}"
            );
        }
    }

    /// Tests that malformed version strings cause the function to panic.
    /// Validates error handling for unparseable version input.
    #[test]
    fn test_is_windows_10_with_malformed_version() {
        let mut mock_api = MockWindowsApi::new();
        mock_api
            .expect_get_os_version()
            .times(1)
            .return_const("invalid.version.string".to_string());

        let result = std::panic::catch_unwind(|| {
            return is_windows_10(&mock_api);
        });
        assert!(
            result.is_err(),
            "Should panic with malformed version string"
        );
    }
}

/// Tests UTF-16 buffer conversion functionality.
mod utf16_conversion_test {
    use super::*;

    /// Tests basic UTF-16 to string conversion with null termination.
    /// Validates standard ASCII string handling.
    #[test]
    fn test_utf16_buffer_to_string_basic() {
        let test_string = "Hello World";
        let utf16_buffer: Vec<u16> = test_string
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let result = utf16_buffer_to_string(&utf16_buffer);
        assert_eq!(result, test_string);
    }

    /// Tests UTF-16 to string conversion with Unicode characters.
    /// Validates proper handling of international characters and emojis.
    #[test]
    fn test_utf16_buffer_to_string_unicode() {
        let test_string = "Test 🦀 Rust 中文 Тест";
        let utf16_buffer: Vec<u16> = test_string
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let result = utf16_buffer_to_string(&utf16_buffer);
        assert_eq!(result, test_string);
    }

    /// Tests UTF-16 to string conversion with empty buffer.
    /// Validates handling of null-only buffers.
    #[test]
    fn test_utf16_buffer_to_string_empty() {
        let utf16_buffer: Vec<u16> = vec![0];

        let result = utf16_buffer_to_string(&utf16_buffer);
        assert_eq!(result, "");
    }

    /// Tests UTF-16 to string conversion without null termination.
    /// Validates handling of buffers that lack proper null terminators.
    #[test]
    fn test_utf16_buffer_to_string_no_null_terminator() {
        let test_string = "No Null";
        let utf16_buffer: Vec<u16> = test_string.encode_utf16().collect();

        let result = utf16_buffer_to_string(&utf16_buffer);
        assert_eq!(result, test_string);
    }

    /// Tests UTF-16 to string conversion with multiple null terminators.
    /// Validates that only the first null terminator is respected.
    #[test]
    fn test_utf16_buffer_to_string_multiple_nulls() {
        let test_string = "Hello";
        let mut utf16_buffer: Vec<u16> = test_string.encode_utf16().collect();
        utf16_buffer.extend_from_slice(&[0, 0, 0]);

        let result = utf16_buffer_to_string(&utf16_buffer);
        assert_eq!(result, test_string);
    }
}

/// Test module for console color functions with proper mocking.
mod console_color_test {
    use super::*;

    /// Tests console color setting with text attributes and buffer filling.
    /// Validates proper color application across the entire console buffer
    /// and the post-fill invalidate that works around the conhost
    /// stale-row/column bug.
    #[test]
    fn test_set_console_color() {
        let mut mock_api = MockWindowsApi::new();
        let test_color = CONSOLE_CHARACTER_ATTRIBUTES(0x0F);

        let mut buffer_info = CONSOLE_SCREEN_BUFFER_INFO::default();
        buffer_info.dwSize.X = 80;
        buffer_info.dwSize.Y = 25;

        mock_api
            .expect_set_console_text_attribute()
            .with(mockall::predicate::eq(test_color))
            .times(1)
            .returning(|_| return Ok(()));

        mock_api
            .expect_get_console_screen_buffer_info()
            .times(1)
            .return_const(Ok(buffer_info));

        // Single call covering the entire buffer (width * height cells
        // from (0,0)) - FillConsoleOutputAttribute auto-spans rows.
        mock_api
            .expect_fill_console_output_attribute()
            .with(
                mockall::predicate::eq(test_color.0),
                mockall::predicate::eq(80u32 * 25u32),
                mockall::predicate::eq(COORD { X: 0, Y: 0 }),
            )
            .times(1)
            .returning(|_, _, _| return Ok(80 * 25));

        mock_api
            .expect_invalidate_console_window()
            .times(1)
            .returning(|| return Ok(()));

        set_console_color(&mock_api, test_color);
    }

    /// Tests that a failing invalidate is logged and does not propagate -
    /// a stale visual is recoverable; panicking would kill the SSH
    /// session for a non-critical visual cue.
    #[test]
    fn test_set_console_color_swallows_invalidate_error() {
        let mut mock_api = MockWindowsApi::new();
        let test_color = CONSOLE_CHARACTER_ATTRIBUTES(0x0F);

        let mut buffer_info = CONSOLE_SCREEN_BUFFER_INFO::default();
        buffer_info.dwSize.X = 80;
        buffer_info.dwSize.Y = 25;

        mock_api
            .expect_set_console_text_attribute()
            .times(1)
            .returning(|_| return Ok(()));
        mock_api
            .expect_get_console_screen_buffer_info()
            .times(1)
            .return_const(Ok(buffer_info));
        mock_api
            .expect_fill_console_output_attribute()
            .times(1)
            .returning(|_, _, _| return Ok(80 * 25));
        mock_api
            .expect_invalidate_console_window()
            .times(1)
            .returning(|| return Err(windows::core::Error::from_thread()));

        set_console_color(&mock_api, test_color);
    }

    /// Tests console color setting error handling when API calls fail.
    /// Validates that function panics appropriately on Windows API errors.
    #[test]
    fn test_set_console_color_error_handling() {
        let mut mock_api = MockWindowsApi::new();
        let test_color = CONSOLE_CHARACTER_ATTRIBUTES(0x0F);

        mock_api
            .expect_set_console_text_attribute()
            .with(mockall::predicate::eq(test_color))
            .times(1)
            .returning(|_| return Err(windows::core::Error::from_thread()));

        let result = std::panic::catch_unwind(|| {
            set_console_color(&mock_api, test_color);
        });

        assert!(
            result.is_err(),
            "Should panic when set_console_text_attribute fails"
        );
    }
}

/// Test module for the non-destructive palette snapshot/tint/restore helpers.
mod console_palette_test {
    use super::*;

    /// Build a `CONSOLE_SCREEN_BUFFER_INFOEX` carrying `palette`.
    fn info_with_palette(palette: [COLORREF; 16]) -> CONSOLE_SCREEN_BUFFER_INFOEX {
        return CONSOLE_SCREEN_BUFFER_INFOEX {
            ColorTable: palette,
            ..Default::default()
        };
    }

    /// A palette with 16 distinct entries so remaps are observable.
    fn sample_palette() -> [COLORREF; 16] {
        let mut palette = [COLORREF(0); 16];
        for (index, entry) in palette.iter_mut().enumerate() {
            *entry = COLORREF(0x0000_1000 * (index as u32 + 1));
        }
        return palette;
    }

    /// Build a snapshot from a color table and default attribute.
    fn snapshot_of(color_table: [COLORREF; 16], attributes: u16) -> ConsolePaletteSnapshot {
        return ConsolePaletteSnapshot {
            color_table,
            default_attributes: CONSOLE_CHARACTER_ATTRIBUTES(attributes),
        };
    }

    /// Snapshotting returns the console's color table and default attribute.
    #[test]
    fn test_snapshot_console_palette_returns_table_and_attributes() {
        let mut mock = MockWindowsApi::new();
        let palette = sample_palette();
        mock.expect_get_console_screen_buffer_info_ex()
            .times(1)
            .returning(move || {
                return Ok(CONSOLE_SCREEN_BUFFER_INFOEX {
                    ColorTable: palette,
                    wAttributes: CONSOLE_CHARACTER_ATTRIBUTES(0x08),
                    ..Default::default()
                });
            });

        assert_eq!(
            snapshot_console_palette(&mock),
            Some(snapshot_of(palette, 0x08))
        );
    }

    /// A buffer-info failure degrades the snapshot to `None` rather than panicking.
    #[test]
    fn test_snapshot_console_palette_error_returns_none() {
        let mut mock = MockWindowsApi::new();
        mock.expect_get_console_screen_buffer_info_ex()
            .times(1)
            .returning(|| return Err(windows::core::Error::from_thread()));

        assert_eq!(snapshot_console_palette(&mock), None);
    }

    /// Two default themes, so the remap cannot be hardcoded to a fixed slot.
    #[test]
    fn test_tinted_palette_remaps_default_entries_by_nibble() {
        let table = sample_palette();
        let test_cases = [
            ("dark, default 0/8", 0x08u16, 0x1Fu16, 8, 15, 0, 1),
            ("light, default 1/7", 0x71u16, 0x40u16, 1, 0, 7, 4),
        ];
        for (description, default_attribute, tint, fg_dst, fg_src, bg_dst, bg_src) in test_cases {
            let tinted = tinted_palette(
                &snapshot_of(table, default_attribute),
                CONSOLE_CHARACTER_ATTRIBUTES(tint),
            );
            assert_eq!(
                tinted[fg_dst], table[fg_src],
                "{description}: default text takes the foreground nibble color"
            );
            assert_eq!(
                tinted[bg_dst], table[bg_src],
                "{description}: default background takes the background nibble color"
            );
            for (index, entry) in tinted.iter().enumerate() {
                if index == fg_dst || index == bg_dst {
                    continue;
                }
                assert_eq!(
                    *entry, table[index],
                    "{description}: untouched entry {index} changed"
                );
            }
        }
    }

    #[test]
    fn test_set_console_palette_writes_color_table_widens_window_and_repaints() {
        let base = sample_palette();
        let mut current = base;
        current[0] = COLORREF(0x00AB_CDEF);
        let mut info = info_with_palette(current);
        info.srWindow.Right = 79;
        info.srWindow.Bottom = 24;
        let mut mock = MockWindowsApi::new();
        mock.expect_get_console_screen_buffer_info_ex()
            .times(1)
            .returning(move || return Ok(info));
        mock.expect_set_console_screen_buffer_info_ex()
            .times(1)
            .returning(move |info| {
                assert_eq!(info.ColorTable, base);
                assert_eq!(info.srWindow.Right, 80);
                assert_eq!(info.srWindow.Bottom, 25);
                return Ok(());
            });
        mock.expect_invalidate_console_window()
            .times(1)
            .returning(|| return Ok(()));

        set_console_palette(&mock, &base);
    }

    /// A read failure before recolor is swallowed: no set call is attempted.
    #[test]
    fn test_set_console_palette_read_failure_skips_set() {
        let base = sample_palette();
        let mut mock = MockWindowsApi::new();
        mock.expect_get_console_screen_buffer_info_ex()
            .times(1)
            .returning(|| return Err(windows::core::Error::from_thread()));
        // No expect_set_console_screen_buffer_info_ex: the mock panics if called.

        set_console_palette(&mock, &base);
    }
}

/// Test module for clear screen functions with proper mocking.
mod clear_screen_test {
    use super::*;

    /// Tests console screen clearing with scroll buffer operations.
    /// Validates proper screen clearing and cursor positioning to origin.
    #[test]
    fn test_clear_screen() {
        let mut mock_api = MockWindowsApi::new();

        let mut buffer_info = CONSOLE_SCREEN_BUFFER_INFO::default();
        buffer_info.dwSize.X = 80;
        buffer_info.dwSize.Y = 25;
        buffer_info.wAttributes = CONSOLE_CHARACTER_ATTRIBUTES(0x07);

        mock_api
            .expect_get_console_screen_buffer_info()
            .times(1)
            .return_const(Ok(buffer_info));

        mock_api
            .expect_scroll_console_screen_buffer()
            .times(1)
            .returning(|_, _, _| return Ok(()));

        mock_api
            .expect_set_console_cursor_position()
            .with(mockall::predicate::eq(COORD { X: 0, Y: 0 }))
            .times(1)
            .returning(|_| return Ok(()));

        clear_screen(&mock_api);
    }

    /// Tests clear screen error handling when buffer info retrieval fails.
    /// Validates that function panics appropriately on Windows API errors.
    #[test]
    fn test_clear_screen_error_handling() {
        let mut mock_api = MockWindowsApi::new();

        mock_api
            .expect_get_console_screen_buffer_info()
            .times(1)
            .returning(|| return Err(windows::core::Error::from_thread()));

        let result = std::panic::catch_unwind(|| {
            clear_screen(&mock_api);
        });

        assert!(
            result.is_err(),
            "Should panic when get_console_screen_buffer_info fails"
        );
    }
}

/// Test module for console border color functions with proper mocking.
mod console_border_color_test {
    use super::*;

    /// Tests console border color setting on Windows 10 (no-op behavior).
    /// Validates that function skips DWM calls on Windows 10 systems.
    #[test]
    fn test_set_console_border_color_windows_10() {
        let mut api = MockWindowsApi::new();
        let test_color = COLORREF(0x00FF0000);

        api.expect_get_os_version()
            .times(1)
            .return_const("10.0.19045".to_string());

        api.expect_set_console_border_color()
            .with(mockall::predicate::eq(test_color))
            .times(0);

        set_console_border_color(&api, test_color);
    }

    /// Tests console border color setting on Windows 11 with DWM integration.
    /// Validates that function properly calls DWM APIs on Windows 11+ systems.
    #[test]
    fn test_set_console_border_color_windows_11() {
        let mut api = MockWindowsApi::new();
        let test_color = COLORREF(0x00FF0000);

        api.expect_get_os_version()
            .times(1)
            .return_const("10.0.22000".to_string());

        api.expect_set_console_border_color()
            .with(mockall::predicate::eq(test_color))
            .times(1)
            .returning(|_| return Ok(()));

        set_console_border_color(&api, test_color);
    }

    /// Tests console border color setting error handling when DWM calls fail.
    /// Validates that function panics appropriately on DWM API errors.
    #[test]
    fn test_set_console_border_color_error_handling() {
        let mut api = MockWindowsApi::new();
        let test_color = COLORREF(0x00FF0000);

        api.expect_get_os_version()
            .times(1)
            .return_const("10.0.22000".to_string());

        api.expect_set_console_border_color()
            .with(mockall::predicate::eq(test_color))
            .times(1)
            .returning(|_| return Err(windows::core::Error::from_thread()));

        let result = std::panic::catch_unwind(|| {
            set_console_border_color(&api, test_color);
        });

        assert!(
            result.is_err(),
            "Should panic when set_console_border_color fails"
        );
    }
}

/// Test module for console input functions with proper mocking.
mod console_input_test {
    use windows::Win32::System::Console::KEY_EVENT_RECORD;

    use super::*;

    /// Tests basic console input reading with single event retrieval.
    /// Validates proper input record handling and event type detection.
    #[test]
    fn test_read_console_input() {
        let mut mock_api = MockWindowsApi::new();

        let test_record = INPUT_RECORD {
            EventType: KEY_EVENT,
            ..Default::default()
        };

        mock_api
            .expect_read_console_input()
            .with(mockall::predicate::always())
            .times(1)
            .returning(move |buffer: &mut [INPUT_RECORD]| {
                buffer[0] = test_record;
                return Ok(1);
            });

        let result = read_console_input(&mock_api);
        assert_eq!(result.EventType, KEY_EVENT);
    }

    /// Tests console input reading with retry logic when no events are available.
    /// Validates that function retries until an event is successfully retrieved.
    #[test]
    fn test_read_console_input_retry() {
        let mut mock_api = MockWindowsApi::new();

        let test_record = INPUT_RECORD {
            EventType: KEY_EVENT,
            ..Default::default()
        };

        let mut call_count = 0;
        mock_api
            .expect_read_console_input()
            .with(mockall::predicate::always())
            .times(2)
            .returning(move |buffer: &mut [INPUT_RECORD]| {
                call_count += 1;
                if call_count == 1 {
                    return Ok(0);
                } else {
                    buffer[0] = test_record;
                    return Ok(1);
                }
            });

        let result = read_console_input(&mock_api);
        assert_eq!(result.EventType, KEY_EVENT);
    }

    /// Tests keyboard input filtering with event type detection and field validation.
    /// Validates that function filters out non-key events and returns complete key data.
    #[test]
    fn test_read_keyboard_input() {
        let mut mock_api = MockWindowsApi::new();

        let non_key_record = INPUT_RECORD {
            EventType: MOUSE_EVENT as u16,
            ..Default::default()
        };
        let mut key_event_record = KEY_EVENT_RECORD {
            bKeyDown: windows::core::BOOL(1),
            wRepeatCount: 1,
            wVirtualKeyCode: 0x41,
            wVirtualScanCode: 0x1E,
            dwControlKeyState: 0,
            ..Default::default()
        };
        key_event_record.uChar.UnicodeChar = 'A' as u16;

        let key_event_data = INPUT_RECORD_0 {
            KeyEvent: key_event_record,
        };
        let key_record = INPUT_RECORD {
            EventType: KEY_EVENT,
            Event: key_event_data,
        };

        let mut call_count = 0;
        mock_api
            .expect_read_console_input()
            .with(mockall::predicate::always())
            .times(2)
            .returning(move |buffer: &mut [INPUT_RECORD]| {
                call_count += 1;
                if call_count == 1 {
                    buffer[0] = non_key_record;
                } else {
                    buffer[0] = key_record;
                }
                return Ok(1);
            });

        let result = read_keyboard_input(&mock_api);

        let returned_key_event = unsafe { result.KeyEvent };
        assert_eq!(returned_key_event.bKeyDown, key_event_record.bKeyDown);
        assert_eq!(
            returned_key_event.wRepeatCount,
            key_event_record.wRepeatCount
        );
        assert_eq!(
            returned_key_event.wVirtualKeyCode,
            key_event_record.wVirtualKeyCode
        );
        assert_eq!(
            returned_key_event.wVirtualScanCode,
            key_event_record.wVirtualScanCode
        );
        assert_eq!(unsafe { returned_key_event.uChar.UnicodeChar }, unsafe {
            key_event_record.uChar.UnicodeChar
        });
        assert_eq!(
            returned_key_event.dwControlKeyState,
            key_event_record.dwControlKeyState
        );
    }

    /// Tests console input reading error handling when API calls fail.
    /// Validates that function panics appropriately on Windows API errors.
    #[test]
    fn test_read_console_input_error_handling() {
        let mut mock_api = MockWindowsApi::new();

        mock_api
            .expect_read_console_input()
            .with(mockall::predicate::always())
            .times(1)
            .returning(|_| return Err(windows::core::Error::from_thread()));

        let result = std::panic::catch_unwind(|| {
            read_console_input(&mock_api);
        });

        assert!(
            result.is_err(),
            "Should panic when read_console_input fails"
        );
    }
}

/// Test module for command line building functionality.
mod command_line_test {
    use crate::api::{build_command_line, build_command_line_wide, encode_wide_z};
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::OsStringExt;

    /// `build_command_line_wide` quotes the application and each argument
    /// and terminates with a NUL - mirroring the UTF-8 `build_command_line`
    /// output for plain ASCII inputs.
    #[test]
    fn test_build_command_line_wide_simple() {
        let application = OsString::from("cmd.exe");
        let args = vec![OsString::from("arg1"), OsString::from("arg2")];

        let result = build_command_line_wide(&application, &args);

        assert_eq!(
            result,
            vec![
                34, 99, 109, 100, 46, 101, 120, 101, 34, 32, 34, 97, 114, 103, 49, 34, 32, 34, 97,
                114, 103, 50, 34, 0
            ]
        );
    }

    /// `build_command_line_wide` still quotes the application and terminates
    /// with a NUL when no arguments are supplied.
    #[test]
    fn test_build_command_line_wide_no_args() {
        let application = OsString::from("notepad.exe");
        let args: Vec<OsString> = vec![];

        let result = build_command_line_wide(&application, &args);

        assert_eq!(
            result,
            vec![34, 110, 111, 116, 101, 112, 97, 100, 46, 101, 120, 101, 34, 0]
        );
    }

    /// `build_command_line_wide` passes non-UTF-8 byte sequences (lone
    /// surrogates) through unchanged, instead of replacing them with
    /// `U+FFFD` the way the UTF-8 path would.
    #[test]
    fn test_build_command_line_wide_preserves_lone_surrogate() {
        let application = OsString::from("a.exe");
        let surrogate_arg = OsString::from_wide(&[0xD800u16, b'a' as u16]);
        let args = vec![surrogate_arg];

        let result = build_command_line_wide(&application, &args);

        // "a.exe" quoted, space, quote, 0xD800, 'a', quote, NUL.
        assert_eq!(
            result,
            vec![34, 97, 46, 101, 120, 101, 34, 32, 34, 0xD800, 97, 34, 0]
        );
    }

    /// `encode_wide_z` returns the UTF-16 encoding of its input followed by
    /// a single NUL terminator.
    #[test]
    fn test_encode_wide_z_appends_nul_terminator() {
        let encoded = encode_wide_z(OsStr::new("abc"));
        assert_eq!(encoded, vec![97, 98, 99, 0]);
    }

    /// `encode_wide_z` returns just the NUL terminator for an empty input.
    #[test]
    fn test_encode_wide_z_empty_input() {
        let encoded = encode_wide_z(OsStr::new(""));
        assert_eq!(encoded, vec![0]);
    }

    /// Tests build_command_line with simple application and arguments.
    /// Validates proper UTF-16 encoding and quoting.
    #[test]
    fn test_build_command_line_simple() {
        let application = "cmd.exe";
        let args = vec!["arg1".to_string(), "arg2".to_string()];

        let result = build_command_line(application, &args);

        // Also make sure its null terminated
        assert_eq!(
            result,
            vec![
                34, 99, 109, 100, 46, 101, 120, 101, 34, 32, 34, 97, 114, 103, 49, 34, 32, 34, 97,
                114, 103, 50, 34, 0
            ]
        );
    }

    /// Tests build_command_line with no arguments.
    /// Validates proper handling of applications without arguments.
    #[test]
    fn test_build_command_line_no_args() {
        let application = "notepad.exe";
        let args: Vec<String> = vec![];

        let result = build_command_line(application, &args);

        assert_eq!(
            result,
            vec![34, 110, 111, 116, 101, 112, 97, 100, 46, 101, 120, 101, 34, 0]
        );
    }

    /// Tests build_command_line with arguments containing spaces.
    /// Validates proper quoting of complex arguments.
    #[test]
    fn test_build_command_line_spaces() {
        let application = "program.exe";
        let args = vec!["arg with spaces".to_string(), "another arg".to_string()];

        let result = build_command_line(application, &args);

        assert_eq!(
            result,
            vec![
                34, 112, 114, 111, 103, 114, 97, 109, 46, 101, 120, 101, 34, 32, 34, 97, 114, 103,
                32, 119, 105, 116, 104, 32, 115, 112, 97, 99, 101, 115, 34, 32, 34, 97, 110, 111,
                116, 104, 101, 114, 32, 97, 114, 103, 34, 0
            ]
        );
    }
}

mod create_process_with_args_test {
    use windows::Win32::{
        Foundation::{GetLastError, STILL_ACTIVE},
        System::Threading::{TerminateProcess, STARTF_USESHOWWINDOW},
        UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
    };

    use crate::api::{build_startupinfo, DefaultWindowsApi, WindowsApi};

    /// Tests create_process_with_args with valid application and arguments.
    /// Validates that the process creation function is called with correct parameters.
    /// Note: This test actually creates a process.
    #[test]
    fn test_create_process_with_args() {
        let windows_api = DefaultWindowsApi;
        let application = r"C:\Windows\System32\timeout.exe";
        let args = vec!["30".to_string()];
        let process_info = match windows_api.create_process_with_args(application, args, true) {
            None => panic!("Failed to create process: {:?}", unsafe { GetLastError() }),
            Some(process_info) => process_info,
        };
        assert!(windows_api.get_exit_code(process_info.hProcess).unwrap() == STILL_ACTIVE.0 as u32);
        unsafe { TerminateProcess(process_info.hProcess, 0) }.expect("Failed to terminate process");
        assert!(windows_api.get_exit_code(process_info.hProcess).unwrap() == 0);
    }

    /// Tests that build_startupinfo populates STARTF_USESHOWWINDOW and
    /// SW_SHOWNOACTIVATE when keyboard focus is suppressed, so the spawned
    /// console window appears without stealing foreground focus from the daemon.
    #[test]
    fn test_build_startupinfo_without_focus_sets_show_no_activate() {
        let startupinfo = build_startupinfo(false);
        assert!(
            (startupinfo.dwFlags & STARTF_USESHOWWINDOW) == STARTF_USESHOWWINDOW,
            "STARTF_USESHOWWINDOW must be set when keyboard focus is suppressed"
        );
        assert_eq!(startupinfo.wShowWindow, SW_SHOWNOACTIVATE.0 as u16);
    }

    /// Tests that build_startupinfo leaves STARTF_USESHOWWINDOW unset when
    /// keyboard focus is allowed, so the spawned process picks its own
    /// show-window behaviour.
    #[test]
    fn test_build_startupinfo_with_focus_leaves_flags_default() {
        let startupinfo = build_startupinfo(true);
        assert!(
            (startupinfo.dwFlags & STARTF_USESHOWWINDOW) != STARTF_USESHOWWINDOW,
            "STARTF_USESHOWWINDOW must not be set when keyboard focus is allowed"
        );
        assert_eq!(startupinfo.wShowWindow, 0);
    }
}
