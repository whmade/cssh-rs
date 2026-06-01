//! macOS implementations of the `cssh-rs-platform` traits.
//!
//! Scaffolds for the macOS port. Every method panics with
//! `unimplemented!()` until M5 lands the concrete impls; the crate
//! exists so workspace consumers compile against the platform-
//! abstraction surface on macOS targets.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]
#![doc(html_no_source)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use std::ffi::{OsStr, OsString};

use cssh_rs_platform::{
    ControlChannelClient, ControlChannelServer, LaunchContext, ProcessSpawner, WindowHandleProbe,
};

/// Placeholder macOS launch context.
///
/// The real type will carry the Apple Event session identifier (or
/// equivalent NSWorkspace activation token) once M5 lands.
#[derive(Debug, Default)]
pub struct MacOsLaunchContext;

impl LaunchContext for MacOsLaunchContext {}

/// Placeholder macOS process spawner.
#[derive(Debug, Default)]
pub struct MacOsProcessSpawner;

impl ProcessSpawner for MacOsProcessSpawner {
    type Context = MacOsLaunchContext;
    type Handle = std::process::Child;
    type Error = std::io::Error;

    fn spawn(
        &self,
        _program: &OsStr,
        _args: &[OsString],
        _context: &Self::Context,
    ) -> Result<Self::Handle, Self::Error> {
        unimplemented!("cssh-rs-platform-macos: process spawn lands in M5");
    }
}

/// Placeholder macOS control-channel server endpoint.
#[derive(Debug, Default)]
pub struct MacOsControlChannelServer;

impl ControlChannelServer for MacOsControlChannelServer {
    type Error = std::io::Error;

    async fn accept(&mut self) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel accept lands in M5");
    }

    async fn send(&mut self, _frame: &[u8]) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel send lands in M5");
    }

    async fn recv(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel recv lands in M5");
    }
}

/// Placeholder macOS control-channel client endpoint.
#[derive(Debug, Default)]
pub struct MacOsControlChannelClient;

impl ControlChannelClient for MacOsControlChannelClient {
    type Error = std::io::Error;

    async fn connect(&mut self, _endpoint: &OsStr) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel connect lands in M5");
    }

    async fn send(&mut self, _bytes: &[u8]) -> Result<(), Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel send lands in M5");
    }

    async fn recv(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        unimplemented!("cssh-rs-platform-macos: control-channel recv lands in M5");
    }
}

/// Placeholder macOS window-handle probe.
///
/// `Handle` is `u64` to carry an `AXUIElement`-derived window number
/// (or equivalent Quartz `CGWindowID`); the concrete representation is
/// settled in M5 once the focus-tracking implementation is in place.
#[derive(Debug, Default)]
pub struct MacOsWindowHandleProbe;

impl WindowHandleProbe for MacOsWindowHandleProbe {
    type Handle = u64;

    fn window_handle_for_process(&self, _pid: u32) -> Option<Self::Handle> {
        unimplemented!("cssh-rs-platform-macos: window-handle probe lands in M5");
    }
}
