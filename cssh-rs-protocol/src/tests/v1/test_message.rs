//! Unit tests for [`crate::v1::message`].

use super::*;
use crate::v1::keycode::{KeyCode, Modifiers};
use crate::v1::window::KIND_WINDOWS_HWND;
use ciborium::value::Value;

fn round_trip<T>(value: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf).expect("encode");
    let decoded: T = ciborium::de::from_reader(buf.as_slice()).expect("decode");
    assert_eq!(&decoded, value);
}

#[test]
fn test_daemon_to_client_round_trips_every_variant() {
    let variants = [
        DaemonToClient::Input {
            event: InputEvent::Key {
                code: KeyCode::Char("x".to_string()),
                modifiers: Modifiers::CTRL,
                text: None,
            },
        },
        DaemonToClient::StateChange {
            state: ClientRunState::Disabled,
        },
        DaemonToClient::Highlight { on: true },
        DaemonToClient::Activate {
            token: "tok-123".to_string(),
        },
        DaemonToClient::Terminate {
            reason: "daemon exit".to_string(),
        },
        DaemonToClient::Signal(SignalKind::Break),
        DaemonToClient::KeepAlive,
        DaemonToClient::Unknown,
    ];
    for message in &variants {
        round_trip(message);
    }
}

#[test]
fn test_client_to_daemon_round_trips_every_variant() {
    let window = WindowHandle {
        kind: KIND_WINDOWS_HWND.to_string(),
        value: Value::Integer(0x1234_i64.into()),
    };
    let variants = [
        ClientToDaemon::Ready {
            child_pid: 99,
            window: window.clone(),
        },
        ClientToDaemon::WindowChanged {
            window: window.clone(),
        },
        ClientToDaemon::ChildExited { code: -1 },
        ClientToDaemon::KeepAlive,
        ClientToDaemon::Unknown,
    ];
    for message in &variants {
        round_trip(message);
    }
}
