//! ConPTY seam: open a pseudo-terminal, run `ssh` on the slave, and expose the
//! master reader/writer/child. The concrete `portable-pty` usage here is the
//! only part of the client not covered by unit tests (it spawns a real PTY);
//! the DSR responder it relies on is pure and is tested.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

/// The conhost startup Device-Status-Report query (cursor position request).
const DSR_QUERY: &[u8] = b"\x1b[6n";
/// The reply that unblocks the child: cursor at row 1, column 1.
const DSR_REPLY: &[u8] = b"\x1b[1;1R";

/// A shared PTY master writer that daemon-delivered bytes, local keystrokes,
/// and DSR replies all converge on.
pub(crate) type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// A shared PTY master handle used to resize the pseudo-terminal when the
/// console window changes size.
pub(crate) type SharedMaster = Arc<Mutex<Box<dyn MasterPty + Send>>>;

/// The pieces of a spawned client PTY the run loop drives.
pub(crate) struct ClientPty {
    /// Shared writer feeding the PTY master.
    pub(crate) writer: SharedWriter,
    /// Reader draining the PTY master (child output).
    pub(crate) reader: Box<dyn Read + Send>,
    /// The spawned SSH child.
    pub(crate) child: Box<dyn Child + Send + Sync>,
    /// Pid of the SSH child, used as the signal target and reported to the
    /// daemon in `Ready`.
    pub(crate) child_pid: u32,
    /// The master end; shared so the input thread can resize the PTY, and held
    /// so the PTY is not torn down early.
    pub(crate) master: SharedMaster,
}

/// Open a PTY, run `program args` on the slave, and return its handles.
///
/// # Arguments
///
/// * `program` - The SSH (or configured) program to run.
/// * `args`    - Arguments passed to `program`.
/// * `rows`    - Initial PTY row count.
/// * `cols`    - Initial PTY column count.
///
/// # Returns
///
/// The spawned [`ClientPty`], or the I/O error that prevented spawning.
#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) fn spawn_client_pty(
    program: &str,
    args: &[String],
    rows: u16,
    cols: u16,
) -> io::Result<ClientPty> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| return io::Error::other(err.to_string()))?;

    let mut cmd = CommandBuilder::new(program);
    for arg in args {
        cmd.arg(arg);
    }
    inherit_env(&mut cmd);

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|err| return io::Error::other(err.to_string()))?;
    // A missing pid must fail the spawn: pid 0 would target the whole console
    // group in GenerateConsoleCtrlEvent and misreport the child in `Ready`.
    let child_pid = child
        .process_id()
        .ok_or_else(|| return io::Error::other("spawned PTY child has no process id"))?;
    // Close our handle to the slave so the child owns the only one; otherwise
    // the master read never sees EOF when the child exits.
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
        master: Arc::new(Mutex::new(pair.master)),
    });
}

/// Resize the PTY master to `cols` x `rows`.
///
/// A poisoned lock or a failed resize is ignored: the child simply keeps its
/// previous size until the next resize event rather than aborting the session.
///
/// # Arguments
///
/// * `master` - Shared PTY master handle.
/// * `cols`   - New column count.
/// * `rows`   - New row count.
#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) fn resize_pty(master: &SharedMaster, cols: u16, rows: u16) {
    if let Ok(master) = master.lock() {
        let _ = master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }
}

/// Copy the current process environment into `cmd`.
///
/// `portable-pty`'s `CommandBuilder` starts from an empty environment, but a
/// Windows process needs at least `SystemRoot` and `PATH` to initialize.
#[cfg_attr(coverage_nightly, coverage(off))]
fn inherit_env(cmd: &mut CommandBuilder) {
    for (key, value) in std::env::vars() {
        cmd.env(key, value);
    }
}

/// Answer any conhost DSR cursor-position queries in `chunk` and return the
/// bytes that should be rendered to the visible console.
///
/// conhost emits `ESC [ 6 n` at startup and blocks the child until it receives
/// `ESC [ 1 ; 1 R`, so whoever holds the master must reply. The query itself is
/// stripped from the rendered output so this window does not echo it back into
/// its own input. `carry` holds a query that was split across reads.
///
/// # Arguments
///
/// * `chunk`  - Newly read bytes from the PTY master.
/// * `carry`  - Partial-query bytes held from the previous call; updated here.
/// * `master` - Shared writer used to send the DSR reply.
///
/// # Returns
///
/// The bytes to render (with any DSR query removed).
pub(crate) fn scan_and_answer_dsr(
    chunk: &[u8],
    carry: &mut Vec<u8>,
    master: &Mutex<Box<dyn Write + Send>>,
) -> Vec<u8> {
    let mut buf = std::mem::take(carry);
    buf.extend_from_slice(chunk);

    let mut out = Vec::with_capacity(buf.len());
    let mut i = 0;
    while i < buf.len() {
        let rest = &buf[i..];
        if rest.starts_with(DSR_QUERY) {
            write_all(master, DSR_REPLY);
            i += DSR_QUERY.len();
            continue;
        }
        // A trailing strict prefix of the query may be the start of a query
        // split across reads; hold it back rather than render it.
        if rest.len() < DSR_QUERY.len() && DSR_QUERY.starts_with(rest) {
            *carry = rest.to_vec();
            return out;
        }
        out.push(buf[i]);
        i += 1;
    }
    return out;
}

/// Write `bytes` to the shared master, ignoring a poisoned lock.
fn write_all(master: &Mutex<Box<dyn Write + Send>>, bytes: &[u8]) {
    if let Ok(mut writer) = master.lock() {
        let _ = writer.write_all(bytes);
        let _ = writer.flush();
    }
}

#[cfg(test)]
#[path = "../tests/client/test_pty.rs"]
mod tests;
