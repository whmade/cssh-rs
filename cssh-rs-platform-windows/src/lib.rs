//! Windows implementation of the `cssh-rs-platform` traits.
//!
//! Hosts the [`api::WindowsApi`] abstraction over the raw `windows` crate
//! and the concrete implementations of the platform traits defined in
//! `cssh-rs-platform` ([`LaunchContext`], [`ProcessSpawner`],
//! [`ControlChannelServer`], [`ControlChannelClient`], [`WindowHandleProbe`]).
//!
//! The trait impls wrap existing [`api::WindowsApi`] helpers and the
//! `tokio::net::windows::named_pipe` types so the daemon and client can
//! eventually be ported to the platform-agnostic surface without behaviour
//! changes; today nothing in the workspace consumes the impls yet.
//!
//! All items in this crate are only available on Windows targets; on other
//! targets the crate compiles to an empty library so the workspace stays
//! buildable for `cargo check --target` of Linux and macOS hosts.
//!
//! [`LaunchContext`]: cssh_rs_platform::LaunchContext
//! [`ProcessSpawner`]: cssh_rs_platform::ProcessSpawner
//! [`ControlChannelServer`]: cssh_rs_platform::ControlChannelServer
//! [`ControlChannelClient`]: cssh_rs_platform::ControlChannelClient
//! [`WindowHandleProbe`]: cssh_rs_platform::WindowHandleProbe

#![deny(clippy::implicit_return)]
#![allow(
    clippy::needless_return,
    clippy::doc_overindented_list_items,
    rustdoc::private_intra_doc_links
)]
#![warn(missing_docs)]
#![doc(html_no_source)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[cfg(windows)]
pub mod api;
#[cfg(windows)]
pub mod traits;

/// Maximum expected length of window title of a client window.
///
/// Only used as fixed buffer size when reading the current window title to
/// check if we need to reset it. If the actual window title exceeds this
/// length it will be cut off at that point.
#[cfg(windows)]
pub const MAX_WINDOW_TITLE_LENGTH: usize = 2048;

#[cfg(windows)]
pub use api::{
    arrange_console, build_command_line, clear_screen, get_console_input_buffer,
    get_console_output_buffer, get_console_title, is_windows_10, read_console_input,
    read_keyboard_input, set_console_border_color, set_console_color, set_console_palette,
    snapshot_console_palette, tinted_palette, utf16_buffer_to_string, ConsolePaletteSnapshot,
    DefaultWindowsApi, WindowsApi, KEY_EVENT,
};

#[cfg(all(windows, any(test, feature = "mock")))]
pub use api::MockWindowsApi;
