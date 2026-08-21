//! Unit tests for [`crate::v1::window`].

use super::*;

#[test]
fn test_window_handle_round_trips_through_cbor() {
    let handle = WindowHandle {
        kind: KIND_X11_WINDOW.to_string(),
        value: Value::Map(vec![
            (
                Value::Text("xid".to_string()),
                Value::Integer(4242_i64.into()),
            ),
            (
                Value::Text("display".to_string()),
                Value::Text(":0".to_string()),
            ),
        ]),
    };

    let mut buf = Vec::new();
    ciborium::ser::into_writer(&handle, &mut buf).expect("encode WindowHandle");
    let decoded: WindowHandle =
        ciborium::de::from_reader(buf.as_slice()).expect("decode WindowHandle");

    assert_eq!(decoded, handle);
}

#[test]
fn test_kind_constants_have_expected_values() {
    assert_eq!(KIND_WINDOWS_HWND, "windows.hwnd");
    assert_eq!(KIND_X11_WINDOW, "x11.window");
    assert_eq!(KIND_WAYLAND_APP_ID, "wayland.app_id");
    assert_eq!(KIND_MACOS_AX, "macos.ax");
    assert_eq!(KIND_MACOS_ITERM2, "macos.iterm2");
}
