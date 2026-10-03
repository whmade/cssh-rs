//! Unit tests for `dispatch_daemon_message`, driven with a `Vec<u8>` sink and
//! a `MockWindowsApi` so no PTY or console is ever touched.

use std::io::Write;
use std::sync::{Arc, Mutex};

use tokio::sync::watch;
use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};

use cssh_rs_protocol::v1::input::{ClientRunState, InputEvent, SignalKind};
use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
use cssh_rs_protocol::v1::message::DaemonToClient;

use crate::client::dispatch::{dispatch_daemon_message, Dispatch};
use crate::client::ClientState;
use crate::utils::windows::MockWindowsApi;

/// A `Write` sink that records everything into a shared buffer the test can
/// inspect after dispatch.
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

/// Build a master sink plus the shared buffer backing it.
#[allow(clippy::type_complexity)]
fn sink() -> (Mutex<Box<dyn Write + Send>>, Arc<Mutex<Vec<u8>>>) {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let writer: Box<dyn Write + Send> = Box::new(SharedSink(Arc::clone(&captured)));
    return (Mutex::new(writer), captured);
}

/// A `Write` sink whose every write fails, to drive the master write-error path.
struct FailingSink;

impl Write for FailingSink {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        return Err(std::io::Error::other("write failed"));
    }

    fn flush(&mut self) -> std::io::Result<()> {
        return Ok(());
    }
}

#[test]
fn input_key_writes_bytes_to_master() {
    let (master, captured) = sink();
    let (state_sender, _state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, _hl_rx) = watch::channel(false);
    let api = MockWindowsApi::new();

    let message = DaemonToClient::Input {
        event: InputEvent::Key {
            code: KeyCode::Char("a".to_string()),
            modifiers: Modifiers::NONE,
            text: Some("a".to_string()),
        },
    };
    let outcome = dispatch_daemon_message(
        &api,
        message,
        &master,
        1234,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(outcome, Dispatch::Continue);
    assert_eq!(&*captured.lock().expect("captured"), b"a");
}

#[test]
fn signal_break_relays_ctrl_break_to_child_group() {
    let (master, _captured) = sink();
    let (state_sender, _state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, _hl_rx) = watch::channel(false);

    let mut api = MockWindowsApi::new();
    api.expect_generate_console_ctrl_event()
        .withf(|event, group| return *event == CTRL_BREAK_EVENT && *group == 4242)
        .times(1)
        .returning(|_, _| return Ok(()));

    let outcome = dispatch_daemon_message(
        &api,
        DaemonToClient::Signal(SignalKind::Break),
        &master,
        4242,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(outcome, Dispatch::Continue);
}

#[test]
fn signal_interrupt_relays_ctrl_c_to_child_group() {
    let (master, _captured) = sink();
    let (state_sender, _state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, _hl_rx) = watch::channel(false);

    let mut api = MockWindowsApi::new();
    api.expect_generate_console_ctrl_event()
        .withf(|event, group| return *event == CTRL_C_EVENT && *group == 7)
        .times(1)
        .returning(|_, _| return Ok(()));

    dispatch_daemon_message(
        &api,
        DaemonToClient::Signal(SignalKind::Interrupt),
        &master,
        7,
        &state_sender,
        &highlight_sender,
    );
}

#[test]
fn state_change_and_highlight_reach_watch_channels() {
    let (master, _captured) = sink();
    let (state_sender, mut state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, mut highlight_rx) = watch::channel(false);
    let api = MockWindowsApi::new();

    dispatch_daemon_message(
        &api,
        DaemonToClient::StateChange {
            state: ClientRunState::Disabled,
        },
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(*state_rx.borrow_and_update(), ClientState::Disabled);

    dispatch_daemon_message(
        &api,
        DaemonToClient::Highlight { on: true },
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert!(*highlight_rx.borrow_and_update());
}

#[test]
fn terminate_returns_terminate() {
    let (master, _captured) = sink();
    let (state_sender, _state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, _hl_rx) = watch::channel(false);
    let api = MockWindowsApi::new();

    let outcome = dispatch_daemon_message(
        &api,
        DaemonToClient::Terminate {
            reason: "daemon exit".to_string(),
        },
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(outcome, Dispatch::Terminate);
}

#[test]
fn signal_unknown_is_a_noop() {
    let (master, _captured) = sink();
    let (state_sender, _state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, _hl_rx) = watch::channel(false);
    // No `generate_console_ctrl_event` expectation: an unknown signal must not
    // reach the child at all.
    let api = MockWindowsApi::new();

    let outcome = dispatch_daemon_message(
        &api,
        DaemonToClient::Signal(SignalKind::Unknown),
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(outcome, Dispatch::Continue);
}

#[test]
fn signal_relay_failure_is_swallowed() {
    let (master, _captured) = sink();
    let (state_sender, _state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, _hl_rx) = watch::channel(false);

    let mut api = MockWindowsApi::new();
    api.expect_generate_console_ctrl_event()
        .times(1)
        .returning(|_, _| return Err(windows::core::Error::from_thread()));

    // A failed relay is logged, not propagated: the client keeps running.
    let outcome = dispatch_daemon_message(
        &api,
        DaemonToClient::Signal(SignalKind::Interrupt),
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(outcome, Dispatch::Continue);
}

#[test]
fn state_change_active_maps_to_active() {
    let (master, _captured) = sink();
    let (state_sender, mut state_rx) = watch::channel(ClientState::Disabled);
    let (highlight_sender, _hl_rx) = watch::channel(false);
    let api = MockWindowsApi::new();

    dispatch_daemon_message(
        &api,
        DaemonToClient::StateChange {
            state: ClientRunState::Active,
        },
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(*state_rx.borrow_and_update(), ClientState::Active);

    // A forward-compatible unknown state also keeps input flowing (Active).
    dispatch_daemon_message(
        &api,
        DaemonToClient::StateChange {
            state: ClientRunState::Unknown,
        },
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(*state_rx.borrow_and_update(), ClientState::Active);
}

#[test]
fn master_write_failure_is_swallowed() {
    let writer: Box<dyn Write + Send> = Box::new(FailingSink);
    let master = Mutex::new(writer);
    let (state_sender, _state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, _hl_rx) = watch::channel(false);
    let api = MockWindowsApi::new();

    // A write error on the master is logged; dispatch still reports Continue.
    let outcome = dispatch_daemon_message(
        &api,
        DaemonToClient::Input {
            event: InputEvent::Key {
                code: KeyCode::Char("a".to_string()),
                modifiers: Modifiers::NONE,
                text: Some("a".to_string()),
            },
        },
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(outcome, Dispatch::Continue);
}

#[test]
fn poisoned_master_mutex_is_swallowed() {
    let (master, _captured) = sink();
    let (state_sender, _state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, _hl_rx) = watch::channel(false);
    let api = MockWindowsApi::new();

    // Poison the master mutex so the next lock returns a poison error.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let poison = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = master.lock().expect("lock");
        panic!("intentionally poison the master mutex");
    }));
    std::panic::set_hook(previous_hook);
    assert!(poison.is_err());

    // The poisoned lock is logged, not unwrapped: dispatch keeps running.
    let outcome = dispatch_daemon_message(
        &api,
        DaemonToClient::Input {
            event: InputEvent::Key {
                code: KeyCode::Char("a".to_string()),
                modifiers: Modifiers::NONE,
                text: Some("a".to_string()),
            },
        },
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(outcome, Dispatch::Continue);
}

#[test]
fn keepalive_is_a_noop() {
    let (master, captured) = sink();
    let (state_sender, _state_rx) = watch::channel(ClientState::Active);
    let (highlight_sender, _hl_rx) = watch::channel(false);
    let api = MockWindowsApi::new();

    let outcome = dispatch_daemon_message(
        &api,
        DaemonToClient::KeepAlive,
        &master,
        1,
        &state_sender,
        &highlight_sender,
    );
    assert_eq!(outcome, Dispatch::Continue);
    assert!(captured.lock().expect("captured").is_empty());
}
