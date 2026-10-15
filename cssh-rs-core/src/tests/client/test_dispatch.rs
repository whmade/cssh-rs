//! Unit tests for `dispatch_daemon_message`.

use std::ops::ControlFlow;
use std::sync::Mutex;

use tokio::sync::watch;
use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};

use cssh_rs_protocol::v1::input::{ClientRunState, InputEvent, SignalKind};
use cssh_rs_protocol::v1::message::DaemonToClient;
use cssh_rs_protocol::ClientState;

use crate::client::dispatch::dispatch_daemon_message;
use crate::utils::windows::MockWindowsApi;

const CHILD_PID: u32 = 4242;

/// Dispatch `message` and return the outcome and the bytes written to the master.
fn dispatch(
    api: &MockWindowsApi,
    message: DaemonToClient,
    state_sender: &watch::Sender<ClientState>,
    highlight_sender: &watch::Sender<bool>,
) -> (ControlFlow<()>, Vec<u8>) {
    let master = Mutex::new(Vec::new());
    let flow = dispatch_daemon_message(
        api,
        message,
        &master,
        CHILD_PID,
        state_sender,
        highlight_sender,
    );
    return (flow, master.into_inner().unwrap());
}

#[test]
fn test_input_is_written_to_master() {
    let (state_sender, _state_receiver) = watch::channel(ClientState::Active);
    let (highlight_sender, _highlight_receiver) = watch::channel(false);

    let (flow, written) = dispatch(
        &MockWindowsApi::new(),
        DaemonToClient::Input {
            event: InputEvent::Raw {
                bytes: b"a".to_vec(),
            },
        },
        &state_sender,
        &highlight_sender,
    );

    assert_eq!(flow, ControlFlow::Continue(()));
    assert_eq!(written, b"a");
}

#[test]
fn test_signals_are_relayed_to_child_process_group() {
    for (kind, expected_event) in [
        (SignalKind::Interrupt, CTRL_C_EVENT),
        (SignalKind::Break, CTRL_BREAK_EVENT),
    ] {
        let (state_sender, _state_receiver) = watch::channel(ClientState::Active);
        let (highlight_sender, _highlight_receiver) = watch::channel(false);
        let mut api = MockWindowsApi::new();
        api.expect_generate_console_ctrl_event()
            .withf(move |event, group| return *event == expected_event && *group == CHILD_PID)
            .times(1)
            .returning(|_, _| return Ok(()));

        let (flow, _) = dispatch(
            &api,
            DaemonToClient::Signal(kind),
            &state_sender,
            &highlight_sender,
        );

        assert_eq!(flow, ControlFlow::Continue(()));
    }
}

#[test]
fn test_state_and_highlight_reach_watch_channels() {
    let (state_sender, mut state_receiver) = watch::channel(ClientState::Active);
    let (highlight_sender, mut highlight_receiver) = watch::channel(false);
    let api = MockWindowsApi::new();

    for (run_state, expected_state) in [
        (ClientRunState::Disabled, ClientState::Disabled),
        (ClientRunState::Unknown, ClientState::Active),
    ] {
        let _ = dispatch(
            &api,
            DaemonToClient::StateChange { state: run_state },
            &state_sender,
            &highlight_sender,
        );
        assert_eq!(*state_receiver.borrow_and_update(), expected_state);
    }
    let _ = dispatch(
        &api,
        DaemonToClient::Highlight { on: true },
        &state_sender,
        &highlight_sender,
    );
    assert!(*highlight_receiver.borrow_and_update());
}

#[test]
fn test_terminate_breaks_and_other_messages_continue() {
    let (state_sender, _state_receiver) = watch::channel(ClientState::Active);
    let (highlight_sender, _highlight_receiver) = watch::channel(false);
    let api = MockWindowsApi::new();

    for (message, expected_flow) in [
        (
            DaemonToClient::Terminate {
                reason: "daemon exit".to_string(),
            },
            ControlFlow::Break(()),
        ),
        (
            DaemonToClient::Signal(SignalKind::Unknown),
            ControlFlow::Continue(()),
        ),
        (DaemonToClient::KeepAlive, ControlFlow::Continue(())),
    ] {
        let (flow, written) = dispatch(&api, message, &state_sender, &highlight_sender);

        assert_eq!(flow, expected_flow);
        assert!(written.is_empty());
    }
}
