//! Unit tests for the DSR responder.

use std::sync::Mutex;

use crate::client::pty::scan_and_answer_dsr;

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
