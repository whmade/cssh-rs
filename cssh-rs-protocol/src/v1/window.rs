//! Opaque, platform-tagged identifier for a client's terminal window.

use ciborium::value::Value;
use serde_derive::{Deserialize, Serialize};

/// Handle kind for a Windows console window; `value` is the `HWND` as a `u64`.
pub const KIND_WINDOWS_HWND: &str = "windows.hwnd";

/// Handle kind for an X11 window; `value` is `{ xid: u64, display: String }`.
pub const KIND_X11_WINDOW: &str = "x11.window";

/// Handle kind for a Wayland toplevel; `value` is
/// `{ app_id: String, token: Option<String> }`.
pub const KIND_WAYLAND_APP_ID: &str = "wayland.app_id";

/// Handle kind for a macOS Accessibility window; `value` is
/// `{ pid: u32, ax_title: String }`.
pub const KIND_MACOS_AX: &str = "macos.ax";

/// Handle kind for an iTerm2 session; `value` is `{ session_id: String }`.
pub const KIND_MACOS_ITERM2: &str = "macos.iterm2";

/// Opaque, platform-tagged handle naming a client's terminal window.
///
/// The daemon stores and forwards a `WindowHandle` without ever inspecting
/// `value`; only the client that produced it and the window-manager plugin
/// that consumes it interpret the payload for a given `kind`. See the
/// `KIND_*` constants for the payload shape of each kind defined at v1.0.
///
/// Because `value` is a self-describing CBOR tree
/// ([`ciborium::value::Value`]), which holds no `Eq`, this type derives
/// `PartialEq` only; do not add `Eq` or `Hash`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowHandle {
    /// Namespaced discriminator naming the payload schema, e.g.
    /// [`KIND_WINDOWS_HWND`]. Determines how `value` is interpreted.
    pub kind: String,
    /// Opaque platform payload; the daemon never introspects it.
    pub value: Value,
}

impl WindowHandle {
    /// Construct a [`KIND_WINDOWS_HWND`] handle wrapping a raw `HWND` value.
    ///
    /// # Arguments
    /// * `hwnd` - The Windows window handle as a `u64`.
    ///
    /// # Returns
    /// A `WindowHandle` whose `value` is the handle encoded as a CBOR integer.
    pub fn windows_hwnd(hwnd: u64) -> Self {
        return Self {
            kind: KIND_WINDOWS_HWND.to_string(),
            value: Value::from(hwnd),
        };
    }

    /// Return the raw `HWND` value of a [`KIND_WINDOWS_HWND`] handle.
    ///
    /// # Returns
    /// `Some(hwnd)` when this is a Windows handle with an integer payload in
    /// `u64` range, otherwise `None`.
    pub fn as_windows_hwnd(&self) -> Option<u64> {
        if self.kind != KIND_WINDOWS_HWND {
            return None;
        }
        return self
            .value
            .as_integer()
            .and_then(|integer| return u64::try_from(integer).ok());
    }
}

#[cfg(test)]
#[path = "../tests/v1/test_window.rs"]
mod tests;
