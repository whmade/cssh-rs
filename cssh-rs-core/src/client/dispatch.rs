//! Application of daemon-to-client messages.

use std::io::Write;
use std::ops::ControlFlow;
use std::sync::Mutex;

use log::info;
use tokio::sync::watch;

use cssh_rs_protocol::v1::input::{ClientRunState, SignalKind};
use cssh_rs_protocol::v1::message::DaemonToClient;
use cssh_rs_protocol::ClientState;

use crate::client::input_bytes::{ctrl_break_keystroke, input_event_to_bytes, CTRL_C_KEYSTROKE};
use crate::client::pty::write_to_master;

/// Apply one daemon-to-client message.
///
/// # Arguments
///
/// * `message`          - The decoded message.
/// * `master`           - PTY master writer.
/// * `state_sender`     - Watch sender for the client state.
/// * `highlight_sender` - Watch sender for the highlight flag.
///
/// # Returns
///
/// [`ControlFlow::Break`] when the daemon requested termination.
pub(crate) fn dispatch_daemon_message(
    message: DaemonToClient,
    master: &Mutex<impl Write>,
    state_sender: &watch::Sender<ClientState>,
    highlight_sender: &watch::Sender<bool>,
) -> ControlFlow<()> {
    match message {
        DaemonToClient::Input { event } => write_to_master(master, &input_event_to_bytes(&event)),
        DaemonToClient::Signal(SignalKind::Break) => {
            write_to_master(master, &ctrl_break_keystroke())
        }
        DaemonToClient::Signal(SignalKind::Interrupt) => write_to_master(master, CTRL_C_KEYSTROKE),
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
        DaemonToClient::Signal(SignalKind::Unknown)
        | DaemonToClient::Activate { .. }
        | DaemonToClient::KeepAlive
        | DaemonToClient::Unknown => {}
    }
    return ControlFlow::Continue(());
}

#[cfg(test)]
#[path = "../tests/client/test_dispatch.rs"]
mod tests;
