//! ConPTY that runs `ssh` and answers its startup cursor-position query.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, PoisonError};

use log::error;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

/// Device Status Report query conhost sends at startup, blocking until answered.
const DSR_QUERY: &[u8] = b"\x1b[6n";
/// Cursor position report answering [`DSR_QUERY`]: row 1, column 1.
const DSR_REPLY: &[u8] = b"\x1b[1;1R";

/// PTY master writer shared by every input source.
pub(crate) type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// A spawned SSH child and the PTY master it runs on.
pub(crate) struct ClientPty {
    pub(crate) writer: SharedWriter,
    pub(crate) reader: Box<dyn Read + Send>,
    pub(crate) child: Box<dyn Child + Send + Sync>,
    pub(crate) child_pid: u32,
    /// Keeps the pseudoconsole open.
    pub(crate) _master: Box<dyn MasterPty + Send>,
}

/// Run `program` with `arguments` on a new PTY.
///
/// # Arguments
///
/// * `program`   - The SSH program to run.
/// * `arguments` - Arguments passed to `program`.
/// * `rows`      - PTY row count.
/// * `columns`   - PTY column count.
///
/// # Returns
///
/// The spawned [`ClientPty`], or the error that prevented spawning it.
#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) fn spawn_client_pty(
    program: &str,
    arguments: &[String],
    rows: u16,
    columns: u16,
) -> io::Result<ClientPty> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols: columns,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| return io::Error::other(err.to_string()))?;

    let mut command = CommandBuilder::new(program);
    command.args(arguments);
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|err| return io::Error::other(err.to_string()))?;
    // Process group 0 would signal every process on this console.
    let child_pid = child
        .process_id()
        .ok_or_else(|| return io::Error::other("spawned PTY child has no process id"))?;
    // Otherwise reading the master never sees EOF once the child exits.
    drop(pair.slave);

    let writer = pair
        .master
        .take_writer()
        .map_err(|err| return io::Error::other(err.to_string()))?;
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|err| return io::Error::other(err.to_string()))?;

    return Ok(ClientPty {
        writer: Arc::new(Mutex::new(writer)),
        reader,
        child,
        child_pid,
        _master: pair.master,
    });
}

/// Answer and strip every [`DSR_QUERY`] in the PTY output.
///
/// # Arguments
///
/// * `chunk`  - Bytes newly read from the PTY master.
/// * `carry`  - A query prefix held back from the previous chunk; updated here.
/// * `master` - PTY master writer the reply is sent to.
///
/// # Returns
///
/// The bytes to render.
pub(crate) fn scan_and_answer_dsr(
    chunk: &[u8],
    carry: &mut Vec<u8>,
    master: &Mutex<impl Write>,
) -> Vec<u8> {
    let mut buffer = std::mem::take(carry);
    buffer.extend_from_slice(chunk);

    let mut rendered = Vec::with_capacity(buffer.len());
    let mut position = 0;
    while position < buffer.len() {
        let rest = &buffer[position..];
        if rest.starts_with(DSR_QUERY) {
            write_to_master(master, DSR_REPLY);
            position += DSR_QUERY.len();
            continue;
        }
        if rest.len() < DSR_QUERY.len() && DSR_QUERY.starts_with(rest) {
            *carry = rest.to_vec();
            break;
        }
        rendered.push(buffer[position]);
        position += 1;
    }
    return rendered;
}

/// Write `bytes` to the PTY master, logging a failure.
///
/// # Arguments
///
/// * `master` - PTY master writer.
/// * `bytes`  - Bytes to write.
pub(crate) fn write_to_master(master: &Mutex<impl Write>, bytes: &[u8]) {
    let mut writer = master.lock().unwrap_or_else(PoisonError::into_inner);
    if let Err(err) = writer.write_all(bytes).and_then(|()| return writer.flush()) {
        error!("Failed to write to the PTY master: {}", err);
    }
}

#[cfg(test)]
#[path = "../tests/client/test_pty.rs"]
mod tests;
