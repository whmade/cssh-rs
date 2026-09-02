//! Real-console integration tests for the palette helpers, catching real-Windows
//! behaviour the mocked unit tests cannot.
#![cfg(windows)]
#![allow(clippy::needless_return)]

use cssh_rs_platform_windows::{
    set_console_palette, snapshot_console_palette, tinted_palette, DefaultWindowsApi,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Console::{
    AllocConsole, CreateConsoleScreenBuffer, GetConsoleMode, GetStdHandle,
    ReadConsoleOutputAttribute, SetConsoleActiveScreenBuffer, SetConsoleCursorPosition,
    SetConsoleMode, SetStdHandle, WriteConsoleOutputAttribute, WriteConsoleOutputCharacterW,
    WriteConsoleW, CONSOLE_CHARACTER_ATTRIBUTES, CONSOLE_MODE, CONSOLE_TEXTMODE_BUFFER, COORD,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_OUTPUT_HANDLE,
};

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;

/// A private console screen buffer wired to `STD_OUTPUT_HANDLE` for the test,
/// restoring the previous handle on drop.
struct ScopedScreenBuffer {
    buffer: HANDLE,
    previous_stdout: HANDLE,
}

impl ScopedScreenBuffer {
    fn new() -> Self {
        // CreateConsoleScreenBuffer needs the process to own a console; ignore
        // the error when one is already attached.
        let _ = unsafe { AllocConsole() };
        let buffer = unsafe {
            CreateConsoleScreenBuffer(
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                CONSOLE_TEXTMODE_BUFFER,
                None,
            )
        }
        .expect("CreateConsoleScreenBuffer failed");
        let previous_stdout =
            unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.expect("GetStdHandle failed");
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, buffer) }.expect("SetStdHandle failed");
        return Self {
            buffer,
            previous_stdout,
        };
    }

    /// Read `len` per-cell attributes from `(0, row)`.
    fn read_attributes(&self, row: i16, len: usize) -> Vec<u16> {
        let mut attributes = vec![0u16; len];
        let mut read = 0u32;
        unsafe {
            ReadConsoleOutputAttribute(
                self.buffer,
                &mut attributes,
                COORD { X: 0, Y: row },
                &mut read,
            )
        }
        .expect("ReadConsoleOutputAttribute failed");
        attributes.truncate(read as usize);
        return attributes;
    }

    /// Enable VT processing on the buffer so `write_vt` interprets escapes.
    fn enable_vt(&self) {
        let mut mode = CONSOLE_MODE(0);
        unsafe { GetConsoleMode(self.buffer, &mut mode) }.expect("GetConsoleMode failed");
        unsafe {
            SetConsoleMode(
                self.buffer,
                mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
            )
        }
        .expect("SetConsoleMode failed");
    }

    /// Write `text` (may contain VT escapes) at `(0, row)` via `WriteConsoleW`,
    /// the path a real shell/prompt uses.
    fn write_vt(&self, row: i16, text: &str) {
        unsafe { SetConsoleCursorPosition(self.buffer, COORD { X: 0, Y: row }) }
            .expect("SetConsoleCursorPosition failed");
        let chars: Vec<u16> = text.encode_utf16().collect();
        let mut written = 0u32;
        unsafe { WriteConsoleW(self.buffer, &chars, Some(&mut written), None) }
            .expect("WriteConsoleW failed");
    }

    /// Write `text` with `attribute` starting at `(0, row)`.
    fn write_run(&self, row: i16, text: &str, attribute: u16) {
        let chars: Vec<u16> = text.encode_utf16().collect();
        let mut written = 0u32;
        unsafe {
            WriteConsoleOutputCharacterW(self.buffer, &chars, COORD { X: 0, Y: row }, &mut written)
        }
        .expect("WriteConsoleOutputCharacterW failed");
        let attributes = vec![attribute; chars.len()];
        let mut attr_written = 0u32;
        unsafe {
            WriteConsoleOutputAttribute(
                self.buffer,
                &attributes,
                COORD { X: 0, Y: row },
                &mut attr_written,
            )
        }
        .expect("WriteConsoleOutputAttribute failed");
    }
}

impl Drop for ScopedScreenBuffer {
    fn drop(&mut self) {
        // Reactivate the original buffer before closing ours: a test that called
        // SetConsoleActiveScreenBuffer(self.buffer) would otherwise leave an
        // interactive console showing (then holding a closed handle to) our
        // private buffer.
        let _ = unsafe { SetConsoleActiveScreenBuffer(self.previous_stdout) };
        let _ = unsafe { SetStdHandle(STD_OUTPUT_HANDLE, self.previous_stdout) };
        let _ = unsafe { CloseHandle(self.buffer) };
    }
}

#[test]
fn test_palette_tint_leaves_cell_attributes_untouched() {
    let screen = ScopedScreenBuffer::new();
    screen.enable_vt();
    // Legacy green text on row 0, truecolor magenta blocks on row 1.
    screen.write_run(0, "LEGACY>", 0x0A);
    screen.write_vt(
        1,
        &format!("\x1b[38;2;255;0;255m{}\x1b[0m", "\u{2588}".repeat(7)),
    );

    let legacy_before = screen.read_attributes(0, 7);
    let truecolor_before = screen.read_attributes(1, 7);

    let api = DefaultWindowsApi;
    let base = snapshot_console_palette(&api).expect("snapshot palette");

    assert!(
        set_console_palette(
            &api,
            &tinted_palette(&base, CONSOLE_CHARACTER_ATTRIBUTES(0x1F)),
        ),
        "applying the tinted palette must succeed"
    );
    assert_eq!(
        screen.read_attributes(0, 7),
        legacy_before,
        "palette tint must not change legacy cell attributes"
    );
    assert_eq!(
        screen.read_attributes(1, 7),
        truecolor_before,
        "palette tint must not change truecolor cell attributes"
    );

    set_console_palette(&api, &base.color_table);
    assert_eq!(
        snapshot_console_palette(&api),
        Some(base),
        "restoring must bring back the exact original palette"
    );
}
