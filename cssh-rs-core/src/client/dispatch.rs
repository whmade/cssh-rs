//! Pure dispatch of decoded daemon-to-client messages onto the PTY master and
//! the client's visual-state channels.
//!
//! Kept free of any concrete PTY so it can be unit-tested with a `Vec<u8>`
//! sink and a `MockWindowsApi`.

use std::io::Write;
use std::sync::Mutex;

use log::{error, info, warn};
use tokio::sync::watch;
use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};

use cssh_rs_protocol::v1::input::{ClientRunState, SignalKind};
use cssh_rs_protocol::v1::message::DaemonToClient;

use crate::client::input_bytes::input_event_to_bytes;
use crate::client::ClientState;
use crate::utils::windows::WindowsApi;

/// Outcome of dispatching one message: whether the client keeps running.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Dispatch {
    /// Keep the client running.
    Continue,
    /// The daemon asked the client to terminate.
    Terminate,
}

/// Apply one daemon-to-client message.
///
/// Input events are encoded to terminal bytes and written to `master`; signals
/// are relayed to the SSH child's process group; state and highlight changes
/// are published on the watch channels; `Terminate` ends the run.
///
/// # Arguments
///
/// * `api`              - The Windows API implementation to use.
/// * `message`          - The decoded daemon-to-client message.
/// * `master`           - Shared PTY master writer all input converges on.
/// * `child_pid`        - Pid of the SSH child, used as the signal target.
/// * `state_sender`     - Watch sender for the authoritative visual state.
/// * `highlight_sender` - Watch sender for the highlight flag.
///
/// # Returns
///
/// [`Dispatch::Terminate`] when the daemon requested shutdown, else
/// [`Dispatch::Continue`].
pub(crate) fn dispatch_daemon_message(
    api: &dyn WindowsApi,
    message: DaemonToClient,
    master: &Mutex<Box<dyn Write + Send>>,
    child_pid: u32,
    state_sender: &watch::Sender<ClientState>,
    highlight_sender: &watch::Sender<bool>,
) -> Dispatch {
    match message {
        DaemonToClient::Input { event } => {
            let bytes = input_event_to_bytes(&event);
            if !bytes.is_empty() {
                write_to_master(master, &bytes);
            }
        }
        DaemonToClient::Signal(kind) => {
            let ctrl_event = match kind {
                SignalKind::Interrupt => CTRL_C_EVENT,
                SignalKind::Break => CTRL_BREAK_EVENT,
                SignalKind::Unknown => return Dispatch::Continue,
            };
            if let Err(err) = api.generate_console_ctrl_event(ctrl_event, child_pid) {
                warn!("Failed to relay console control event: {}", err);
            }
        }
        DaemonToClient::StateChange { state } => {
            state_sender.send_replace(run_state_to_client_state(state));
        }
        DaemonToClient::Highlight { on } => {
            highlight_sender.send_replace(on);
        }
        DaemonToClient::Terminate { reason } => {
            info!("Daemon requested termination: {}", reason);
            return Dispatch::Terminate;
        }
        DaemonToClient::Activate { .. } | DaemonToClient::KeepAlive | DaemonToClient::Unknown => {}
    }
    return Dispatch::Continue;
}

/// Map the wire run-state onto the client's two-state visual model. An
/// `Unknown` state (added in a later protocol minor version) is treated as
/// `Active` so input keeps flowing.
fn run_state_to_client_state(state: ClientRunState) -> ClientState {
    match state {
        ClientRunState::Active | ClientRunState::Unknown => return ClientState::Active,
        ClientRunState::Disabled => return ClientState::Disabled,
    }
}

/// Write `bytes` to the shared master, logging (never panicking) on failure.
fn write_to_master(master: &Mutex<Box<dyn Write + Send>>, bytes: &[u8]) {
    match master.lock() {
        Ok(mut writer) => {
            if let Err(err) = writer.write_all(bytes) {
                error!("Failed to write to PTY master: {}", err);
            }
            let _ = writer.flush();
        }
        Err(err) => error!("PTY master mutex poisoned: {}", err),
    }
}

#[cfg(test)]
#[path = "../tests/client/test_dispatch.rs"]
mod tests;
