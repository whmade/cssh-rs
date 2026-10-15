//! Application of daemon-to-client messages.

use std::io::Write;
use std::ops::ControlFlow;
use std::sync::Mutex;

use log::{info, warn};
use tokio::sync::watch;
use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};

use cssh_rs_protocol::v1::input::{ClientRunState, SignalKind};
use cssh_rs_protocol::v1::message::DaemonToClient;
use cssh_rs_protocol::ClientState;

use crate::client::input_bytes::input_event_to_bytes;
use crate::client::pty::write_to_master;
use crate::utils::windows::WindowsApi;

/// Apply one daemon-to-client message.
///
/// # Arguments
///
/// * `api`              - The Windows API implementation to use.
/// * `message`          - The decoded message.
/// * `master`           - PTY master writer.
/// * `child_pid`        - Process id of the SSH child, the signal target.
/// * `state_sender`     - Watch sender for the client state.
/// * `highlight_sender` - Watch sender for the highlight flag.
///
/// # Returns
///
/// [`ControlFlow::Break`] when the daemon requested termination.
pub(crate) fn dispatch_daemon_message(
    api: &dyn WindowsApi,
    message: DaemonToClient,
    master: &Mutex<impl Write>,
    child_pid: u32,
    state_sender: &watch::Sender<ClientState>,
    highlight_sender: &watch::Sender<bool>,
) -> ControlFlow<()> {
    match message {
        DaemonToClient::Input { event } => write_to_master(master, &input_event_to_bytes(&event)),
        DaemonToClient::Signal(kind) => {
            let ctrl_event = match kind {
                SignalKind::Interrupt => CTRL_C_EVENT,
                SignalKind::Break => CTRL_BREAK_EVENT,
                SignalKind::Unknown => return ControlFlow::Continue(()),
            };
            if let Err(err) = api.generate_console_ctrl_event(ctrl_event, child_pid) {
                warn!("Failed to relay console control event: {}", err);
            }
        }
        DaemonToClient::StateChange { state } => {
            state_sender.send_replace(match state {
                ClientRunState::Disabled => ClientState::Disabled,
                ClientRunState::Active | ClientRunState::Unknown => ClientState::Active,
            });
        }
        DaemonToClient::Highlight { on } => {
            highlight_sender.send_replace(on);
        }
        DaemonToClient::Terminate { reason } => {
            info!("Daemon requested termination: {}", reason);
            return ControlFlow::Break(());
        }
        DaemonToClient::Activate { .. } | DaemonToClient::KeepAlive | DaemonToClient::Unknown => {}
    }
    return ControlFlow::Continue(());
}

#[cfg(test)]
#[path = "../tests/client/test_dispatch.rs"]
mod tests;
