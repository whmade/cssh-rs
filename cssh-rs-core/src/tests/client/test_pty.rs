//! Unit tests for the DSR responder and PTY sizing.

use std::sync::Mutex;

use windows::Win32::System::Console::{CONSOLE_SCREEN_BUFFER_INFOEX, SMALL_RECT};

use crate::client::pty::{console_viewport_size, scan_and_answer_dsr};
use crate::utils::windows::MockWindowsApi;

#[test]
fn test_answers_and_strips_dsr_query() {
    let master = Mutex::new(Vec::new());
    let mut carry = Vec::new();

    let rendered = scan_and_answer_dsr(b"hello\x1b[6nworld", &mut carry, &master);

    assert_eq!(rendered, b"helloworld");
    assert_eq!(master.into_inner().unwrap(), b"\x1b[1;1R");
    assert!(carry.is_empty());
}

#[test]
fn test_answers_query_split_across_reads() {
    let master = Mutex::new(Vec::new());
    let mut carry = Vec::new();

    let first = scan_and_answer_dsr(b"abc\x1b[", &mut carry, &master);
    let second = scan_and_answer_dsr(b"6ndef", &mut carry, &master);

    assert_eq!(first, b"abc");
    assert_eq!(second, b"def");
    assert_eq!(master.into_inner().unwrap(), b"\x1b[1;1R");
    assert!(carry.is_empty());
}

#[test]
fn test_renders_held_prefix_that_is_not_a_query() {
    let master = Mutex::new(Vec::new());
    let mut carry = Vec::new();

    let first = scan_and_answer_dsr(b"x\x1b[", &mut carry, &master);
    let second = scan_and_answer_dsr(b"A", &mut carry, &master);

    assert_eq!(first, b"x");
    assert_eq!(second, b"\x1b[A");
    assert!(master.into_inner().unwrap().is_empty());
    assert!(carry.is_empty());
}

#[test]
fn test_console_viewport_size_is_the_inclusive_window_rectangle() {
    let mut api = MockWindowsApi::new();
    api.expect_get_console_screen_buffer_info_ex()
        .returning(|| {
            return Ok(CONSOLE_SCREEN_BUFFER_INFOEX {
                srWindow: SMALL_RECT {
                    Left: 10,
                    Top: 186,
                    Right: 89,
                    Bottom: 202,
                },
                ..Default::default()
            });
        });

    let size = console_viewport_size(&api).unwrap();

    assert_eq!((size.cols, size.rows), (80, 17));
}
