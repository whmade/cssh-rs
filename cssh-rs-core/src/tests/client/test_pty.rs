//! Unit tests for the pure DSR responder. The `spawn_client_pty` seam spawns a
//! real PTY and is exercised only by the live end-to-end run, not here.

use std::io::Write;
use std::sync::{Arc, Mutex};

use crate::client::pty::scan_and_answer_dsr;

/// A `Write` sink recording into a shared buffer.
struct SharedSink(Arc<Mutex<Vec<u8>>>);

impl Write for SharedSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("sink").extend_from_slice(buf);
        return Ok(buf.len());
    }

    fn flush(&mut self) -> std::io::Result<()> {
        return Ok(());
    }
}

#[allow(clippy::type_complexity)]
fn sink() -> (Mutex<Box<dyn Write + Send>>, Arc<Mutex<Vec<u8>>>) {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let writer: Box<dyn Write + Send> = Box::new(SharedSink(Arc::clone(&captured)));
    return (Mutex::new(writer), captured);
}

#[test]
fn answers_dsr_and_strips_it_from_render() {
    let (master, replied) = sink();
    let mut carry = Vec::new();
    let chunk = b"hello\x1b[6nworld";

    let rendered = scan_and_answer_dsr(chunk, &mut carry, &master);

    assert_eq!(rendered, b"helloworld");
    assert_eq!(&*replied.lock().expect("replied"), b"\x1b[1;1R");
    assert!(carry.is_empty());
}

#[test]
fn passes_through_when_no_query_present() {
    let (master, replied) = sink();
    let mut carry = Vec::new();
    let chunk = b"just some output\r\n";

    let rendered = scan_and_answer_dsr(chunk, &mut carry, &master);

    assert_eq!(rendered, chunk);
    assert!(replied.lock().expect("replied").is_empty());
    assert!(carry.is_empty());
}

#[test]
fn handles_query_split_across_two_reads() {
    let (master, replied) = sink();
    let mut carry = Vec::new();

    // First read ends mid-query: "abc" + ESC "[" (a strict prefix of the DSR).
    let first = scan_and_answer_dsr(b"abc\x1b[", &mut carry, &master);
    assert_eq!(first, b"abc");
    assert_eq!(carry, b"\x1b[");
    assert!(replied.lock().expect("replied").is_empty());

    // Second read completes the query: "6n" + trailing output.
    let second = scan_and_answer_dsr(b"6ndef", &mut carry, &master);
    assert_eq!(second, b"def");
    assert_eq!(&*replied.lock().expect("replied"), b"\x1b[1;1R");
    assert!(carry.is_empty());
}

#[test]
fn carried_prefix_that_is_not_a_query_is_rendered() {
    let (master, replied) = sink();
    let mut carry = Vec::new();

    // "ESC [" held back...
    let first = scan_and_answer_dsr(b"x\x1b[", &mut carry, &master);
    assert_eq!(first, b"x");
    assert_eq!(carry, b"\x1b[");

    // ...then "A" arrives: it is a cursor-up sequence, not a DSR, so the held
    // bytes render normally and nothing is replied.
    let second = scan_and_answer_dsr(b"A", &mut carry, &master);
    assert_eq!(second, b"\x1b[A");
    assert!(replied.lock().expect("replied").is_empty());
    assert!(carry.is_empty());
}
