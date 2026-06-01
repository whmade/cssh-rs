//! Linux implementations of the `cssh-rs-platform` traits.
//!
//! Scaffolds for the Linux/Wayland port. Every method panics with
//! `unimplemented!()` until M4 lands the concrete impls; the crate
//! exists so workspace consumers compile against the platform-
//! abstraction surface on Linux targets.
//!
//! All items in this crate are only available on Linux targets; on other
//! targets the crate compiles to an empty library so the workspace stays
//! buildable for `cargo check --target` of Windows and macOS hosts.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]
#![doc(html_no_source)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[cfg(target_os = "linux")]
use std::ffi::{OsStr, OsString};

#[cfg(target_os = "linux")]
use cssh_rs_platform::{
    ControlChannelClient, ControlChannelServer, LaunchContext, ProcessSpawner, WindowHandleProbe,
};

/// Placeholder Linux launch context.
///
/// The real type will carry the Wayland activation token (or equivalent
/// X11 startup id) once M4 lands.
#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub struct LinuxLaunchContext;

#[cfg(target_os = "linux")]
impl LaunchContext for LinuxLaunchContext {}

/// Placeholder Linux process spawner.
#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub struct LinuxProcessSpawner;

#[cfg(target_os = "linux")]
#[cfg_attr(coverage_nightly, coverage(off))]
impl ProcessSpawner for LinuxProcessSpawner {
    type Context = LinuxLaunchContext;
    type Handle = std::process::Child;
    type Error = std::io::Error;

    fn spawn(
        &self,
        _program: &OsStr,
        _args: &[OsString],
        _context: &Self::Context,
    ) -> Result<Self::Handle, Self::Error> {
        unimplemented!("cssh-rs-platform-linux: process spawn lands in M4");
    }
}

/// Placeholder Linux control-channel server endpoint.
#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub struct LinuxControlChannelServer;

#[cfg(target_os = "linux")]
#[cfg_attr(coverage_nightly, coverage(off))]
impl ControlChannelServer for LinuxControlChannelServer {
    type Error = std::io::Error;

    async fn accept(&mut self) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-linux: control-channel accept lands in M4");
    }

    async fn send(&mut self, _frame: &[u8]) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-linux: control-channel send lands in M4");
    }

    async fn recv(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        unimplemented!("cssh-rs-platform-linux: control-channel recv lands in M4");
    }
}

/// Placeholder Linux control-channel client endpoint.
#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub struct LinuxControlChannelClient;

#[cfg(target_os = "linux")]
#[cfg_attr(coverage_nightly, coverage(off))]
impl ControlChannelClient for LinuxControlChannelClient {
    type Error = std::io::Error;

    async fn connect(&mut self, _endpoint: &OsStr) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-linux: control-channel connect lands in M4");
    }

    async fn send(&mut self, _bytes: &[u8]) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-linux: control-channel send lands in M4");
    }

    async fn recv(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        unimplemented!("cssh-rs-platform-linux: control-channel recv lands in M4");
    }
}

/// Placeholder Linux window-handle probe.
///
/// `Handle` is `u64` to leave room for both X11 window XIDs (`u32`) and
/// Wayland surface ids (`u64`); the concrete representation is settled
/// in M4 once the focus-tracking implementation is in place.
#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub struct LinuxWindowHandleProbe;

#[cfg(target_os = "linux")]
#[cfg_attr(coverage_nightly, coverage(off))]
impl WindowHandleProbe for LinuxWindowHandleProbe {
    type Handle = u64;

    fn window_handle_for_process(&self, _pid: u32) -> Option<Self::Handle> {
        unimplemented!("cssh-rs-platform-linux: window-handle probe lands in M4");
    }
}
