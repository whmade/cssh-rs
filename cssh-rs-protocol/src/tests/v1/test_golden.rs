//! Golden-trace fixtures for the v1 CBOR wire format.
//!
//! Each fixture freezes the exact CBOR bytes of one real, wire-emitted v1
//! value, so an accidental encoding change (a renamed serde tag, a reordered
//! field, a changed integer width) fails CI. These complement the round-trip
//! tests in the sibling `test_*.rs` files: round-trip proves encode and decode
//! are inverses, golden proves the bytes themselves never drift.
//!
//! Coverage is exhaustive per wire enum: [`golden_variants`] pins one fixture
//! for every non-`Unknown` variant, and its arm list also feeds a compile-time
//! match, so a variant added later fails to build until it gets a fixture. The
//! `#[serde(other)] Unknown` catch-alls are omitted - they are decode-only and
//! never serialized onto the wire.
//!
//! Regenerating after an intentional wire change: run
//! `INSTA_UPDATE=always cargo test -p cssh-rs-protocol` (or `cargo insta
//! accept`), review the diff, and commit the updated `tests/golden/v1/*.snap`.

use serde::Serialize;

use crate::v1::handshake::{Hello, Welcome};
use crate::v1::input::{ClientRunState, InputEvent, SignalKind};
use crate::v1::keycode::{KeyCode, ModifierKeyCode, Modifiers};
use crate::v1::limits::DEFAULT_MAX_FRAME_LEN;
use crate::v1::message::{ClientToDaemon, DaemonToClient};
use crate::v1::version::{ProtocolVersion, Role};
use crate::v1::window::{WindowHandle, KIND_WINDOWS_HWND};

/// Encode a value to CBOR exactly as the wire codec does.
fn cbor_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf).expect("encode CBOR body");
    return buf;
}

/// Render bytes as an offset / hex / ASCII dump so a wire change surfaces as a
/// readable diff. The ASCII gutter exposes CBOR text (serde tags, field names,
/// string payloads), which is where a renamed tag would show.
fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (index, chunk) in bytes.chunks(16).enumerate() {
        let mut hex = String::new();
        let mut ascii = String::new();
        for &byte in chunk {
            hex.push_str(&format!("{byte:02x} "));
            let printable = (0x20..0x7f).contains(&byte);
            ascii.push(if printable { byte as char } else { '.' });
        }
        out.push_str(&format!("{:04x}  {hex:<48} |{ascii}|\n", index * 16));
    }
    return out;
}

/// Frozen text rendering of a value's CBOR body: the byte count and a hex dump.
fn golden<T: Serialize>(value: &T) -> String {
    let bytes = cbor_bytes(value);
    return format!("{} bytes\n{}", bytes.len(), hex_dump(&bytes));
}

/// A representative Windows `HWND` window handle for the client-to-daemon
/// fixtures.
fn windows_window() -> WindowHandle {
    return WindowHandle {
        kind: KIND_WINDOWS_HWND.to_string(),
        value: ciborium::value::Value::Integer(0x000A_1234_i64.into()),
    };
}

/// Freeze one golden fixture per non-`Unknown` variant of a wire enum.
///
/// The arm list feeds both the emitted fixtures and a compile-time exhaustive
/// match (`$excluded` is always the decode-only `Unknown` catch-all), so a
/// variant added later fails to build until it is listed here with a fixture.
macro_rules! golden_variants {
    (
        $prefix:literal, $ty:ty, excluded: $excluded:pat,
        { $( $pattern:pat => $slug:literal : $value:expr ),+ $(,)? }
    ) => {{
        const _: fn(&$ty) = |value| {
            match value {
                $( $pattern => {} , )+
                $excluded => {}
            };
        };
        $( insta::assert_snapshot!(concat!($prefix, "_", $slug), golden(&$value)); )+
    }};
}

#[test]
fn test_golden_message_catalog() {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("../../../tests/golden/v1");
    settings.set_prepend_module_to_snapshot(false);
    // A golden fixture is a function of the wire bytes only; drop insta's
    // source-expression header so reformatting the test never churns fixtures.
    settings.set_omit_expression(true);
    settings.bind(|| {
        golden_variants!("dtc", DaemonToClient, excluded: DaemonToClient::Unknown, {
            DaemonToClient::Input { .. } => "input": DaemonToClient::Input {
                event: InputEvent::Key {
                    code: KeyCode::Char("a".to_string()),
                    modifiers: Modifiers::NONE,
                    text: None,
                },
            },
            DaemonToClient::StateChange { .. } => "state_change": DaemonToClient::StateChange {
                state: ClientRunState::Disabled,
            },
            DaemonToClient::Highlight { .. } => "highlight": DaemonToClient::Highlight { on: true },
            DaemonToClient::Activate { .. } => "activate": DaemonToClient::Activate {
                token: "wm-activation-token".to_string(),
            },
            DaemonToClient::Terminate { .. } => "terminate": DaemonToClient::Terminate {
                reason: "daemon shutting down".to_string(),
            },
            DaemonToClient::Signal(_) => "signal": DaemonToClient::Signal(SignalKind::Break),
            DaemonToClient::KeepAlive => "keep_alive": DaemonToClient::KeepAlive,
        });

        golden_variants!("ctd", ClientToDaemon, excluded: ClientToDaemon::Unknown, {
            ClientToDaemon::Ready { .. } => "ready": ClientToDaemon::Ready {
                child_pid: 4321,
                window: windows_window(),
            },
            ClientToDaemon::WindowChanged { .. } => "window_changed": ClientToDaemon::WindowChanged {
                window: windows_window(),
            },
            ClientToDaemon::ChildExited { .. } => "child_exited": ClientToDaemon::ChildExited {
                code: -1,
            },
            ClientToDaemon::KeepAlive => "keep_alive": ClientToDaemon::KeepAlive,
        });

        golden_variants!("input", InputEvent, excluded: InputEvent::Unknown, {
            InputEvent::Key { .. } => "key": InputEvent::Key {
                code: KeyCode::Char("a".to_string()),
                modifiers: Modifiers::NONE,
                text: None,
            },
            InputEvent::Raw { .. } => "raw": InputEvent::Raw {
                bytes: vec![0x1b, 0x5b, 0x41],
            },
        });

        golden_variants!("keycode", KeyCode, excluded: KeyCode::Unknown, {
            KeyCode::Char(_) => "char": KeyCode::Char("a".to_string()),
            KeyCode::Enter => "enter": KeyCode::Enter,
            KeyCode::Escape => "escape": KeyCode::Escape,
            KeyCode::Tab => "tab": KeyCode::Tab,
            KeyCode::Backspace => "backspace": KeyCode::Backspace,
            KeyCode::Delete => "delete": KeyCode::Delete,
            KeyCode::Insert => "insert": KeyCode::Insert,
            KeyCode::Up => "up": KeyCode::Up,
            KeyCode::Down => "down": KeyCode::Down,
            KeyCode::Left => "left": KeyCode::Left,
            KeyCode::Right => "right": KeyCode::Right,
            KeyCode::Home => "home": KeyCode::Home,
            KeyCode::End => "end": KeyCode::End,
            KeyCode::PageUp => "page_up": KeyCode::PageUp,
            KeyCode::PageDown => "page_down": KeyCode::PageDown,
            KeyCode::F(_) => "f": KeyCode::F(5),
            KeyCode::Modifier(_) => "modifier": KeyCode::Modifier(ModifierKeyCode::RightAlt),
        });

        golden_variants!("modifierkeycode", ModifierKeyCode, excluded: ModifierKeyCode::Unknown, {
            ModifierKeyCode::LeftShift => "left_shift": ModifierKeyCode::LeftShift,
            ModifierKeyCode::RightShift => "right_shift": ModifierKeyCode::RightShift,
            ModifierKeyCode::LeftControl => "left_control": ModifierKeyCode::LeftControl,
            ModifierKeyCode::RightControl => "right_control": ModifierKeyCode::RightControl,
            ModifierKeyCode::LeftAlt => "left_alt": ModifierKeyCode::LeftAlt,
            ModifierKeyCode::RightAlt => "right_alt": ModifierKeyCode::RightAlt,
            ModifierKeyCode::LeftMeta => "left_meta": ModifierKeyCode::LeftMeta,
            ModifierKeyCode::RightMeta => "right_meta": ModifierKeyCode::RightMeta,
        });

        golden_variants!("clientrunstate", ClientRunState, excluded: ClientRunState::Unknown, {
            ClientRunState::Active => "active": ClientRunState::Active,
            ClientRunState::Disabled => "disabled": ClientRunState::Disabled,
        });

        golden_variants!("role", Role, excluded: Role::Unknown, {
            Role::Daemon => "daemon": Role::Daemon,
            Role::Client => "client": Role::Client,
        });

        golden_variants!("signalkind", SignalKind, excluded: SignalKind::Unknown, {
            SignalKind::Break => "break": SignalKind::Break,
            SignalKind::Interrupt => "interrupt": SignalKind::Interrupt,
        });

        // Encoding details not tied to a single variant: the optional `text`
        // field (IME / dead-key composed input, multi-byte UTF-8) and a
        // multi-bit `Modifiers` value.
        insta::assert_snapshot!(
            "dtc_input_key_modifiers_text",
            golden(&DaemonToClient::Input {
                event: InputEvent::Key {
                    code: KeyCode::Char("e".to_string()),
                    modifiers: Modifiers::CTRL | Modifiers::ALT,
                    text: Some("\u{00e9}".to_string()),
                },
            })
        );

        insta::assert_snapshot!(
            "hello",
            golden(&Hello {
                protocol_version: ProtocolVersion::new(1, 0),
                role: Role::Client,
                pid: 4321,
                capabilities: vec!["paste".to_string(), "highlight".to_string()],
                max_frame_len: DEFAULT_MAX_FRAME_LEN,
            })
        );
        insta::assert_snapshot!(
            "welcome",
            golden(&Welcome {
                client_id: 7,
                server_capabilities: vec!["paste".to_string()],
            })
        );
    });
}
